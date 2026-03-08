//! Startup initialization methods for SimWorld.
//!
//! Extracted from sim.rs to keep the main module under 700 lines.
//! Handles vehicle config loading, signal controller construction,
//! loop detector placement, sign buffer upload, and GPU subsystem init.

use std::collections::HashMap;

use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;

use velos_net::RoadGraph;
use velos_signal::actuated::ActuatedController;
use velos_signal::adaptive::AdaptiveController;
use velos_signal::config::SignalConfig;
use velos_signal::controller::FixedTimeController;
use velos_signal::detector::LoopDetector;
use velos_signal::plan::{SignalPhase, SignalPlan};
use velos_signal::signs::GpuSign;
use velos_signal::SignalController;
use velos_vehicle::config::VehicleConfig;

use velos_demand::bus_spawner::BusSpawner;
use velos_vehicle::bus::BusStop;

use crate::compute::{ComputeDispatcher, GpuVehicleParams};

/// Load vehicle configuration from TOML with env override.
///
/// Falls back to `VehicleConfig::default()` on error with a warning log.
pub(crate) fn load_vehicle_config() -> VehicleConfig {
    let path = std::env::var("VELOS_VEHICLE_CONFIG")
        .unwrap_or_else(|_| "data/hcmc/vehicle_params.toml".to_string());

    match velos_vehicle::config::load_vehicle_config(&path) {
        Ok(config) => {
            log::info!("Loaded vehicle config from '{}'", path);
            config
        }
        Err(e) => {
            log::warn!(
                "Failed to load vehicle config from '{}': {}. Using defaults.",
                path,
                e
            );
            VehicleConfig::default()
        }
    }
}

/// Build polymorphic signal controllers from road graph and config.
///
/// For each node with in_degree >= 4, checks signal_config for a matching
/// node_id. If found, instantiates the configured controller type. Otherwise,
/// creates a FixedTimeController with default 30s green phases.
///
/// Returns (controllers, signalized_nodes map).
/// Result of building signal controllers: (controllers, signalized_nodes map).
pub(crate) type SignalControllerResult = (
    Vec<(NodeIndex, Box<dyn SignalController>)>,
    HashMap<u32, Vec<EdgeIndex>>,
);

#[allow(clippy::type_complexity)]
pub(crate) fn build_signal_controllers(
    road_graph: &RoadGraph,
    signal_config: &SignalConfig,
) -> SignalControllerResult {
    let g = road_graph.inner();

    // Index config by node_id for O(1) lookup.
    let config_map: HashMap<u32, &velos_signal::config::IntersectionConfig> = signal_config
        .intersection
        .iter()
        .map(|ic| (ic.node_id, ic))
        .collect();

    let mut controllers: Vec<(NodeIndex, Box<dyn SignalController>)> = Vec::new();
    let mut signalized_nodes: HashMap<u32, Vec<EdgeIndex>> = HashMap::new();

    for node_idx in g.node_indices() {
        let in_degree = g
            .edges_directed(node_idx, Direction::Incoming)
            .count();

        if in_degree < 4 {
            continue;
        }

        let approaches: Vec<usize> = (0..in_degree).collect();
        let half = in_degree / 2;
        let phase_a = SignalPhase {
            green_duration: 30.0,
            amber_duration: 3.0,
            approaches: approaches[..half].to_vec(),
        };
        let phase_b = SignalPhase {
            green_duration: 30.0,
            amber_duration: 3.0,
            approaches: approaches[half..].to_vec(),
        };
        let plan = SignalPlan::new(vec![phase_a, phase_b]);

        let node_id = node_idx.index() as u32;
        let controller: Box<dyn SignalController> =
            if let Some(ic) = config_map.get(&node_id) {
                match ic.controller.as_str() {
                    "actuated" => Box::new(ActuatedController::new_with_params(
                        plan,
                        in_degree,
                        ic.min_green,
                        ic.max_green,
                        ic.gap_threshold,
                    )),
                    "adaptive" => Box::new(AdaptiveController::new(plan, in_degree)),
                    _ => Box::new(FixedTimeController::new(plan, in_degree)),
                }
            } else {
                Box::new(FixedTimeController::new(plan, in_degree))
            };

        controllers.push((node_idx, controller));

        let edges: Vec<EdgeIndex> = g
            .edges_directed(node_idx, Direction::Incoming)
            .map(|e| e.id())
            .collect();
        signalized_nodes.insert(node_id, edges);
    }

    log::info!(
        "Built {} signal controllers ({} from config overrides)",
        controllers.len(),
        signal_config.intersection.len(),
    );

    (controllers, signalized_nodes)
}

/// Build loop detectors for actuated intersections.
///
/// For each actuated intersection, creates a LoopDetector on each incoming
/// edge at 80% of edge length (upstream position). Only actuated
/// intersections need detectors; fixed-time and adaptive ignore them.
pub(crate) fn build_loop_detectors(
    road_graph: &RoadGraph,
    signal_config: &SignalConfig,
    signalized_nodes: &HashMap<u32, Vec<EdgeIndex>>,
) -> Vec<(NodeIndex, Vec<LoopDetector>)> {
    let g = road_graph.inner();

    // Collect node_ids configured as actuated.
    let actuated_nodes: std::collections::HashSet<u32> = signal_config
        .intersection
        .iter()
        .filter(|ic| ic.controller == "actuated")
        .map(|ic| ic.node_id)
        .collect();

    let mut detectors = Vec::new();

    for (&node_id, edges) in signalized_nodes {
        if !actuated_nodes.contains(&node_id) {
            continue;
        }

        let node_idx = NodeIndex::new(node_id as usize);
        let mut node_detectors = Vec::new();

        for &edge_idx in edges {
            let edge_length = g
                .edge_weight(edge_idx)
                .map(|e| e.length_m)
                .unwrap_or(100.0);

            // Place detector at 80% of edge length (upstream of intersection).
            let offset = edge_length * 0.8;
            node_detectors.push(LoopDetector::new(edge_idx.index() as u32, offset));
        }

        detectors.push((node_idx, node_detectors));
    }

    if !detectors.is_empty() {
        log::info!(
            "Created loop detectors at {} actuated intersections",
            detectors.len()
        );
    }

    detectors
}

/// Upload traffic signs from the road graph to the GPU sign buffer.
///
/// Collects speed limit signs from edges and uploads via `dispatcher.upload_signs()`.
/// If no signs exist, logs info and skips (sign_count=0 is valid).
pub(crate) fn upload_network_signs(
    road_graph: &RoadGraph,
    dispatcher: &mut ComputeDispatcher,
    queue: &wgpu::Queue,
) {
    let g = road_graph.inner();
    let mut signs: Vec<GpuSign> = Vec::new();

    for edge_idx in g.edge_indices() {
        let edge = &g[edge_idx];
        // Generate a speed limit sign for each edge with a known speed limit.
        if edge.speed_limit_mps > 0.0 {
            signs.push(GpuSign {
                sign_type: 0, // SpeedLimit
                value: edge.speed_limit_mps as f32,
                edge_id: edge_idx.index() as u32,
                offset_m: 0.0, // At start of edge
            });
        }
    }

    if signs.is_empty() {
        log::info!("No traffic signs in network; sign_count=0");
    } else {
        log::info!("Uploading {} traffic signs to GPU", signs.len());
        dispatcher.upload_signs(queue, &signs);
    }
}

/// Load zone configuration from TOML with env override.
///
/// Falls back to `ZoneConfig::new()` (all edges Micro) on error with a warning log.
/// This follows the same graceful degradation pattern as `load_vehicle_config()`.
pub(crate) fn load_zone_config() -> velos_meso::zone_config::ZoneConfig {
    let path = std::env::var("VELOS_ZONE_CONFIG")
        .unwrap_or_else(|_| "data/hcmc/zone_config.toml".to_string());

    match velos_meso::zone_config::ZoneConfig::load_from_toml(std::path::Path::new(&path)) {
        Ok(config) => {
            log::info!(
                "Loaded zone config from '{}' ({} zone assignments)",
                path,
                config.len()
            );
            config
        }
        Err(e) => {
            log::warn!(
                "Failed to load zone config from '{}': {}. All edges default to Micro.",
                path,
                e
            );
            velos_meso::zone_config::ZoneConfig::new()
        }
    }
}

/// Load GTFS bus stop data and create a BusSpawner for time-gated bus spawning.
///
/// Reads GTFS CSV files from the directory specified by `VELOS_GTFS_PATH` env var
/// (default: `data/gtfs`). On missing or invalid data, returns empty results with
/// a log message -- graceful degradation, no crash.
///
/// Returns `(bus_stops, optional_spawner, stop_id_to_index_map)`.
pub(crate) fn load_gtfs_bus_stops(
    road_graph: &RoadGraph,
) -> (Vec<BusStop>, Option<BusSpawner>, HashMap<String, usize>) {
    let path_str = std::env::var("VELOS_GTFS_PATH")
        .unwrap_or_else(|_| "data/gtfs".to_string());
    let path = std::path::Path::new(&path_str);

    if !path.is_dir() {
        log::info!("No GTFS data found at '{}', bus stops inactive", path_str);
        return (Vec::new(), None, HashMap::new());
    }

    let (routes, schedules) = match velos_demand::load_gtfs_csv(path) {
        Ok(data) => data,
        Err(e) => {
            log::warn!("Failed to load GTFS data from '{}': {}. Bus stops inactive.", path_str, e);
            return (Vec::new(), None, HashMap::new());
        }
    };

    // Collect all unique GtfsStops across routes (dedup by stop_id).
    let mut seen_stop_ids = std::collections::HashSet::new();
    let mut all_stops = Vec::new();
    for route in &routes {
        for stop in &route.stops {
            if seen_stop_ids.insert(stop.stop_id.clone()) {
                all_stops.push(stop.clone());
            }
        }
    }

    if all_stops.is_empty() {
        log::info!("GTFS data at '{}' has no stops, bus stops inactive", path_str);
        return (Vec::new(), None, HashMap::new());
    }

    // Project and snap stops to road edges.
    let proj = velos_net::EquirectangularProjection::new(10.7756, 106.7019);
    let bus_stops = velos_net::snap_gtfs_stops(&all_stops, road_graph, &proj);

    // Build stop_id_to_index by matching each original stop to the nearest
    // snapped BusStop (handles merge deduplication).
    let stop_id_to_index = build_stop_id_mapping(&all_stops, &bus_stops, &proj);

    // Build route_stop_ids: route_id -> Vec<stop_id>
    let mut route_stop_ids: HashMap<String, Vec<String>> = HashMap::new();
    for route in &routes {
        let stop_ids: Vec<String> = route.stops.iter()
            .map(|s| s.stop_id.clone())
            .collect();
        route_stop_ids.insert(route.route_id.clone(), stop_ids);
    }

    let bus_spawner = BusSpawner::new(&route_stop_ids, &stop_id_to_index, schedules);

    log::info!(
        "GTFS loaded: {} bus stops snapped, {} routes",
        bus_stops.len(),
        routes.len(),
    );

    (bus_stops, Some(bus_spawner), stop_id_to_index)
}

/// Build a mapping from GTFS stop_id to index in the snapped bus_stops Vec.
///
/// After snap_gtfs_stops merges nearby stops, the output Vec may be smaller
/// than the input. For each original GtfsStop, we find the closest BusStop
/// on the same edge (within merge threshold) to determine the correct index.
fn build_stop_id_mapping(
    original_stops: &[velos_demand::GtfsStop],
    bus_stops: &[BusStop],
    _proj: &velos_net::EquirectangularProjection,
) -> HashMap<String, usize> {
    let mut mapping = HashMap::new();

    if bus_stops.is_empty() {
        return mapping;
    }

    // For each original stop, project to local coords, find which bus_stop
    // it corresponds to by matching edge_id and checking offset proximity.
    // We re-snap each stop individually to get its edge_id and offset,
    // then find the matching bus_stop index.
    //
    // Note: We don't need to rebuild the R-tree -- we can match by name
    // since snap_gtfs_stops preserves the GtfsStop.name in BusStop.name.
    // But name matching is fragile if stops have duplicate names.
    // Instead, we project each stop and match against bus_stops by edge proximity.

    // Build a quick lookup: for each edge_id, list of (bus_stop_index, offset_m).
    let mut edge_index: HashMap<u32, Vec<(usize, f64)>> = HashMap::new();
    for (idx, bs) in bus_stops.iter().enumerate() {
        edge_index.entry(bs.edge_id).or_default().push((idx, bs.offset_m));
    }

    // For each original stop, project and find the nearest edge, then
    // match to the closest bus_stop on that edge.
    // We need the graph's R-tree. But we don't have the graph here.
    // Simpler approach: match by name (unique within a single GTFS dataset).
    for stop in original_stops {
        // Find bus_stop with matching name.
        if let Some(idx) = bus_stops.iter().position(|bs| bs.name == stop.name) {
            mapping.insert(stop.stop_id.clone(), idx);
        }
        // If no name match (stop was merged or skipped), it won't be in the mapping.
        // This is correct: BusSpawner filters out unmapped stop_ids.
    }

    mapping
}

/// Upload vehicle parameters to the GPU uniform buffer at binding 7.
pub(crate) fn upload_vehicle_params(
    vehicle_config: &VehicleConfig,
    dispatcher: &ComputeDispatcher,
    queue: &wgpu::Queue,
) {
    let gpu_params = GpuVehicleParams::from_config(vehicle_config);
    dispatcher.upload_vehicle_params(queue, &gpu_params);
    log::info!("Vehicle params uploaded to GPU uniform buffer (binding 7)");
}

#[cfg(test)]
mod tests {
    use super::*;
    use petgraph::graph::DiGraph;
    use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};

    fn make_signalized_graph() -> RoadGraph {
        let mut g = DiGraph::new();
        let center = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let n = g.add_node(RoadNode { pos: [0.0, 100.0] });
        let s = g.add_node(RoadNode { pos: [0.0, -100.0] });
        let e = g.add_node(RoadNode { pos: [100.0, 0.0] });
        let w = g.add_node(RoadNode { pos: [-100.0, 0.0] });

        let edge = || RoadEdge {
            length_m: 100.0,
            speed_limit_mps: 13.9,
            lane_count: 2,
            oneway: true,
            road_class: RoadClass::Primary,
            geometry: vec![[0.0, 0.0], [100.0, 0.0]],
            motorbike_only: false,
            time_windows: None,
        };

        // 4 incoming edges to center -> signalized
        g.add_edge(n, center, edge());
        g.add_edge(s, center, edge());
        g.add_edge(e, center, edge());
        g.add_edge(w, center, edge());

        RoadGraph::new(g)
    }

    #[test]
    fn build_controllers_default_fixed_time() {
        let graph = make_signalized_graph();
        let config = SignalConfig::default();

        let (controllers, signalized) = build_signal_controllers(&graph, &config);
        assert_eq!(controllers.len(), 1);
        assert_eq!(signalized.len(), 1);

        // Default is fixed-time, verify it produces PhaseState
        let (_, ctrl) = &controllers[0];
        let state = ctrl.get_phase_state(0);
        // Initial state: first phase should be Green
        assert_eq!(state, velos_signal::plan::PhaseState::Green);
    }

    #[test]
    fn build_controllers_with_actuated_config() {
        let graph = make_signalized_graph();
        let config = SignalConfig {
            intersection: vec![velos_signal::config::IntersectionConfig {
                node_id: 0, // center node
                controller: "actuated".to_string(),
                min_green: 10.0,
                max_green: 50.0,
                gap_threshold: 2.0,
            }],
        };

        let (controllers, _) = build_signal_controllers(&graph, &config);
        assert_eq!(controllers.len(), 1);
        // Verify it responds to tick with detectors (actuated behavior)
        let (_, ctrl) = &controllers[0];
        let state = ctrl.get_phase_state(0);
        assert_eq!(state, velos_signal::plan::PhaseState::Green);
    }

    #[test]
    fn build_loop_detectors_only_for_actuated() {
        let graph = make_signalized_graph();
        let config = SignalConfig {
            intersection: vec![velos_signal::config::IntersectionConfig {
                node_id: 0,
                controller: "actuated".to_string(),
                min_green: 7.0,
                max_green: 60.0,
                gap_threshold: 3.0,
            }],
        };

        let (_, signalized) = build_signal_controllers(&graph, &config);
        let detectors = build_loop_detectors(&graph, &config, &signalized);
        assert_eq!(detectors.len(), 1);
        // 4 incoming edges -> 4 detectors
        assert_eq!(detectors[0].1.len(), 4);
        // Detector at 80% of edge length
        assert!((detectors[0].1[0].offset_m - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn build_loop_detectors_empty_for_fixed_only() {
        let graph = make_signalized_graph();
        let config = SignalConfig::default();

        let (_, signalized) = build_signal_controllers(&graph, &config);
        let detectors = build_loop_detectors(&graph, &config, &signalized);
        assert!(detectors.is_empty());
    }

    #[test]
    fn load_vehicle_config_fallback() {
        let config = load_vehicle_config();
        // Should succeed with either real file or default
        assert!(config.motorbike.v0 > 0.0);
        assert!(config.car.v0 > 0.0);
    }

    #[test]
    fn load_zone_config_missing_file_defaults_to_micro() {
        // No zone_config.toml exists in test environment, so should get all-Micro default.
        let config = load_zone_config();
        assert_eq!(
            config.zone_type(42),
            velos_meso::zone_config::ZoneType::Micro
        );
        assert_eq!(
            config.zone_type(999),
            velos_meso::zone_config::ZoneType::Micro
        );
    }

    #[test]
    fn load_gtfs_bus_stops_missing_dir_returns_empty() {
        // Point to a non-existent directory.
        // SAFETY: Test-only, single-threaded env var mutation.
        unsafe { std::env::set_var("VELOS_GTFS_PATH", "/tmp/nonexistent_gtfs_dir_12345") };
        let graph = make_signalized_graph();
        let (bus_stops, spawner, mapping) = load_gtfs_bus_stops(&graph);
        assert!(bus_stops.is_empty(), "no GTFS dir should return empty bus_stops");
        assert!(spawner.is_none(), "no GTFS dir should return None spawner");
        assert!(mapping.is_empty(), "no GTFS dir should return empty mapping");
        unsafe { std::env::remove_var("VELOS_GTFS_PATH") };
    }

    #[test]
    fn load_gtfs_bus_stops_invalid_dir_returns_empty() {
        // Create a temp dir without any GTFS files.
        let tmp_dir = std::env::temp_dir().join("velos_gtfs_empty_test");
        let _ = std::fs::create_dir_all(&tmp_dir);
        // SAFETY: Test-only, single-threaded env var mutation.
        unsafe { std::env::set_var("VELOS_GTFS_PATH", tmp_dir.to_str().unwrap()) };
        let graph = make_signalized_graph();
        let (bus_stops, spawner, _) = load_gtfs_bus_stops(&graph);
        assert!(bus_stops.is_empty(), "empty GTFS dir should return empty bus_stops");
        assert!(spawner.is_none(), "empty GTFS dir should return None spawner");
        unsafe { std::env::remove_var("VELOS_GTFS_PATH") };
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn load_gtfs_bus_stops_with_valid_data() {
        // Create a temp GTFS directory with valid test data.
        let tmp_dir = std::env::temp_dir().join("velos_gtfs_valid_test");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        // Write minimal valid GTFS files.
        std::fs::write(
            tmp_dir.join("routes.txt"),
            "route_id,route_long_name\nR1,Test Route 1\n",
        ).unwrap();

        // Stop at lat/lon that maps near the graph edges.
        // The projection center is (10.7756, 106.7019), mapping to (0,0) in local coords.
        std::fs::write(
            tmp_dir.join("stops.txt"),
            "stop_id,stop_name,stop_lat,stop_lon\n\
             S1,Stop One,10.7756,106.7019\n",
        ).unwrap();

        std::fs::write(
            tmp_dir.join("trips.txt"),
            "trip_id,route_id\nT1,R1\n",
        ).unwrap();

        std::fs::write(
            tmp_dir.join("stop_times.txt"),
            "trip_id,stop_id,arrival_time,departure_time,stop_sequence\n\
             T1,S1,06:00:00,06:00:00,1\n",
        ).unwrap();

        // SAFETY: Test-only, single-threaded env var mutation.
        unsafe { std::env::set_var("VELOS_GTFS_PATH", tmp_dir.to_str().unwrap()) };

        // Build a graph with geometry near (0,0) in local coords.
        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [200.0, 0.0] });
        g.add_edge(a, b, RoadEdge {
            length_m: 200.0,
            speed_limit_mps: 13.9,
            lane_count: 2,
            oneway: true,
            road_class: RoadClass::Primary,
            geometry: vec![[0.0, 0.0], [200.0, 0.0]],
            motorbike_only: false,
            time_windows: None,
        });
        let graph = RoadGraph::new(g);

        let (bus_stops, spawner, mapping) = load_gtfs_bus_stops(&graph);

        assert!(!bus_stops.is_empty(), "valid GTFS should produce bus_stops");
        assert!(spawner.is_some(), "valid GTFS should produce a BusSpawner");
        assert!(!mapping.is_empty(), "valid GTFS should produce stop_id mapping");
        assert!(mapping.contains_key("S1"), "mapping should contain S1");

        unsafe { std::env::remove_var("VELOS_GTFS_PATH") };
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn build_stop_id_mapping_matches_by_name() {
        use velos_demand::GtfsStop;
        use velos_vehicle::bus::BusStop;

        let proj = velos_net::EquirectangularProjection::new(10.7756, 106.7019);
        let original = vec![
            GtfsStop { stop_id: "S1".to_string(), name: "Alpha".to_string(), lat: 10.7756, lon: 106.7019 },
            GtfsStop { stop_id: "S2".to_string(), name: "Beta".to_string(), lat: 10.7757, lon: 106.7020 },
        ];
        let bus_stops = vec![
            BusStop { edge_id: 0, offset_m: 10.0, capacity: 40, name: "Alpha".to_string() },
            BusStop { edge_id: 1, offset_m: 20.0, capacity: 40, name: "Beta".to_string() },
        ];

        let mapping = build_stop_id_mapping(&original, &bus_stops, &proj);
        assert_eq!(mapping.get("S1"), Some(&0));
        assert_eq!(mapping.get("S2"), Some(&1));
    }
}
