//! glTF/glb mesh loading into GPU vertex/index buffers.
//!
//! Loads 3D meshes from `.glb` files for agent rendering. Falls back to
//! procedural box meshes when files are not found.
//!
//! **Model sources (CC0 licensed):**
//! - Kenney Car Kit: <https://kenney-assets.itch.io/car-kit>
//! - Quaternius: <https://quaternius.com/>
//!
//! Place `.glb` files in `assets/models/` with names:
//! `motorbike.glb`, `car.glb`, `bus.glb`, `truck.glb`, `pedestrian.glb`.

use std::collections::HashMap;
use std::path::Path;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use velos_core::components::VehicleType;

/// 3D vertex with position and normal (24 bytes).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex3D {
    /// World-space position.
    pub position: [f32; 3],
    /// Surface normal (normalized).
    pub normal: [f32; 3],
}

/// A loaded mesh in CPU memory (vertices + indices).
pub struct LoadedMesh {
    pub vertices: Vec<Vertex3D>,
    pub indices: Vec<u32>,
}

/// Load a `.glb` file and extract vertices and indices from the first primitive.
///
/// Returns `Err` if the file cannot be read or contains no mesh data.
pub fn load_glb(path: &Path) -> Result<LoadedMesh, String> {
    let (document, buffers, _images) =
        gltf::import(path).map_err(|e| format!("Failed to import glb: {e}"))?;

    let mesh = document
        .meshes()
        .next()
        .ok_or_else(|| "No meshes in glb file".to_string())?;

    let primitive = mesh
        .primitives()
        .next()
        .ok_or_else(|| "No primitives in mesh".to_string())?;

    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .ok_or_else(|| "No positions in mesh".to_string())?
        .collect();

    let normals: Vec<[f32; 3]> = reader
        .read_normals()
        .map(|iter| iter.collect())
        .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

    let vertices: Vec<Vertex3D> = positions
        .iter()
        .zip(normals.iter())
        .map(|(&position, &normal)| Vertex3D { position, normal })
        .collect();

    let indices: Vec<u32> = reader
        .read_indices()
        .ok_or_else(|| "No indices in mesh".to_string())?
        .into_u32()
        .collect();

    Ok(LoadedMesh { vertices, indices })
}

/// Generate a procedural box mesh with normals.
///
/// Used as a fallback when `.glb` files are not found.
/// The box is centered at origin with the given dimensions.
pub fn generate_fallback_box(width: f32, height: f32, depth: f32) -> LoadedMesh {
    let hw = width / 2.0;
    let hh = height / 2.0;
    let hd = depth / 2.0;

    // 6 faces, 4 vertices each, with face normals
    #[rustfmt::skip]
    let vertices = vec![
        // Front face (+Z)
        Vertex3D { position: [-hw, -hh,  hd], normal: [ 0.0,  0.0,  1.0] },
        Vertex3D { position: [ hw, -hh,  hd], normal: [ 0.0,  0.0,  1.0] },
        Vertex3D { position: [ hw,  hh,  hd], normal: [ 0.0,  0.0,  1.0] },
        Vertex3D { position: [-hw,  hh,  hd], normal: [ 0.0,  0.0,  1.0] },
        // Back face (-Z)
        Vertex3D { position: [ hw, -hh, -hd], normal: [ 0.0,  0.0, -1.0] },
        Vertex3D { position: [-hw, -hh, -hd], normal: [ 0.0,  0.0, -1.0] },
        Vertex3D { position: [-hw,  hh, -hd], normal: [ 0.0,  0.0, -1.0] },
        Vertex3D { position: [ hw,  hh, -hd], normal: [ 0.0,  0.0, -1.0] },
        // Top face (+Y)
        Vertex3D { position: [-hw,  hh,  hd], normal: [ 0.0,  1.0,  0.0] },
        Vertex3D { position: [ hw,  hh,  hd], normal: [ 0.0,  1.0,  0.0] },
        Vertex3D { position: [ hw,  hh, -hd], normal: [ 0.0,  1.0,  0.0] },
        Vertex3D { position: [-hw,  hh, -hd], normal: [ 0.0,  1.0,  0.0] },
        // Bottom face (-Y)
        Vertex3D { position: [-hw, -hh, -hd], normal: [ 0.0, -1.0,  0.0] },
        Vertex3D { position: [ hw, -hh, -hd], normal: [ 0.0, -1.0,  0.0] },
        Vertex3D { position: [ hw, -hh,  hd], normal: [ 0.0, -1.0,  0.0] },
        Vertex3D { position: [-hw, -hh,  hd], normal: [ 0.0, -1.0,  0.0] },
        // Right face (+X)
        Vertex3D { position: [ hw, -hh,  hd], normal: [ 1.0,  0.0,  0.0] },
        Vertex3D { position: [ hw, -hh, -hd], normal: [ 1.0,  0.0,  0.0] },
        Vertex3D { position: [ hw,  hh, -hd], normal: [ 1.0,  0.0,  0.0] },
        Vertex3D { position: [ hw,  hh,  hd], normal: [ 1.0,  0.0,  0.0] },
        // Left face (-X)
        Vertex3D { position: [-hw, -hh, -hd], normal: [-1.0,  0.0,  0.0] },
        Vertex3D { position: [-hw, -hh,  hd], normal: [-1.0,  0.0,  0.0] },
        Vertex3D { position: [-hw,  hh,  hd], normal: [-1.0,  0.0,  0.0] },
        Vertex3D { position: [-hw,  hh, -hd], normal: [-1.0,  0.0,  0.0] },
    ];

    // Two triangles per face, 6 faces = 36 indices
    let mut indices = Vec::with_capacity(36);
    for face in 0..6u32 {
        let base = face * 4;
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    LoadedMesh { vertices, indices }
}

/// Fallback dimensions by vehicle type (width, height, depth in metres).
fn fallback_dimensions(vtype: VehicleType) -> (f32, f32, f32) {
    match vtype {
        VehicleType::Motorbike | VehicleType::Bicycle => (0.6, 1.2, 2.0),
        VehicleType::Car => (1.8, 1.4, 4.5),
        VehicleType::Bus => (2.5, 3.0, 12.0),
        VehicleType::Truck => (2.5, 2.5, 8.0),
        VehicleType::Emergency => (1.8, 1.6, 5.0),
        VehicleType::Pedestrian => (0.4, 1.7, 0.4),
    }
}

/// GPU mesh handle: vertex buffer, index buffer, and index count.
pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

/// Collection of loaded GPU meshes per vehicle type.
pub struct MeshSet {
    pub meshes: HashMap<VehicleType, GpuMesh>,
}

impl MeshSet {
    /// Load meshes for all vehicle types.
    ///
    /// Tries to load from `assets/models/{type}.glb`. Falls back to
    /// procedural box meshes if the file is not found.
    pub fn load_all(device: &wgpu::Device) -> Self {
        let types = [
            (VehicleType::Motorbike, "motorbike"),
            (VehicleType::Car, "car"),
            (VehicleType::Bus, "bus"),
            (VehicleType::Truck, "truck"),
            (VehicleType::Pedestrian, "pedestrian"),
            (VehicleType::Emergency, "emergency"),
            (VehicleType::Bicycle, "bicycle"),
        ];

        let mut meshes = HashMap::new();
        for (vtype, name) in types {
            let path = Path::new("assets/models").join(format!("{name}.glb"));
            let loaded = if path.exists() {
                match load_glb(&path) {
                    Ok(m) => {
                        log::info!("Loaded {name}.glb ({} verts, {} indices)", m.vertices.len(), m.indices.len());
                        m
                    }
                    Err(e) => {
                        log::warn!("Failed to load {name}.glb: {e}, using fallback box");
                        let (w, h, d) = fallback_dimensions(vtype);
                        generate_fallback_box(w, h, d)
                    }
                }
            } else {
                log::info!("No {name}.glb found, using fallback box");
                let (w, h, d) = fallback_dimensions(vtype);
                generate_fallback_box(w, h, d)
            };

            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{name}_vertex_buf")),
                contents: bytemuck::cast_slice(&loaded.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{name}_index_buf")),
                contents: bytemuck::cast_slice(&loaded.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            let index_count = loaded.indices.len() as u32;

            meshes.insert(vtype, GpuMesh { vertex_buffer, index_buffer, index_count });
        }

        MeshSet { meshes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vertex3d_size() {
        assert_eq!(
            std::mem::size_of::<Vertex3D>(),
            24,
            "Vertex3D should be 24 bytes (position + normal)"
        );
    }

    #[test]
    fn test_fallback_box_generates_valid_mesh() {
        let mesh = generate_fallback_box(2.0, 1.0, 4.0);
        assert_eq!(mesh.vertices.len(), 24, "Box should have 24 vertices (4 per face)");
        assert_eq!(mesh.indices.len(), 36, "Box should have 36 indices (6 per face)");

        // All indices should be in range
        for &idx in &mesh.indices {
            assert!(
                (idx as usize) < mesh.vertices.len(),
                "Index {} out of range",
                idx
            );
        }
    }

    #[test]
    fn test_fallback_box_normals_are_unit_length() {
        let mesh = generate_fallback_box(1.0, 1.0, 1.0);
        for v in &mesh.vertices {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!(
                (len - 1.0).abs() < 1e-6,
                "Normal not unit length: {:?} (len={})",
                v.normal,
                len
            );
        }
    }

    #[test]
    fn test_fallback_box_dimensions() {
        let mesh = generate_fallback_box(2.0, 3.0, 4.0);
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for v in &mesh.vertices {
            for i in 0..3 {
                min[i] = min[i].min(v.position[i]);
                max[i] = max[i].max(v.position[i]);
            }
        }
        assert!((max[0] - min[0] - 2.0).abs() < 1e-6, "Width should be 2.0");
        assert!((max[1] - min[1] - 3.0).abs() < 1e-6, "Height should be 3.0");
        assert!((max[2] - min[2] - 4.0).abs() < 1e-6, "Depth should be 4.0");
    }

    #[test]
    fn test_load_glb_nonexistent_file() {
        let result = load_glb(Path::new("nonexistent.glb"));
        assert!(result.is_err(), "Should error on nonexistent file");
    }

    #[test]
    fn test_fallback_dimensions_all_types() {
        let types = [
            VehicleType::Motorbike,
            VehicleType::Car,
            VehicleType::Bus,
            VehicleType::Truck,
            VehicleType::Emergency,
            VehicleType::Bicycle,
            VehicleType::Pedestrian,
        ];
        for vtype in types {
            let (w, h, d) = fallback_dimensions(vtype);
            assert!(w > 0.0 && h > 0.0 && d > 0.0, "Dimensions should be positive for {:?}", vtype);
        }
    }
}
