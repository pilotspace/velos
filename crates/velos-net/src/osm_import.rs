//! OSM PBF import pipeline: reads an OSM PBF file and builds a directed road graph.
//!
//! Two-pass approach:
//! 1. Collect node coordinates and highway ways with tags.
//! 2. Build directed graph edges from way node sequences.

use std::collections::HashMap;
use std::path::Path;

use osmpbf::{Element, ElementReader};
use petgraph::graph::{DiGraph, NodeIndex};

use crate::error::NetError;
use crate::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};
use crate::projection::EquirectangularProjection;

/// Parsed road properties from OSM tags.
#[derive(Debug, Clone)]
struct RoadProperties {
    road_class: RoadClass,
    lane_count: u8,
    speed_limit_mps: f64,
    oneway: bool,
    /// Whether motorcycle=designated or motorcycle=yes tag is present.
    motorcycle_designated: bool,
    /// Road width in metres from `width` tag, if available.
    width_m: Option<f64>,
}

/// Import an OSM PBF file into a directed road graph.
///
/// Filters to highway=primary|secondary|tertiary|residential only.
/// Projects all coordinates to local metres centered on `(center_lat, center_lon)`.
pub fn import_osm(
    pbf_path: &Path,
    center_lat: f64,
    center_lon: f64,
) -> Result<RoadGraph, NetError> {
    let proj = EquirectangularProjection::new(center_lat, center_lon);

    // Phase 1: Collect node coordinates and highway ways.
    let mut node_coords: HashMap<i64, (f64, f64)> = HashMap::new();
    let mut ways: Vec<(Vec<i64>, RoadProperties)> = Vec::new();

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
            Element::Way(way) => {
                if let Some(props) = parse_road_tags(&way) {
                    let refs: Vec<i64> = way.refs().collect();
                    if refs.len() >= 2 {
                        ways.push((refs, props));
                    }
                }
            }
            Element::Relation(_) => {}
        })
        .map_err(|e| NetError::OsmParse(format!("PBF read error: {e}")))?;

    log::info!(
        "OSM import: {} nodes, {} highway ways",
        node_coords.len(),
        ways.len()
    );

    // Phase 2: Build directed graph.
    let mut graph = DiGraph::<RoadNode, RoadEdge>::new();
    let mut osm_to_graph: HashMap<i64, NodeIndex> = HashMap::new();

    for (refs, props) in &ways {
        // Ensure all referenced nodes exist in our coordinate map.
        let projected: Vec<Option<[f64; 2]>> = refs
            .iter()
            .map(|&nid| {
                node_coords.get(&nid).map(|&(lat, lon)| {
                    let (x, y) = proj.project(lat, lon);
                    [x, y]
                })
            })
            .collect();

        // Get or create graph nodes for each OSM node in the way.
        let graph_nodes: Vec<Option<NodeIndex>> = refs
            .iter()
            .zip(projected.iter())
            .map(|(&nid, pos_opt)| {
                pos_opt.map(|pos| {
                    *osm_to_graph
                        .entry(nid)
                        .or_insert_with(|| graph.add_node(RoadNode { pos }))
                })
            })
            .collect();

        // Create edges for each consecutive pair of nodes.
        for window in graph_nodes.windows(2) {
            let (Some(a), Some(b)) = (window[0], window[1]) else {
                continue;
            };

            let pos_a = graph[a].pos;
            let pos_b = graph[b].pos;
            let length_m = euclidean_dist(pos_a, pos_b);

            // Skip degenerate zero-length edges.
            if length_m < 0.01 {
                continue;
            }

            let geometry = vec![pos_a, pos_b];

            // Detect motorbike-only: motorcycle=designated, or narrow
            // service/residential roads (width < 4m).
            let motorbike_only = props.motorcycle_designated
                || (matches!(
                    props.road_class,
                    RoadClass::Service | RoadClass::Residential
                ) && props.width_m.is_some_and(|w| w < 4.0));

            // Forward edge (always).
            graph.add_edge(
                a,
                b,
                RoadEdge {
                    length_m,
                    speed_limit_mps: props.speed_limit_mps,
                    lane_count: props.lane_count,
                    oneway: props.oneway,
                    road_class: props.road_class,
                    geometry: geometry.clone(),
                    motorbike_only,
                    time_windows: None,
                },
            );

            // Reverse edge (only if not oneway).
            if !props.oneway {
                graph.add_edge(
                    b,
                    a,
                    RoadEdge {
                        length_m,
                        speed_limit_mps: props.speed_limit_mps,
                        lane_count: props.lane_count,
                        oneway: false,
                        road_class: props.road_class,
                        geometry: vec![pos_b, pos_a],
                        motorbike_only,
                        time_windows: None,
                    },
                );
            }
        }
    }

    log::info!(
        "Road graph built: {} nodes, {} edges",
        graph.node_count(),
        graph.edge_count()
    );

    Ok(RoadGraph::new(graph))
}

/// Parse OSM way tags to extract road properties.
/// Returns `None` if the way is not a supported highway type.
fn parse_road_tags(way: &osmpbf::Way) -> Option<RoadProperties> {
    let mut highway = None;
    let mut lanes_tag = None;
    let mut maxspeed_tag = None;
    let mut oneway_tag = None;
    let mut motorcycle_tag = None;
    let mut width_tag = None;

    for (key, value) in way.tags() {
        match key {
            "highway" => highway = Some(value),
            "lanes" => lanes_tag = Some(value),
            "maxspeed" => maxspeed_tag = Some(value),
            "oneway" => oneway_tag = Some(value),
            "motorcycle" => motorcycle_tag = Some(value),
            "width" => width_tag = Some(value),
            _ => {}
        }
    }

    let road_class = match highway? {
        "motorway" | "motorway_link" => RoadClass::Motorway,
        "trunk" | "trunk_link" => RoadClass::Trunk,
        "primary" | "primary_link" => RoadClass::Primary,
        "secondary" | "secondary_link" => RoadClass::Secondary,
        "tertiary" | "tertiary_link" => RoadClass::Tertiary,
        "residential" => RoadClass::Residential,
        "service" => RoadClass::Service,
        _ => return None,
    };

    let lane_count = lanes_tag
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or_else(|| infer_lanes(road_class));

    let speed_limit_mps = maxspeed_tag
        .and_then(|v| v.trim_end_matches(" km/h").parse::<f64>().ok())
        .unwrap_or_else(|| default_speed_kmh(road_class))
        / 3.6;

    let oneway = matches!(oneway_tag, Some("yes") | Some("1") | Some("true"));

    let motorcycle_designated =
        matches!(motorcycle_tag, Some("designated") | Some("yes"));

    let width_m = width_tag.and_then(|v| {
        v.trim_end_matches(" m")
            .trim_end_matches('m')
            .trim()
            .parse::<f64>()
            .ok()
    });

    Some(RoadProperties {
        road_class,
        lane_count,
        speed_limit_mps,
        oneway,
        motorcycle_designated,
        width_m,
    })
}

/// Infer lane count from road class when `lanes` tag is missing.
fn infer_lanes(road_class: RoadClass) -> u8 {
    match road_class {
        RoadClass::Motorway | RoadClass::Trunk => 3,
        RoadClass::Primary | RoadClass::Secondary => 2,
        RoadClass::Tertiary | RoadClass::Residential | RoadClass::Service => 1,
    }
}

/// Default speed limit in km/h by road class (HCMC urban defaults).
fn default_speed_kmh(road_class: RoadClass) -> f64 {
    match road_class {
        RoadClass::Motorway => 80.0,
        RoadClass::Trunk => 60.0,
        RoadClass::Primary => 50.0,
        RoadClass::Secondary => 40.0,
        RoadClass::Tertiary => 30.0,
        RoadClass::Residential => 20.0,
        RoadClass::Service => 15.0,
    }
}

/// Euclidean distance between two 2D points.
fn euclidean_dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_lanes_primary() {
        assert_eq!(infer_lanes(RoadClass::Primary), 2);
        assert_eq!(infer_lanes(RoadClass::Secondary), 2);
    }

    #[test]
    fn infer_lanes_minor() {
        assert_eq!(infer_lanes(RoadClass::Tertiary), 1);
        assert_eq!(infer_lanes(RoadClass::Residential), 1);
    }

    #[test]
    fn default_speed_values() {
        assert!((default_speed_kmh(RoadClass::Primary) - 50.0).abs() < f64::EPSILON);
        assert!((default_speed_kmh(RoadClass::Secondary) - 40.0).abs() < f64::EPSILON);
        assert!((default_speed_kmh(RoadClass::Tertiary) - 30.0).abs() < f64::EPSILON);
        assert!((default_speed_kmh(RoadClass::Residential) - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn euclidean_dist_test() {
        let d = euclidean_dist([0.0, 0.0], [3.0, 4.0]);
        assert!((d - 5.0).abs() < 1e-10);
    }
}
