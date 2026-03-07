//! GPU perception pipeline: gathers per-agent awareness data in a single compute pass.
//!
//! The perception kernel reads agent state, signal state, sign data, and congestion
//! information to produce a `PerceptionResult` per agent. This runs AFTER wave_front
//! update so positions are current.
//!
//! Uses a SEPARATE bind group layout from wave_front to avoid binding conflicts.

use crate::compute::bgl_entry;

/// Per-agent perception result from GPU gather pass. 32 bytes, GPU-aligned.
///
/// Written by perception.wgsl, read back to CPU for evaluation phase consumption.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PerceptionResult {
    /// Leader vehicle speed (m/s), 0.0 if no leader detected.
    pub leader_speed: f32,
    /// Gap to leader vehicle (m), 9999.0 if no leader.
    pub leader_gap: f32,
    /// Signal state: 0=green, 1=amber, 2=red, 3=none.
    pub signal_state: u32,
    /// Distance to next signal (m).
    pub signal_distance: f32,
    /// Travel time ratio on own route edges (1.0 = free flow).
    pub congestion_own_route: f32,
    /// Grid heatmap value at agent position (0.0-1.0).
    pub congestion_area: f32,
    /// Active speed limit (m/s), 0.0 if none.
    pub sign_speed_limit: f32,
    /// Bit flags: bit0=route_blocked, bit1=emergency_nearby.
    pub flags: u32,
}

const _: () = assert!(std::mem::size_of::<PerceptionResult>() == 32);

/// Uniform parameters for perception dispatch.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PerceptionParams {
    /// Number of agents to process.
    pub agent_count: u32,
    /// Congestion grid width in cells.
    pub grid_width: u32,
    /// Congestion grid height in cells.
    pub grid_height: u32,
    /// Size of each grid cell in metres (default 500.0).
    pub grid_cell_size: f32,
}

const _: () = assert!(std::mem::size_of::<PerceptionParams>() == 16);

const WORKGROUP_SIZE: u32 = 256;

/// Input buffer references for perception bind group creation.
///
/// Groups the 6 input buffers to avoid too-many-arguments clippy warning.
pub struct PerceptionBindings<'a> {
    /// Agent state buffer (from ComputeDispatcher::agent_buffer()).
    pub agent_buffer: &'a wgpu::Buffer,
    /// Lane agent index buffer (from ComputeDispatcher::lane_agents_buffer()).
    pub lane_agents_buffer: &'a wgpu::Buffer,
    /// Signal state buffer (one entry per edge).
    pub signal_buffer: &'a wgpu::Buffer,
    /// Traffic sign buffer (GpuSign entries).
    pub sign_buffer: &'a wgpu::Buffer,
    /// Congestion grid heatmap (flat array, grid_height * grid_width).
    pub congestion_grid_buffer: &'a wgpu::Buffer,
    /// Per-edge travel time ratio (current / free_flow).
    pub edge_travel_ratio_buffer: &'a wgpu::Buffer,
}

/// GPU perception pipeline with separate bind group from wave_front.
///
/// Reads agent buffer (from ComputeDispatcher), signal/sign/congestion buffers,
/// and writes PerceptionResult per agent for CPU readback.
pub struct PerceptionPipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    result_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    max_agents: u32,
}

impl PerceptionPipeline {
    /// Create perception pipeline from perception.wgsl.
    ///
    /// Allocates result buffer (max_agents * 32 bytes) and staging buffer for
    /// CPU readback. Uses SEPARATE bind group layout from wave_front.
    pub fn new(device: &wgpu::Device, max_agents: u32) -> Self {
        let shader = device
            .create_shader_module(wgpu::include_wgsl!("../shaders/perception.wgsl"));

        // Separate bind group layout (not shared with wave_front)
        // @binding(0): PerceptionParams (uniform)
        // @binding(1): agents (storage, read)
        // @binding(2): lane_agents (storage, read)
        // @binding(3): signals (storage, read)
        // @binding(4): signs (storage, read)
        // @binding(5): congestion_grid (storage, read)
        // @binding(6): edge_travel_ratios (storage, read)
        // @binding(7): results (storage, read_write)
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("perception_bgl"),
                entries: &[
                    bgl_entry(0, wgpu::BufferBindingType::Uniform, false),
                    bgl_entry(1, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    bgl_entry(2, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    bgl_entry(3, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    bgl_entry(4, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    bgl_entry(5, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    bgl_entry(6, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    bgl_entry(7, wgpu::BufferBindingType::Storage { read_only: false }, false),
                ],
            });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("perception_pipeline_layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("perception_pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("perception_gather"),
                compilation_options: Default::default(),
                cache: None,
            });

        let result_size = (max_agents as u64) * (std::mem::size_of::<PerceptionResult>() as u64);
        let result_size = result_size.max(32); // min 32 bytes for valid buffer

        let result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("perception_results"),
            size: result_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("perception_staging"),
            size: result_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("perception_params"),
            size: std::mem::size_of::<PerceptionParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            result_buffer,
            staging_buffer,
            params_buffer,
            max_agents,
        }
    }

    /// Create a bind group with all input buffers and the result buffer.
    ///
    /// The `agent_buffer` is obtained from `ComputeDispatcher::agent_buffer()`.
    /// The `signal_buffer`, `sign_buffer`, `congestion_grid_buffer`, and
    /// `edge_travel_ratio_buffer` are created externally (wired in Plan 07-06).
    pub fn create_bind_group(
        &self,
        device: &wgpu::Device,
        bindings: &PerceptionBindings<'_>,
    ) -> wgpu::BindGroup {
        let agent_buffer = bindings.agent_buffer;
        let lane_agents_buffer = bindings.lane_agents_buffer;
        let signal_buffer = bindings.signal_buffer;
        let sign_buffer = bindings.sign_buffer;
        let congestion_grid_buffer = bindings.congestion_grid_buffer;
        let edge_travel_ratio_buffer = bindings.edge_travel_ratio_buffer;
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("perception_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: agent_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: lane_agents_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: signal_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: sign_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: congestion_grid_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: edge_travel_ratio_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: self.result_buffer.as_entire_binding(),
                },
            ],
        })
    }

    /// Dispatch the perception gather kernel.
    ///
    /// Must be called AFTER wave_front dispatch so agent positions are current.
    /// Dispatches ceil(agent_count / 256) workgroups.
    pub fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        bind_group: &wgpu::BindGroup,
        params: &PerceptionParams,
    ) {
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(params));

        let workgroups = params.agent_count.div_ceil(WORKGROUP_SIZE);
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("perception_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.dispatch_workgroups(workgroups, 1, 1);
        }
    }

    /// Copy result buffer to staging and read back perception results to CPU.
    ///
    /// Returns one `PerceptionResult` per agent up to `agent_count`.
    pub fn readback_results(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        agent_count: u32,
    ) -> Vec<PerceptionResult> {
        let count = agent_count.min(self.max_agents) as usize;
        if count == 0 {
            return Vec::new();
        }

        let byte_size = (count * std::mem::size_of::<PerceptionResult>()) as u64;

        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(
            &self.result_buffer,
            0,
            &self.staging_buffer,
            0,
            byte_size,
        );
        queue.submit(std::iter::once(encoder.finish()));

        let slice = self.staging_buffer.slice(..byte_size);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());

        let data = slice.get_mapped_range();
        let results: Vec<PerceptionResult> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        self.staging_buffer.unmap();

        results
    }

    /// Returns a reference to the result buffer (for chaining with other GPU passes).
    pub fn result_buffer(&self) -> &wgpu::Buffer {
        &self.result_buffer
    }

    /// Returns the maximum number of agents this pipeline was allocated for.
    pub fn max_agents(&self) -> u32 {
        self.max_agents
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;

    #[test]
    fn perception_result_size_is_32_bytes() {
        assert_eq!(std::mem::size_of::<PerceptionResult>(), 32);
    }

    #[test]
    fn perception_result_alignment() {
        // All fields are 4-byte aligned (f32 or u32), struct is 32 bytes
        assert_eq!(std::mem::align_of::<PerceptionResult>(), 4);
    }

    #[test]
    fn perception_params_size_is_16_bytes() {
        assert_eq!(std::mem::size_of::<PerceptionParams>(), 16);
    }

    #[test]
    fn perception_result_zeroed() {
        let r = PerceptionResult::zeroed();
        assert_eq!(r.leader_speed, 0.0);
        assert_eq!(r.leader_gap, 0.0);
        assert_eq!(r.signal_state, 0);
        assert_eq!(r.signal_distance, 0.0);
        assert_eq!(r.congestion_own_route, 0.0);
        assert_eq!(r.congestion_area, 0.0);
        assert_eq!(r.sign_speed_limit, 0.0);
        assert_eq!(r.flags, 0);
    }

    #[test]
    fn perception_params_default_values() {
        let params = PerceptionParams {
            agent_count: 280_000,
            grid_width: 20,
            grid_height: 20,
            grid_cell_size: 500.0,
        };
        assert_eq!(params.agent_count, 280_000);
        assert_eq!(params.grid_cell_size, 500.0);
    }

    #[test]
    fn perception_result_bytemuck_roundtrip() {
        let original = PerceptionResult {
            leader_speed: 13.89,
            leader_gap: 42.5,
            signal_state: 2, // red
            signal_distance: 100.0,
            congestion_own_route: 1.5,
            congestion_area: 0.3,
            sign_speed_limit: 8.33,
            flags: 0b11, // route_blocked + emergency_nearby
        };

        let bytes = bytemuck::bytes_of(&original);
        assert_eq!(bytes.len(), 32);

        let recovered: &PerceptionResult = bytemuck::from_bytes(bytes);
        assert_eq!(recovered.leader_speed, 13.89);
        assert_eq!(recovered.leader_gap, 42.5);
        assert_eq!(recovered.signal_state, 2);
        assert_eq!(recovered.signal_distance, 100.0);
        assert_eq!(recovered.congestion_own_route, 1.5);
        assert_eq!(recovered.congestion_area, 0.3);
        assert_eq!(recovered.sign_speed_limit, 8.33);
        assert_eq!(recovered.flags, 0b11);
    }

    #[test]
    fn perception_result_flag_bits() {
        let mut r = PerceptionResult::zeroed();

        // bit0 = route_blocked
        r.flags |= 1;
        assert_ne!(r.flags & 1, 0);
        assert_eq!(r.flags & 2, 0);

        // bit1 = emergency_nearby
        r.flags |= 2;
        assert_ne!(r.flags & 1, 0);
        assert_ne!(r.flags & 2, 0);
    }
}
