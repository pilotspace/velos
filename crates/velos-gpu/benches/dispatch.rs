//! Performance benchmarks for GPU dispatch with 280K agents.
//!
//! Run with: cargo bench -p velos-gpu --bench dispatch --features gpu-tests
//! Requires Metal GPU adapter on macOS.
//!
//! Benchmarks:
//! - 280k_single_gpu: full wave-front pipeline on single GPU
//! - 280k_multi_gpu_2: 2 logical partitions with boundary protocol
//! - 280k_multi_gpu_4: 4 logical partitions with boundary protocol
//! - lane_sort_280k: CPU-side lane sorting for 280K agents

use criterion::{criterion_group, criterion_main, Criterion};

#[cfg(feature = "gpu-tests")]
mod gpu_benches {
    use criterion::Criterion;
    use petgraph::graph::DiGraph;
    use velos_core::components::GpuAgentState;
    use velos_gpu::compute::{sort_agents_by_lane, ComputeDispatcher};
    use velos_gpu::multi_gpu::MultiGpuScheduler;
    use velos_gpu::partition::partition_network;
    use velos_gpu::GpuContext;
    use velos_net::{RoadEdge, RoadGraph, RoadNode};

    const AGENT_COUNT: usize = 280_000;
    const NODE_SIDE: usize = 160; // 160x160 = 25,600 nodes -> ~51,000 edges

    /// Build a synthetic grid road network similar to 5-district HCMC scale.
    fn make_benchmark_graph() -> RoadGraph {
        let mut g = DiGraph::new();
        let mut node_indices = Vec::with_capacity(NODE_SIDE * NODE_SIDE);

        for row in 0..NODE_SIDE {
            for col in 0..NODE_SIDE {
                let idx = g.add_node(RoadNode {
                    pos: [col as f64 * 50.0, row as f64 * 50.0],
                });
                node_indices.push(idx);
            }
        }

        let edge_data = || RoadEdge {
            length_m: 50.0,
            speed_limit_mps: 13.89,
            lane_count: 2,
            oneway: false,
            road_class: velos_net::graph::RoadClass::Secondary,
            geometry: vec![],
            motorbike_only: false,
            time_windows: None,
        };

        // Horizontal edges.
        for row in 0..NODE_SIDE {
            for col in 0..(NODE_SIDE - 1) {
                let a = node_indices[row * NODE_SIDE + col];
                let b = node_indices[row * NODE_SIDE + col + 1];
                g.add_edge(a, b, edge_data());
            }
        }

        // Vertical edges.
        for row in 0..(NODE_SIDE - 1) {
            for col in 0..NODE_SIDE {
                let a = node_indices[row * NODE_SIDE + col];
                let b = node_indices[(row + 1) * NODE_SIDE + col];
                g.add_edge(a, b, edge_data());
            }
        }

        RoadGraph::new(g)
    }

    /// Generate 280K synthetic agents distributed across edges.
    fn make_agents(graph: &RoadGraph) -> Vec<GpuAgentState> {
        let edge_count = graph.edge_count();
        let mut agents = Vec::with_capacity(AGENT_COUNT);

        for i in 0..AGENT_COUNT {
            let edge_id = (i % edge_count) as u32;
            let lane_idx = (i / edge_count) as u32 % 2;
            let position = ((i % 50) as i32 + 1) * 65536; // Q16.16 metres
            let speed = 655360; // ~10 m/s in Q12.20

            agents.push(GpuAgentState {
                edge_id,
                lane_idx,
                position,
                lateral: 0,
                speed,
                acceleration: 0,
                cf_model: if i % 3 == 0 { 1 } else { 0 }, // mix IDM + Krauss
                rng_state: i as u32,
            });
        }
        agents
    }

    /// Benchmark: CPU-side lane sorting for 280K agents.
    pub fn bench_lane_sort(c: &mut Criterion) {
        let graph = make_benchmark_graph();
        let agents = make_agents(&graph);

        c.bench_function("lane_sort_280k", |b| {
            b.iter(|| {
                let _ = sort_agents_by_lane(&agents);
            });
        });
    }

    /// Benchmark: Single GPU wave-front dispatch for 280K agents.
    pub fn bench_single_gpu(c: &mut Criterion) {
        let ctx = match GpuContext::new_headless() {
            Some(c) => c,
            None => {
                eprintln!("SKIP bench: no GPU adapter");
                return;
            }
        };

        let graph = make_benchmark_graph();
        let agents = make_agents(&graph);
        let (lane_offsets, lane_counts, lane_agents) = sort_agents_by_lane(&agents);

        let mut dispatcher = ComputeDispatcher::new(&ctx.device);
        dispatcher.upload_wave_front_data(
            &ctx.device,
            &ctx.queue,
            &agents,
            &lane_offsets,
            &lane_counts,
            &lane_agents,
        );

        c.bench_function("280k_single_gpu", |b| {
            b.iter(|| {
                let mut encoder = ctx.device.create_command_encoder(&Default::default());
                dispatcher.dispatch_wave_front(&mut encoder, &ctx.device, &ctx.queue, 0.1);
                ctx.queue.submit(std::iter::once(encoder.finish()));
                let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
            });
        });
    }

    /// Benchmark: 2 logical GPU partitions with boundary protocol.
    pub fn bench_multi_gpu_2(c: &mut Criterion) {
        let ctx = match GpuContext::new_headless() {
            Some(c) => c,
            None => {
                eprintln!("SKIP bench: no GPU adapter");
                return;
            }
        };

        let graph = make_benchmark_graph();
        let agents = make_agents(&graph);
        let assignment = partition_network(&graph, 2);
        let mut scheduler = MultiGpuScheduler::new(assignment);
        scheduler.distribute_agents(&agents);

        let mut dispatcher = ComputeDispatcher::new(&ctx.device);

        c.bench_function("280k_multi_gpu_2", |b| {
            b.iter(|| {
                // Per-partition: sort, upload, dispatch, readback.
                for partition in scheduler.partitions_mut() {
                    partition.spawn_inbox_agents();

                    if partition.agent_states.is_empty() {
                        continue;
                    }

                    let (offsets, counts, indices) =
                        sort_agents_by_lane(&partition.agent_states);
                    dispatcher.upload_wave_front_data(
                        &ctx.device,
                        &ctx.queue,
                        &partition.agent_states,
                        &offsets,
                        &counts,
                        &indices,
                    );

                    let mut encoder =
                        ctx.device.create_command_encoder(&Default::default());
                    dispatcher.dispatch_wave_front(
                        &mut encoder, &ctx.device, &ctx.queue, 0.1,
                    );
                    ctx.queue.submit(std::iter::once(encoder.finish()));
                    let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());

                    let updated =
                        dispatcher.readback_wave_front_agents(&ctx.device, &ctx.queue);
                    let len = partition.agent_states.len().min(updated.len());
                    partition.agent_states[..len].copy_from_slice(&updated[..len]);
                }
            });
        });
    }

    /// Benchmark: 4 logical GPU partitions with boundary protocol.
    pub fn bench_multi_gpu_4(c: &mut Criterion) {
        let ctx = match GpuContext::new_headless() {
            Some(c) => c,
            None => {
                eprintln!("SKIP bench: no GPU adapter");
                return;
            }
        };

        let graph = make_benchmark_graph();
        let agents = make_agents(&graph);
        let assignment = partition_network(&graph, 4);
        let mut scheduler = MultiGpuScheduler::new(assignment);
        scheduler.distribute_agents(&agents);

        let mut dispatcher = ComputeDispatcher::new(&ctx.device);

        c.bench_function("280k_multi_gpu_4", |b| {
            b.iter(|| {
                for partition in scheduler.partitions_mut() {
                    partition.spawn_inbox_agents();

                    if partition.agent_states.is_empty() {
                        continue;
                    }

                    let (offsets, counts, indices) =
                        sort_agents_by_lane(&partition.agent_states);
                    dispatcher.upload_wave_front_data(
                        &ctx.device,
                        &ctx.queue,
                        &partition.agent_states,
                        &offsets,
                        &counts,
                        &indices,
                    );

                    let mut encoder =
                        ctx.device.create_command_encoder(&Default::default());
                    dispatcher.dispatch_wave_front(
                        &mut encoder, &ctx.device, &ctx.queue, 0.1,
                    );
                    ctx.queue.submit(std::iter::once(encoder.finish()));
                    let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());

                    let updated =
                        dispatcher.readback_wave_front_agents(&ctx.device, &ctx.queue);
                    let len = partition.agent_states.len().min(updated.len());
                    partition.agent_states[..len].copy_from_slice(&updated[..len]);
                }
            });
        });
    }
}

#[cfg(feature = "gpu-tests")]
fn all_benchmarks(c: &mut Criterion) {
    gpu_benches::bench_lane_sort(c);
    gpu_benches::bench_single_gpu(c);
    gpu_benches::bench_multi_gpu_2(c);
    gpu_benches::bench_multi_gpu_4(c);
}

#[cfg(not(feature = "gpu-tests"))]
fn all_benchmarks(_c: &mut Criterion) {
    eprintln!("SKIP: gpu-tests feature not enabled. Run with --features gpu-tests");
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = all_benchmarks
}
criterion_main!(benches);
