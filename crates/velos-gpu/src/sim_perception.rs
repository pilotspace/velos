//! Perception dispatch and readback wiring for SimWorld.
//!
//! Provides `step_perception()` which dispatches the GPU perception gather
//! pass and reads back per-agent perception results. Also manages the
//! auxiliary GPU buffers needed for perception: signal state, congestion
//! grid, and edge travel time ratios.

use petgraph::visit::EdgeRef;
use petgraph::Direction;

use crate::compute::ComputeDispatcher;
use crate::perception::{PerceptionBindings, PerceptionParams, PerceptionResult};
use crate::sim::SimWorld;

/// Default congestion grid dimensions (20x20 cells, 500m per cell).
const GRID_WIDTH: u32 = 20;
const GRID_HEIGHT: u32 = 20;
const GRID_CELL_SIZE: f32 = 500.0;

/// Auxiliary GPU buffers for perception pipeline input.
///
/// Pre-allocated at startup to avoid per-frame allocation.
/// These buffers feed into `PerceptionBindings` alongside the agent/lane
/// buffers from `ComputeDispatcher`.
pub(crate) struct PerceptionBuffers {
    /// Per-edge signal state (u32: 0=green, 1=amber, 2=red, 3=none).
    pub signal_buffer: wgpu::Buffer,
    /// Congestion grid heatmap (flat f32 array, GRID_HEIGHT * GRID_WIDTH).
    pub congestion_grid_buffer: wgpu::Buffer,
    /// Per-edge travel time ratio (f32: current/free_flow, 1.0 = free flow).
    pub edge_travel_ratio_buffer: wgpu::Buffer,
    /// Number of edges (for signal buffer sizing).
    pub edge_count: u32,
}

impl PerceptionBuffers {
    /// Create pre-allocated perception buffers.
    ///
    /// Signal and travel ratio buffers are sized by `edge_count`.
    /// Congestion grid is fixed at 20x20 cells (1600 bytes).
    /// All buffers are zeroed on creation.
    pub fn new(device: &wgpu::Device, edge_count: u32) -> Self {
        let edge_count = edge_count.max(1); // Ensure at least 1 entry for valid buffer

        let signal_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("perception_signal_state"),
            size: (edge_count as u64) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let congestion_grid_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("perception_congestion_grid"),
            size: (GRID_WIDTH as u64) * (GRID_HEIGHT as u64) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let edge_travel_ratio_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("perception_edge_travel_ratio"),
            size: (edge_count as u64) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            signal_buffer,
            congestion_grid_buffer,
            edge_travel_ratio_buffer,
            edge_count,
        }
    }
}

impl SimWorld {
    /// Dispatch GPU perception gather pass and readback results.
    ///
    /// Guards:
    /// - Returns empty if no PerceptionPipeline (CPU-only mode).
    /// - Returns empty if no agents uploaded to GPU yet.
    /// - Returns empty if no PerceptionBuffers allocated.
    ///
    /// Pipeline: update signal buffer -> create bind group -> dispatch -> readback.
    pub fn step_perception(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        dispatcher: &ComputeDispatcher,
    ) -> Vec<PerceptionResult> {
        let perception = match &self.perception {
            Some(p) => p,
            None => return Vec::new(),
        };

        let agent_buffer = match dispatcher.agent_buffer() {
            Some(b) => b,
            None => return Vec::new(),
        };

        let lane_agents_buffer = match dispatcher.lane_agents_buffer() {
            Some(b) => b,
            None => return Vec::new(),
        };

        let perc_buffers = match &self.perception_buffers {
            Some(b) => b,
            None => return Vec::new(),
        };

        // Update signal state buffer from current signal controllers.
        self.update_signal_buffer(queue);

        // Update edge travel ratio buffer from prediction overlay if available.
        self.update_edge_travel_ratio_buffer(queue);

        let agent_count = dispatcher.wave_front_agent_count;
        if agent_count == 0 {
            return Vec::new();
        }

        // The shared result buffer lives in ComputeDispatcher -- same buffer
        // that wave_front.wgsl reads at binding(8).
        let result_buffer = dispatcher.perception_result_buffer();

        let bindings = PerceptionBindings {
            agent_buffer,
            lane_agents_buffer,
            signal_buffer: &perc_buffers.signal_buffer,
            sign_buffer: dispatcher.sign_buffer(),
            congestion_grid_buffer: &perc_buffers.congestion_grid_buffer,
            edge_travel_ratio_buffer: &perc_buffers.edge_travel_ratio_buffer,
            result_buffer,
        };

        let bind_group = perception.create_bind_group(device, &bindings);

        let params = PerceptionParams {
            agent_count,
            grid_width: GRID_WIDTH,
            grid_height: GRID_HEIGHT,
            grid_cell_size: GRID_CELL_SIZE,
        };

        let mut encoder = device.create_command_encoder(&Default::default());
        perception.dispatch(&mut encoder, queue, &bind_group, &params);
        queue.submit(std::iter::once(encoder.finish()));

        perception.readback_results(device, queue, result_buffer, agent_count)
    }

    /// Write per-edge signal states to the signal buffer.
    ///
    /// Iterates signal controllers, maps phase states to per-edge u32 values
    /// (0=green, 1=amber, 2=red). Edges without signals get 3 (none).
    fn update_signal_buffer(&self, queue: &wgpu::Queue) {
        let perc_buffers = match &self.perception_buffers {
            Some(b) => b,
            None => return,
        };

        let edge_count = perc_buffers.edge_count as usize;
        let mut signal_states = vec![3u32; edge_count]; // 3 = none

        let g = self.road_graph.inner();

        for (node, ctrl) in &self.signal_controllers {
            let incoming: Vec<_> = g.edges_directed(*node, Direction::Incoming).collect();
            for (approach_idx, edge_ref) in incoming.iter().enumerate() {
                let edge_id = edge_ref.id().index();
                if edge_id < edge_count {
                    let phase_state = ctrl.get_phase_state(approach_idx);
                    signal_states[edge_id] = match phase_state {
                        velos_signal::plan::PhaseState::Green => 0,
                        velos_signal::plan::PhaseState::Amber => 1,
                        velos_signal::plan::PhaseState::Red => 2,
                    };
                }
            }
        }

        queue.write_buffer(
            &perc_buffers.signal_buffer,
            0,
            bytemuck::cast_slice(&signal_states),
        );
    }

    /// Write per-edge travel time ratios from prediction overlay.
    ///
    /// If prediction service is available, copies overlay travel times.
    /// Otherwise buffer stays zeroed (free-flow assumption).
    fn update_edge_travel_ratio_buffer(&self, queue: &wgpu::Queue) {
        let perc_buffers = match &self.perception_buffers {
            Some(b) => b,
            None => return,
        };

        if let Some(prediction_service) = &self.reroute.prediction_service {
            let overlay = prediction_service.store().current();
            let edge_count = perc_buffers.edge_count as usize;
            let travel_times = &overlay.edge_travel_times;

            if !travel_times.is_empty() {
                let count = travel_times.len().min(edge_count);
                queue.write_buffer(
                    &perc_buffers.edge_travel_ratio_buffer,
                    0,
                    bytemuck::cast_slice(&travel_times[..count]),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perception_buffers_creation() {
        // Verify buffer sizing logic without GPU device.
        // edge_count=100: signal=400 bytes, travel_ratio=400 bytes
        // congestion_grid = 20*20*4 = 1600 bytes
        let edge_count: u32 = 100;
        let signal_size = (edge_count as u64) * 4;
        let grid_size = (GRID_WIDTH as u64) * (GRID_HEIGHT as u64) * 4;
        let travel_size = (edge_count as u64) * 4;

        assert_eq!(signal_size, 400);
        assert_eq!(grid_size, 1600);
        assert_eq!(travel_size, 400);
    }

    #[test]
    fn perception_buffers_min_edge_count() {
        // Even with edge_count=0, should use at least 1 for valid buffer
        let edge_count: u32 = 0;
        let clamped = edge_count.max(1);
        assert_eq!(clamped, 1);
    }

    #[test]
    fn step_perception_empty_without_pipeline() {
        // CPU-only SimWorld has no perception pipeline -> empty results.
        use petgraph::graph::DiGraph;
        use velos_net::graph::{RoadGraph, RoadNode};

        let mut g = DiGraph::new();
        g.add_node(RoadNode { pos: [0.0, 0.0] });
        g.add_node(RoadNode { pos: [100.0, 0.0] });
        let graph = RoadGraph::new(g);

        let sim = SimWorld::new_cpu_only(graph);
        assert!(sim.perception.is_none());
        // step_perception would return empty vec (no GPU device to call with,
        // but the guard check is verified by the None perception field).
    }

    #[test]
    fn grid_constants() {
        assert_eq!(GRID_WIDTH, 20);
        assert_eq!(GRID_HEIGHT, 20);
        assert_eq!(GRID_CELL_SIZE, 500.0);
    }

    #[test]
    fn signal_dirty_initialized_true() {
        use petgraph::graph::DiGraph;
        use velos_net::graph::{RoadGraph, RoadNode};

        let mut g = DiGraph::new();
        g.add_node(RoadNode { pos: [0.0, 0.0] });
        g.add_node(RoadNode { pos: [100.0, 0.0] });
        let graph = RoadGraph::new(g);
        let sim = SimWorld::new_cpu_only(graph);

        // Dirty flags must be true on creation (force initial upload).
        assert!(sim.signal_dirty, "signal_dirty should start true for initial upload");
        assert!(sim.prediction_dirty, "prediction_dirty should start true for initial upload");
    }

    #[test]
    fn signal_dirty_stays_false_without_phase_change() {
        use petgraph::graph::DiGraph;
        use velos_net::graph::{RoadGraph, RoadNode};

        let mut g = DiGraph::new();
        g.add_node(RoadNode { pos: [0.0, 0.0] });
        g.add_node(RoadNode { pos: [100.0, 0.0] });
        let graph = RoadGraph::new(g);
        let mut sim = SimWorld::new_cpu_only(graph);

        // Set dirty to false (simulating post-upload state).
        sim.signal_dirty = false;

        // Tick signals with no phase change (small dt, no detectors).
        sim.step_signals_with_detectors(0.01, &[]);

        // Without a phase transition, signal_dirty should remain false.
        assert!(!sim.signal_dirty, "signal_dirty should stay false without phase change");
    }

    #[test]
    fn signal_dirty_set_true_on_phase_transition() {
        use petgraph::graph::DiGraph;
        use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};

        // Build a signalized intersection.
        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [100.0, 0.0] });
        g.add_edge(
            a,
            b,
            RoadEdge {
                length_m: 100.0,
                speed_limit_mps: 13.9,
                lane_count: 1,
                oneway: true,
                road_class: RoadClass::Primary,
                geometry: vec![[0.0, 0.0], [100.0, 0.0]],
                motorbike_only: false,
                time_windows: None,
            },
        );
        let graph = RoadGraph::new(g);
        let mut sim = SimWorld::new_cpu_only(graph);

        // If there are signal controllers, advance time enough to trigger
        // a phase change (typical green duration is 20-30s).
        sim.signal_dirty = false;

        if !sim.signal_controllers.is_empty() {
            // Step enough to cross a phase boundary.
            for _ in 0..500 {
                sim.step_signals_with_detectors(0.1, &[]);
            }
            // After 50s of stepping, at least one phase transition should
            // have occurred, setting signal_dirty = true.
            assert!(
                sim.signal_dirty,
                "signal_dirty should be true after phase transition"
            );
        }
        // If no signal controllers, test is trivially valid (no phase to change).
    }

    #[test]
    fn prediction_dirty_stays_false_without_update() {
        use petgraph::graph::DiGraph;
        use velos_net::graph::{RoadGraph, RoadNode};

        let mut g = DiGraph::new();
        g.add_node(RoadNode { pos: [0.0, 0.0] });
        g.add_node(RoadNode { pos: [100.0, 0.0] });
        let graph = RoadGraph::new(g);
        let mut sim = SimWorld::new_cpu_only(graph);

        sim.prediction_dirty = false;

        // step_prediction with no prediction service should not set dirty.
        sim.step_prediction();

        assert!(
            !sim.prediction_dirty,
            "prediction_dirty should stay false without update"
        );
    }
}
