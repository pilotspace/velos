//! Map tile rendering pipeline: PMTiles -> MVT decode -> earcut triangulation -> wgpu render.
//!
//! Provides `MapTileRenderer` which loads OSM vector tiles from a local PMTiles file,
//! decodes MVT protobuf on a background thread, triangulates polygons with earcut,
//! and renders colored geometry as a background layer behind simulation agents.
//!
//! Thread model:
//! - Background thread: owns PMTiles reader, does file I/O + MVT decode + triangulation
//! - Main thread: receives decoded vertex data, creates wgpu buffers, renders

use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::mpsc;

use bytemuck::{Pod, Zeroable};
use lru::LruCache;
use velos_net::projection::EquirectangularProjection;
use wgpu::util::DeviceExt;

use crate::camera::Camera2D;

/// HCMC projection center (WGS84).
const CENTER_LAT: f64 = 10.7756;
const CENTER_LON: f64 = 106.7019;

/// Default LRU cache capacity (number of decoded tiles).
const TILE_CACHE_CAPACITY: usize = 128;

/// Per-vertex data for map tile geometry.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TileVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

impl TileVertex {
    /// Vertex buffer layout for map tile vertices.
    pub fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TileVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// A decoded tile ready for GPU upload (sent from background thread to main).
pub struct DecodedTile {
    pub z: u8,
    pub x: u64,
    pub y: u64,
    pub vertices: Vec<TileVertex>,
}

/// GPU-resident tile data.
struct GpuTile {
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
}

/// Layer colors for map tile features (RGBA, dark theme).
struct LayerColors;

impl LayerColors {
    const BUILDING: [f32; 4] = [0.20, 0.18, 0.22, 0.8];
    const WATER: [f32; 4] = [0.15, 0.25, 0.40, 0.9];
    const ROAD: [f32; 4] = [0.25, 0.25, 0.30, 0.6];
    const PARK: [f32; 4] = [0.15, 0.25, 0.15, 0.7];
    const LANDUSE: [f32; 4] = [0.18, 0.17, 0.20, 0.5];
}

/// Map camera zoom to tile zoom level.
pub fn camera_zoom_to_tile_zoom(camera_zoom: f32) -> u8 {
    if camera_zoom < 0.5 {
        14
    } else if camera_zoom < 2.0 {
        15
    } else {
        16
    }
}

/// Compute visible tile coordinates for the given camera viewport.
///
/// Converts camera world-space bounds (local metres) back to lon/lat,
/// then to slippy map tile x/y at the given zoom level.
pub fn visible_tiles(camera: &Camera2D, zoom: u8) -> Vec<(u8, u64, u64)> {
    let proj = EquirectangularProjection::new(CENTER_LAT, CENTER_LON);

    let half_w = camera.viewport.x / (2.0 * camera.zoom);
    let half_h = camera.viewport.y / (2.0 * camera.zoom);

    let min_x = camera.center.x - half_w;
    let max_x = camera.center.x + half_w;
    let min_y = camera.center.y - half_h;
    let max_y = camera.center.y + half_h;

    // Convert world metres back to lat/lon
    let (min_lat, min_lon) = proj.unproject(min_x as f64, min_y as f64);
    let (max_lat, max_lon) = proj.unproject(max_x as f64, max_y as f64);

    // Convert lon/lat to tile x/y using slippy map math
    let n = 2_u64.pow(zoom as u32);
    let n_f = n as f64;

    let tx_min = lonlat_to_tile_x(min_lon, n_f);
    let tx_max = lonlat_to_tile_x(max_lon, n_f);
    let ty_min = lonlat_to_tile_y(max_lat, n_f); // max_lat -> smaller ty (north is top)
    let ty_max = lonlat_to_tile_y(min_lat, n_f);

    let tx_lo = tx_min.min(tx_max);
    let tx_hi = tx_min.max(tx_max).min(n.saturating_sub(1));
    let ty_lo = ty_min.min(ty_max);
    let ty_hi = ty_min.max(ty_max).min(n.saturating_sub(1));

    let mut tiles = Vec::new();
    for ty in ty_lo..=ty_hi {
        for tx in tx_lo..=tx_hi {
            tiles.push((zoom, tx, ty));
        }
    }
    tiles
}

/// Convert longitude to slippy tile X coordinate.
fn lonlat_to_tile_x(lon: f64, n: f64) -> u64 {
    ((lon + 180.0) / 360.0 * n).floor().max(0.0) as u64
}

/// Convert latitude to slippy tile Y coordinate.
fn lonlat_to_tile_y(lat: f64, n: f64) -> u64 {
    let lat_rad = lat.to_radians();
    ((1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n)
        .floor()
        .max(0.0) as u64
}

/// Convert MVT tile-local coordinates to VELOS local metres.
///
/// MVT coordinates are in tile-local space [0, extent). Convert to
/// Web Mercator lon/lat, then to VELOS local metres.
pub fn tile_coord_to_local(
    tile_x: f32,
    tile_y: f32,
    z: u8,
    tx: u64,
    ty: u64,
    extent: u32,
    proj: &EquirectangularProjection,
) -> (f32, f32) {
    let n = 2_f64.powi(z as i32);
    let extent_f = extent as f64;

    // Tile-local to fractional tile coordinates
    let frac_x = tx as f64 + tile_x as f64 / extent_f;
    let frac_y = ty as f64 + tile_y as f64 / extent_f;

    // Fractional tile to lon/lat (inverse slippy map)
    let lon = frac_x / n * 360.0 - 180.0;
    let lat_rad = (std::f64::consts::PI * (1.0 - 2.0 * frac_y / n)).sinh().atan();
    let lat = lat_rad.to_degrees();

    // WGS84 to local metres
    let (x, y) = proj.project(lat, lon);
    (x as f32, y as f32)
}

/// Color for a given MVT layer name.
fn layer_color(name: &str) -> Option<[f32; 4]> {
    match name {
        "building" | "buildings" => Some(LayerColors::BUILDING),
        "water" | "waterway" => Some(LayerColors::WATER),
        "transportation" | "road" | "roads" | "highway" => Some(LayerColors::ROAD),
        "park" | "green" | "landcover" => Some(LayerColors::PARK),
        "landuse" | "land" | "residential" => Some(LayerColors::LANDUSE),
        _ => None, // skip unknown layers (labels, POIs, etc.)
    }
}

/// Triangulate polygon exterior ring and holes using earcut.
///
/// Returns triangle vertex positions as flat (x, y) pairs.
fn triangulate_polygon(
    exterior: &[(f32, f32)],
    holes: &[Vec<(f32, f32)>],
) -> Vec<(f32, f32)> {
    if exterior.len() < 3 {
        return Vec::new();
    }

    let mut coords: Vec<f64> = Vec::with_capacity((exterior.len() + holes.iter().map(|h| h.len()).sum::<usize>()) * 2);
    let mut hole_indices: Vec<usize> = Vec::new();

    // Add exterior ring
    for &(x, y) in exterior {
        coords.push(x as f64);
        coords.push(y as f64);
    }

    // Add holes
    for hole in holes {
        hole_indices.push(coords.len() / 2);
        for &(x, y) in hole {
            coords.push(x as f64);
            coords.push(y as f64);
        }
    }

    let indices = earcutr::earcut(&coords, &hole_indices, 2).unwrap_or_default();

    let mut result = Vec::with_capacity(indices.len());
    for idx in indices {
        let x = coords[idx * 2] as f32;
        let y = coords[idx * 2 + 1] as f32;
        result.push((x, y));
    }
    result
}

/// Generate thin quad (2 triangles) for a line segment.
fn line_segment_to_quads(
    p0: (f32, f32),
    p1: (f32, f32),
    half_width: f32,
) -> [(f32, f32); 6] {
    let dx = p1.0 - p0.0;
    let dy = p1.1 - p0.1;
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let nx = -dy / len * half_width;
    let ny = dx / len * half_width;

    let a = (p0.0 + nx, p0.1 + ny);
    let b = (p0.0 - nx, p0.1 - ny);
    let c = (p1.0 - nx, p1.1 - ny);
    let d = (p1.0 + nx, p1.1 + ny);

    [a, b, c, a, c, d]
}

/// Decode MVT tile bytes into vertices with reprojected coordinates.
///
/// This runs on the background thread (no GPU access).
pub fn decode_mvt_tile(
    data: &[u8],
    z: u8,
    tx: u64,
    ty: u64,
) -> Vec<TileVertex> {
    let proj = EquirectangularProjection::new(CENTER_LAT, CENTER_LON);

    let reader = match mvt_reader::Reader::new(data.to_vec()) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("MVT decode error for tile {z}/{tx}/{ty}: {e}");
            return Vec::new();
        }
    };

    let layer_meta = match reader.get_layer_metadata() {
        Ok(m) => m,
        Err(e) => {
            log::warn!("MVT layer metadata error for tile {z}/{tx}/{ty}: {e}");
            return Vec::new();
        }
    };

    let mut vertices = Vec::new();

    for meta in &layer_meta {
        let color = match layer_color(&meta.name) {
            Some(c) => c,
            None => continue,
        };

        let ctx = TileContext {
            z,
            tx,
            ty,
            extent: meta.extent,
            proj: &proj,
        };

        let features = match reader.get_features(meta.layer_index) {
            Ok(f) => f,
            Err(_) => continue,
        };

        for feature in &features {
            let geom = feature.get_geometry();
            process_geometry(geom, &ctx, color, &mut vertices);
        }
    }

    vertices
}

/// Tile coordinate context bundled to reduce argument count.
struct TileContext<'a> {
    z: u8,
    tx: u64,
    ty: u64,
    extent: u32,
    proj: &'a EquirectangularProjection,
}

/// Process a geo_types::Geometry and produce TileVertex data.
fn process_geometry(
    geom: &geo_types::Geometry<f32>,
    ctx: &TileContext<'_>,
    color: [f32; 4],
    vertices: &mut Vec<TileVertex>,
) {
    use geo_types::Geometry;

    match geom {
        Geometry::Polygon(poly) => {
            process_polygon(poly, ctx, color, vertices);
        }
        Geometry::MultiPolygon(mp) => {
            for poly in mp.0.iter() {
                process_polygon(poly, ctx, color, vertices);
            }
        }
        Geometry::LineString(ls) => {
            process_linestring(ls, ctx, color, vertices);
        }
        Geometry::MultiLineString(mls) => {
            for ls in mls.0.iter() {
                process_linestring(ls, ctx, color, vertices);
            }
        }
        Geometry::GeometryCollection(gc) => {
            for g in gc.0.iter() {
                process_geometry(g, ctx, color, vertices);
            }
        }
        _ => {} // Skip points, etc.
    }
}

/// Process a single polygon: triangulate and reproject vertices.
fn process_polygon(
    poly: &geo_types::Polygon<f32>,
    ctx: &TileContext<'_>,
    color: [f32; 4],
    vertices: &mut Vec<TileVertex>,
) {
    let exterior: Vec<(f32, f32)> = poly
        .exterior()
        .coords()
        .map(|c| (c.x, c.y))
        .collect();
    let holes: Vec<Vec<(f32, f32)>> = poly
        .interiors()
        .iter()
        .map(|ring: &geo_types::LineString<f32>| ring.coords().map(|c| (c.x, c.y)).collect())
        .collect();

    let triangulated = triangulate_polygon(&exterior, &holes);
    for (tile_x, tile_y) in triangulated {
        let (wx, wy) = tile_coord_to_local(tile_x, tile_y, ctx.z, ctx.tx, ctx.ty, ctx.extent, ctx.proj);
        vertices.push(TileVertex {
            position: [wx, wy],
            color,
        });
    }
}

/// Process a linestring: generate thin quads for each segment.
fn process_linestring(
    ls: &geo_types::LineString<f32>,
    ctx: &TileContext<'_>,
    color: [f32; 4],
    vertices: &mut Vec<TileVertex>,
) {
    let points: Vec<(f32, f32)> = ls
        .coords()
        .map(|c| {
            tile_coord_to_local(c.x, c.y, ctx.z, ctx.tx, ctx.ty, ctx.extent, ctx.proj)
        })
        .collect();

    for pair in points.windows(2) {
        let quad = line_segment_to_quads(pair[0], pair[1], 0.5); // 1m total width
        for (x, y) in quad {
            vertices.push(TileVertex {
                position: [x, y],
                color,
            });
        }
    }
}

/// Map tile renderer: PMTiles -> MVT decode -> wgpu render pipeline.
///
/// Manages background tile decode thread, LRU cache of GPU tile buffers,
/// and wgpu render pipeline for colored polygon geometry.
pub struct MapTileRenderer {
    /// LRU cache of GPU tile buffers: key = (z, x, y).
    tile_cache: LruCache<(u8, u64, u64), GpuTile>,
    /// Receiver for decoded tiles from background thread.
    decoded_rx: mpsc::Receiver<DecodedTile>,
    /// Sender to request tile decoding on background thread.
    request_tx: mpsc::Sender<(u8, u64, u64)>,
    /// Set of tiles currently being decoded (avoid duplicate requests).
    pending: HashSet<(u8, u64, u64)>,
    /// wgpu render pipeline for map tile geometry.
    pipeline: wgpu::RenderPipeline,
    /// Set of currently visible tile keys for draw filtering.
    visible_set: HashSet<(u8, u64, u64)>,
}

impl MapTileRenderer {
    /// Create a new MapTileRenderer.
    ///
    /// If `pmtiles_path` is None or the file does not exist, the renderer
    /// will be inert (no tiles loaded, no background thread work).
    ///
    /// # Arguments
    /// - `device`: wgpu device for pipeline/buffer creation
    /// - `surface_format`: texture format for the render pipeline
    /// - `camera_bind_group_layout`: layout from Renderer (shared camera uniform)
    /// - `pmtiles_path`: optional path to local .pmtiles file
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        pmtiles_path: Option<&Path>,
    ) -> Self {
        let pipeline = Self::create_pipeline(device, surface_format, camera_bind_group_layout);

        let (request_tx, request_rx) = mpsc::channel::<(u8, u64, u64)>();
        let (decoded_tx, decoded_rx) = mpsc::channel::<DecodedTile>();

        // Spawn background decode thread
        let path_owned = pmtiles_path.map(|p| p.to_path_buf());
        std::thread::Builder::new()
            .name("map-tile-decode".into())
            .spawn(move || {
                Self::background_decode_loop(path_owned, request_rx, decoded_tx);
            })
            .expect("Failed to spawn map tile decode thread");

        Self {
            tile_cache: LruCache::new(NonZeroUsize::new(TILE_CACHE_CAPACITY).unwrap()),
            decoded_rx,
            request_tx,
            pending: HashSet::new(),
            pipeline,
            visible_set: HashSet::new(),
        }
    }

    /// Background thread: reads PMTiles, decodes MVT, sends vertices back.
    fn background_decode_loop(
        path: Option<std::path::PathBuf>,
        request_rx: mpsc::Receiver<(u8, u64, u64)>,
        decoded_tx: mpsc::Sender<DecodedTile>,
    ) {
        use std::io::BufReader;

        let mut pmtiles = match path {
            Some(ref p) if p.exists() => {
                match std::fs::File::open(p) {
                    Ok(file) => {
                        match pmtiles2::PMTiles::from_reader(BufReader::new(file)) {
                            Ok(pm) => Some(pm),
                            Err(e) => {
                                log::warn!("Failed to open PMTiles: {e}");
                                None
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to open PMTiles file: {e}");
                        None
                    }
                }
            }
            _ => None,
        };

        while let Ok((z, x, y)) = request_rx.recv() {
            let tile_data = match pmtiles.as_mut() {
                Some(pm) => match pm.get_tile(x, y, z) {
                    Ok(Some(data)) => data,
                    Ok(None) => {
                        log::debug!("No tile data for {z}/{x}/{y}");
                        continue;
                    }
                    Err(e) => {
                        log::warn!("PMTiles read error for {z}/{x}/{y}: {e}");
                        continue;
                    }
                },
                None => continue,
            };

            // Try gzip decompress first, fall back to raw bytes
            let decompressed = decompress_tile(&tile_data);

            let vertices = decode_mvt_tile(&decompressed, z, x, y);

            if decoded_tx
                .send(DecodedTile { z, x, y, vertices })
                .is_err()
            {
                break; // Main thread dropped receiver
            }
        }
    }

    /// Create the wgpu render pipeline for map tile geometry.
    fn create_pipeline(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::include_wgsl!("../shaders/map_tile.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("map_tile_pipeline_layout"),
            bind_group_layouts: &[camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("map_tile_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[TileVertex::vertex_buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    /// Called each frame: receive decoded tiles, request new visible tiles.
    ///
    /// GPU buffer creation happens here (main thread only).
    pub fn update(&mut self, camera: &Camera2D, device: &wgpu::Device) {
        // 1. Receive decoded tiles from background thread, create GPU buffers
        while let Ok(decoded) = self.decoded_rx.try_recv() {
            let key = (decoded.z, decoded.x, decoded.y);
            self.pending.remove(&key);

            if decoded.vertices.is_empty() {
                continue;
            }

            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("tile_{}_{}_{}", decoded.z, decoded.x, decoded.y)),
                contents: bytemuck::cast_slice(&decoded.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            self.tile_cache.put(
                key,
                GpuTile {
                    vertex_buffer,
                    vertex_count: decoded.vertices.len() as u32,
                },
            );
        }

        // 2. Compute visible tiles
        let tile_zoom = camera_zoom_to_tile_zoom(camera.zoom);
        let visible = visible_tiles(camera, tile_zoom);

        self.visible_set.clear();
        for &(z, x, y) in &visible {
            self.visible_set.insert((z, x, y));

            // Touch cached tiles to prevent eviction
            let _ = self.tile_cache.get(&(z, x, y));

            // Request tiles not in cache and not pending
            if !self.tile_cache.contains(&(z, x, y)) && !self.pending.contains(&(z, x, y)) {
                self.pending.insert((z, x, y));
                let _ = self.request_tx.send((z, x, y));
            }
        }
    }

    /// Render all visible cached tiles.
    ///
    /// Must be called with LoadOp::Load (tiles render BEFORE agents clear the buffer).
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        camera_bind_group: &wgpu::BindGroup,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("map_tile_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.05,
                        g: 0.05,
                        b: 0.1,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera_bind_group, &[]);

        for (&key, tile) in self.tile_cache.iter() {
            if !self.visible_set.contains(&key) {
                continue;
            }
            if tile.vertex_count > 0 {
                pass.set_vertex_buffer(0, tile.vertex_buffer.slice(..));
                pass.draw(0..tile.vertex_count, 0..1);
            }
        }
    }

    /// Number of tiles currently in the GPU cache.
    pub fn cached_tile_count(&self) -> usize {
        self.tile_cache.len()
    }
}

/// Try gzip decompression; if it fails, return original data.
fn decompress_tile(data: &[u8]) -> Vec<u8> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    match decoder.read_to_end(&mut decompressed) {
        Ok(_) => decompressed,
        Err(_) => data.to_vec(), // Not gzip-compressed, use raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    /// Camera at HCMC center with zoom 2.0 should return tiles at zoom level 15.
    #[test]
    fn visible_tiles_hcmc_center() {
        let camera = Camera2D::new(Vec2::new(1280.0, 720.0));
        // center at (0, 0) local metres = HCMC center
        let tiles = visible_tiles(&camera, 15);
        assert!(!tiles.is_empty(), "Should find visible tiles");

        // All tiles should be zoom level 15
        for &(z, _x, _y) in &tiles {
            assert_eq!(z, 15);
        }

        // HCMC center at zoom 15: approx tile x=25852, y=15388
        // With viewport 1280x720 at zoom=1.0, visible area is ~1280m x 720m
        // At zoom 15, each tile is ~1222m wide (at HCMC latitude)
        // So we expect ~2x2 tiles
        assert!(
            tiles.len() >= 1 && tiles.len() <= 9,
            "Expected 1-9 tiles for 1280x720 viewport at zoom 1.0, got {}",
            tiles.len()
        );
    }

    /// Camera zoom mapping to tile zoom level.
    #[test]
    fn camera_zoom_mapping() {
        assert_eq!(camera_zoom_to_tile_zoom(0.3), 14);
        assert_eq!(camera_zoom_to_tile_zoom(0.5), 15);
        assert_eq!(camera_zoom_to_tile_zoom(1.0), 15);
        assert_eq!(camera_zoom_to_tile_zoom(2.0), 16);
        assert_eq!(camera_zoom_to_tile_zoom(5.0), 16);
        assert_eq!(camera_zoom_to_tile_zoom(10.0), 16);
    }

    /// Tile coordinate at HCMC center should reproject to near (0, 0) local metres.
    #[test]
    fn mercator_to_local_hcmc_center() {
        let proj = EquirectangularProjection::new(CENTER_LAT, CENTER_LON);

        // HCMC center in slippy map tile coords at zoom 15:
        // lon=106.7019 -> tile_x = (106.7019 + 180) / 360 * 32768 = ~25852.9
        // lat=10.7756 -> tile_y ~= 15388.3
        // So fractional position within tile 25852/15388 at extent=4096:
        // frac_x = 0.9 * 4096 ~ 3686
        // frac_y = 0.3 * 4096 ~ 1229

        // Use the tile that contains HCMC center
        let n = 2_u64.pow(15);
        let tx = ((106.7019 + 180.0) / 360.0 * n as f64).floor() as u64;
        let lat_rad = CENTER_LAT.to_radians();
        let ty = ((1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0
            * n as f64)
            .floor() as u64;

        // Compute the fractional position within the tile for the center
        let frac_x = (106.7019 + 180.0) / 360.0 * n as f64 - tx as f64;
        let frac_y = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI)
            / 2.0
            * n as f64
            - ty as f64;

        let tile_local_x = (frac_x * 4096.0) as f32;
        let tile_local_y = (frac_y * 4096.0) as f32;

        let (wx, wy) = tile_coord_to_local(tile_local_x, tile_local_y, 15, tx, ty, 4096, &proj);

        assert!(
            wx.abs() < 5.0,
            "Expected world x near 0 for HCMC center, got {wx}"
        );
        assert!(
            wy.abs() < 5.0,
            "Expected world y near 0 for HCMC center, got {wy}"
        );
    }

    /// Triangulate a simple rectangle polygon -> 2 triangles (6 vertices).
    #[test]
    fn triangulate_rectangle() {
        let exterior = vec![
            (0.0_f32, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
            (0.0, 0.0), // closing vertex
        ];
        let result = triangulate_polygon(&exterior, &[]);
        // earcut should produce 2 triangles = 6 vertex positions
        assert_eq!(
            result.len(),
            6,
            "Rectangle should triangulate to 6 vertices (2 triangles), got {}",
            result.len()
        );
    }

    /// Triangulate a degenerate polygon (< 3 vertices) -> empty.
    #[test]
    fn triangulate_degenerate() {
        let exterior: Vec<(f32, f32)> = vec![(0.0, 0.0), (1.0, 1.0)];
        let result = triangulate_polygon(&exterior, &[]);
        assert!(result.is_empty(), "Degenerate polygon should produce no triangles");
    }

    /// Layer color mapping.
    #[test]
    fn layer_color_mapping() {
        assert!(layer_color("building").is_some());
        assert!(layer_color("water").is_some());
        assert!(layer_color("transportation").is_some());
        assert!(layer_color("park").is_some());
        assert!(layer_color("landuse").is_some());
        assert!(layer_color("unknown_layer").is_none());
    }

    /// Line segment to quads produces 6 vertices.
    #[test]
    fn line_segment_quad_generation() {
        let quad = line_segment_to_quads((0.0, 0.0), (10.0, 0.0), 0.5);
        assert_eq!(quad.len(), 6);
        // Check that the quad spans the line width
        // For horizontal line, perpendicular is vertical
        assert!((quad[0].1 - 0.5).abs() < 1e-4, "Top should be at y=0.5");
        assert!((quad[1].1 - (-0.5)).abs() < 1e-4, "Bottom should be at y=-0.5");
    }

    /// Decompress gzip data, and pass-through non-gzip data.
    #[test]
    fn decompress_passthrough() {
        let raw = b"hello world";
        let result = decompress_tile(raw);
        assert_eq!(result, raw, "Non-gzip data should pass through unchanged");
    }

    /// Decompress actual gzip data.
    #[test]
    fn decompress_gzip() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let original = b"test data for compression";
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original).unwrap();
        let compressed = encoder.finish().unwrap();

        let result = decompress_tile(&compressed);
        assert_eq!(result, original);
    }
}
