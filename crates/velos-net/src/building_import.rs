//! OSM building footprint extraction from PBF files.
//!
//! Extracts building polygons with height data for 3D extrusion rendering.
//! Follows the same two-pass pattern as `osm_import.rs`:
//! 1. Collect node coordinates.
//! 2. Extract ways with `building=*` tag, resolve coords, compute height.
//!
//! **Limitation (POC):** Only Way-type buildings are extracted. Multipolygon
//! Relations (building outlines with inner courtyards) are not supported.

use std::collections::HashMap;
use std::path::Path;

use osmpbf::{Element, ElementReader};

use crate::error::NetError;
use crate::projection::EquirectangularProjection;

/// A building footprint extracted from OSM data.
#[derive(Debug, Clone)]
pub struct BuildingFootprint {
    /// Exterior ring polygon vertices in projected metres (x_east, y_north).
    /// Guaranteed counter-clockwise winding order.
    pub polygon: Vec<[f64; 2]>,
    /// Building height in metres.
    pub height_m: f64,
}

/// Compute building height from OSM tags.
///
/// Priority:
/// 1. `height` tag (metres, handles " m" suffix)
/// 2. `building:levels` tag (levels * 3.5m per floor)
/// 3. Default: 10.5m (3 floors)
fn compute_building_height(tags: &HashMap<&str, &str>) -> f64 {
    // Check `height` tag first
    if let Some(height_str) = tags.get("height") {
        let cleaned = height_str
            .trim()
            .trim_end_matches(" m")
            .trim_end_matches('m')
            .trim();
        if let Ok(h) = cleaned.parse::<f64>() {
            if h > 0.0 {
                return h;
            }
        }
    }

    // Check `building:levels` tag
    if let Some(levels_str) = tags.get("building:levels") {
        if let Ok(levels) = levels_str.trim().parse::<f64>() {
            if levels > 0.0 {
                return levels * 3.5;
            }
        }
    }

    // Default: 3 floors * 3.5m
    10.5
}

/// Ensure polygon vertices are in counter-clockwise (CCW) order.
///
/// Uses the shoelace formula to compute signed area. If area is negative
/// (clockwise winding), reverses the vertex order.
fn ensure_ccw(polygon: &mut Vec<[f64; 2]>) {
    let signed_area = signed_area_2x(polygon);
    if signed_area < 0.0 {
        polygon.reverse();
    }
}

/// Compute 2x the signed area of a polygon using the shoelace formula.
/// Positive = CCW, Negative = CW.
fn signed_area_2x(polygon: &[[f64; 2]]) -> f64 {
    let n = polygon.len();
    if n < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        area += polygon[i][0] * polygon[j][1];
        area -= polygon[j][0] * polygon[i][1];
    }
    area
}

/// Import building footprints from an OSM PBF file.
///
/// Extracts all ways with a `building=*` tag, resolves node coordinates
/// using the given projection, computes height from tags, and ensures
/// CCW winding order.
///
/// Only Way-type buildings are supported (not multipolygon Relations).
pub fn import_buildings(
    pbf_path: &Path,
    proj: &EquirectangularProjection,
) -> Result<Vec<BuildingFootprint>, NetError> {
    // Pass 1: Collect node coordinates
    let mut node_coords: HashMap<i64, (f64, f64)> = HashMap::new();

    let reader = ElementReader::from_path(pbf_path)
        .map_err(|e| NetError::OsmParse(format!("failed to open PBF: {e}")))?;

    reader
        .for_each(|element| match element {
            Element::Node(node) => {
                node_coords.insert(node.id(), (node.lat(), node.lon()));
            }
            Element::DenseNode(node) => {
                node_coords.insert(node.id, (node.lat(), node.lon()));
            }
            Element::Way(_) | Element::Relation(_) => {}
        })
        .map_err(|e| NetError::OsmParse(format!("PBF read error (pass 1): {e}")))?;

    log::info!("Building import pass 1: {} nodes collected", node_coords.len());

    // Pass 2: Extract building ways
    let mut buildings = Vec::new();

    let reader = ElementReader::from_path(pbf_path)
        .map_err(|e| NetError::OsmParse(format!("failed to open PBF (pass 2): {e}")))?;

    reader
        .for_each(|element| {
            if let Element::Way(way) = element {
                let mut tags: HashMap<&str, &str> = HashMap::new();
                let mut has_building = false;

                for (key, value) in way.tags() {
                    if key == "building" {
                        has_building = true;
                    }
                    tags.insert(key, value);
                }

                if !has_building {
                    return;
                }

                // Resolve node refs to projected coordinates
                let refs: Vec<i64> = way.refs().collect();
                let mut polygon: Vec<[f64; 2]> = Vec::with_capacity(refs.len());

                for &nid in &refs {
                    if let Some(&(lat, lon)) = node_coords.get(&nid) {
                        let (x, y) = proj.project(lat, lon);
                        polygon.push([x, y]);
                    }
                }

                // OSM closed ways repeat the first node at the end -- remove it
                if polygon.len() > 1 && polygon.first() == polygon.last() {
                    polygon.pop();
                }

                // Skip degenerate polygons
                if polygon.len() < 3 {
                    return;
                }

                // Deduplicate consecutive identical vertices
                polygon.dedup();
                if polygon.len() < 3 {
                    return;
                }

                let height_m = compute_building_height(&tags);
                ensure_ccw(&mut polygon);

                buildings.push(BuildingFootprint { polygon, height_m });
            }
        })
        .map_err(|e| NetError::OsmParse(format!("PBF read error (pass 2): {e}")))?;

    log::info!("Building import: {} buildings extracted", buildings.len());

    Ok(buildings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_building_height_from_height_tag() {
        let mut tags = HashMap::new();
        tags.insert("height", "12.5");
        assert!((compute_building_height(&tags) - 12.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_building_height_from_height_tag_with_suffix() {
        let mut tags = HashMap::new();
        tags.insert("height", "12 m");
        assert!((compute_building_height(&tags) - 12.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_building_height_from_levels_tag() {
        let mut tags = HashMap::new();
        tags.insert("building:levels", "4");
        assert!((compute_building_height(&tags) - 14.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_building_height_default() {
        let tags: HashMap<&str, &str> = HashMap::new();
        assert!((compute_building_height(&tags) - 10.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_building_height_priority() {
        // height tag takes priority over building:levels
        let mut tags = HashMap::new();
        tags.insert("height", "20.0");
        tags.insert("building:levels", "4");
        assert!((compute_building_height(&tags) - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_ensure_ccw_reverses_cw_polygon() {
        // CW square (negative signed area)
        let mut polygon = vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
        let original_area = signed_area_2x(&polygon);
        assert!(original_area < 0.0, "Should start as CW (negative area)");

        ensure_ccw(&mut polygon);
        let fixed_area = signed_area_2x(&polygon);
        assert!(fixed_area > 0.0, "Should be CCW after ensure_ccw (positive area)");
    }

    #[test]
    fn test_ensure_ccw_leaves_ccw_unchanged() {
        // CCW square (positive signed area)
        let mut polygon = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let original = polygon.clone();
        let original_area = signed_area_2x(&polygon);
        assert!(original_area > 0.0, "Should start as CCW (positive area)");

        ensure_ccw(&mut polygon);
        assert_eq!(polygon, original, "CCW polygon should not be modified");
    }

    #[test]
    fn test_signed_area_ccw_square() {
        // Unit square CCW: (0,0) -> (1,0) -> (1,1) -> (0,1)
        let polygon = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let area = signed_area_2x(&polygon);
        assert!((area - 2.0).abs() < f64::EPSILON, "2x area of unit square should be 2.0");
    }

    #[test]
    fn test_signed_area_degenerate() {
        let polygon = vec![[0.0, 0.0], [1.0, 0.0]];
        assert!((signed_area_2x(&polygon)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_building_footprint_struct() {
        let fp = BuildingFootprint {
            polygon: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            height_m: 15.0,
        };
        assert_eq!(fp.polygon.len(), 4);
        assert!((fp.height_m - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_import_buildings_integration() {
        // Integration test gated on file existence
        let pbf_path = std::path::Path::new("../../data/hcmc/district1.osm.pbf");
        if !pbf_path.exists() {
            eprintln!("Skipping integration test: {} not found", pbf_path.display());
            return;
        }

        let proj = EquirectangularProjection::new(10.7756, 106.7019);
        let buildings = import_buildings(pbf_path, &proj).expect("import_buildings failed");

        assert!(
            !buildings.is_empty(),
            "Should extract at least some buildings from district1.osm.pbf"
        );

        // Verify all polygons are CCW and have valid height
        for b in &buildings {
            assert!(b.polygon.len() >= 3, "Polygon must have >= 3 vertices");
            assert!(b.height_m > 0.0, "Height must be positive");
            let area = signed_area_2x(&b.polygon);
            assert!(area > 0.0, "All polygons should be CCW (positive area)");
        }

        eprintln!("Integration test: {} buildings extracted", buildings.len());
    }
}
