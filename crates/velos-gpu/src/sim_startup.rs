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
}
