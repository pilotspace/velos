//! Pedestrian adaptive GPU dispatch with prefix-sum compaction.
//!
//! Implements a 6-dispatch pipeline:
//!
//! 1. Count pedestrians per spatial hash cell (atomic adds)
//! 2. Per-workgroup prefix sum (Hillis-Steele) -- 3 sub-dispatches: local, scan sums, propagate
//! 3. Scatter pedestrian indices into compacted array
//! 4. Social force computation on non-empty cells only

use crate::compute::bgl_entry;

const WORKGROUP_SIZE: u32 = 256;

/// GPU-side pedestrian state for adaptive social force dispatch.
/// Matches WGSL `struct Pedestrian` in pedestrian_adaptive.wgsl.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPedestrian {
    pub pos_x: f32,
    pub pos_y: f32,
    pub vel_x: f32,
    pub vel_y: f32,
    pub dest_x: f32,
    pub dest_y: f32,
    pub radius: f32,
    pub _pad: f32,
}

/// Uniform params for pedestrian adaptive dispatch.
/// Matches WGSL `struct PedestrianParams` in pedestrian_adaptive.wgsl.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PedestrianAdaptiveParams {
    pub ped_count: u32,
    pub cell_count: u32,
    pub grid_w: u32,
    pub grid_h: u32,
    pub cell_size: f32,
    pub dt: f32,
    pub a_social: f32,
    pub b_social: f32,
    pub tau: f32,
    pub desired_speed: f32,
    pub lambda: f32,
    pub max_force: f32,
    pub max_speed: f32,
    pub radius: f32,
    pub workgroup_count: u32,
    pub _pad: u32,
}

impl Default for PedestrianAdaptiveParams {
    fn default() -> Self {
        Self {
            ped_count: 0,
            cell_count: 0,
            grid_w: 0,
            grid_h: 0,
            cell_size: 5.0,
            dt: 0.1,
            a_social: 2000.0,
            b_social: 0.08,
            tau: 0.5,
            desired_speed: 1.2,
            lambda: 0.5,
            max_force: 50.0,
            max_speed: 2.0,
            radius: 0.3,
            workgroup_count: 0,
            _pad: 0,
        }
    }
}

/// Pedestrian adaptive dispatch pipeline state.
///
/// Manages 6 compute pipelines for density-adaptive pedestrian social force:
/// count, prefix_sum_local, prefix_sum_workgroup_sums, prefix_sum_propagate,
/// scatter, and social_force_adaptive.
pub struct PedestrianAdaptivePipeline {
    count_pipeline: wgpu::ComputePipeline,
    prefix_local_pipeline: wgpu::ComputePipeline,
    prefix_wg_sums_pipeline: wgpu::ComputePipeline,
    prefix_propagate_pipeline: wgpu::ComputePipeline,
    scatter_pipeline: wgpu::ComputePipeline,
    social_force_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    ped_buffer: Option<wgpu::Buffer>,
    cell_counts_buffer: Option<wgpu::Buffer>,
    cell_offsets_buffer: Option<wgpu::Buffer>,
    compacted_indices_buffer: Option<wgpu::Buffer>,
    cell_map_buffer: Option<wgpu::Buffer>,
    scatter_counters_buffer: Option<wgpu::Buffer>,
    workgroup_sums_buffer: Option<wgpu::Buffer>,
    staging_buffer: Option<wgpu::Buffer>,
    /// Current pedestrian count in GPU buffers.
    pub ped_count: u32,
}

impl PedestrianAdaptivePipeline {
    /// Create all 6 compute pipelines from the embedded WGSL shader.
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!(
            "../shaders/pedestrian_adaptive.wgsl"
        ));

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ped_adaptive_bgl"),
                entries: &[
                    bgl_entry(0, wgpu::BufferBindingType::Uniform, false),
                    bgl_entry(1, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    bgl_entry(2, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    bgl_entry(3, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    bgl_entry(4, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    bgl_entry(5, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    bgl_entry(6, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    bgl_entry(7, wgpu::BufferBindingType::Storage { read_only: false }, false),
                ],
            });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ped_adaptive_pipeline_layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let create_pipeline = |entry: &str, label: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };

        let count_pipeline = create_pipeline("count_per_cell", "ped_count_pipeline");
        let prefix_local_pipeline =
            create_pipeline("prefix_sum_local", "ped_prefix_local_pipeline");
        let prefix_wg_sums_pipeline =
            create_pipeline("prefix_sum_workgroup_sums", "ped_prefix_wg_sums_pipeline");
        let prefix_propagate_pipeline =
            create_pipeline("prefix_sum_propagate", "ped_prefix_propagate_pipeline");
        let scatter_pipeline = create_pipeline("scatter", "ped_scatter_pipeline");
        let social_force_pipeline =
            create_pipeline("social_force_adaptive", "ped_social_force_pipeline");

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ped_adaptive_params"),
            size: std::mem::size_of::<PedestrianAdaptiveParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            count_pipeline,
            prefix_local_pipeline,
            prefix_wg_sums_pipeline,
            prefix_propagate_pipeline,
            scatter_pipeline,
            social_force_pipeline,
            bind_group_layout,
            params_buffer,
            ped_buffer: None,
            cell_counts_buffer: None,
            cell_offsets_buffer: None,
            compacted_indices_buffer: None,
            cell_map_buffer: None,
            scatter_counters_buffer: None,
            workgroup_sums_buffer: None,
            staging_buffer: None,
            ped_count: 0,
        }
    }

    /// Upload pedestrian data for adaptive social force dispatch.
    ///
    /// `pedestrians` contains position, velocity, destination, and radius for each pedestrian.
    /// `grid_w` and `grid_h` define the spatial hash grid dimensions.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pedestrians: &[GpuPedestrian],
        grid_w: u32,
        grid_h: u32,
    ) {
        let ped_count = pedestrians.len() as u32;
        let cell_count = grid_w * grid_h;
        let prefix_wg_count = cell_count.div_ceil(WORKGROUP_SIZE);

        let ped_bytes = std::mem::size_of_val(pedestrians) as u64;
        let cell_u32_bytes = (cell_count as u64) * 4;
        let ped_u32_bytes = (ped_count as u64) * 4;
        let wg_sums_bytes = (prefix_wg_count as u64) * 4;

        let storage_rw = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let storage_rw_src = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC;

        let needs_recreate =
            self.ped_buffer.as_ref().is_none_or(|b| b.size() < ped_bytes.max(32));

        if needs_recreate {
            self.ped_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ped_adaptive_peds"),
                size: ped_bytes.max(32),
                usage: storage_rw_src,
                mapped_at_creation: false,
            }));
            self.cell_counts_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ped_cell_counts"),
                size: cell_u32_bytes.max(4),
                usage: storage_rw,
                mapped_at_creation: false,
            }));
            self.cell_offsets_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ped_cell_offsets"),
                size: cell_u32_bytes.max(4),
                usage: storage_rw,
                mapped_at_creation: false,
            }));
            self.compacted_indices_buffer =
                Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ped_compacted_indices"),
                    size: ped_u32_bytes.max(4),
                    usage: storage_rw,
                    mapped_at_creation: false,
                }));
            self.cell_map_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ped_cell_map"),
                size: ped_u32_bytes.max(4),
                usage: storage_rw,
                mapped_at_creation: false,
            }));
            self.scatter_counters_buffer =
                Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ped_scatter_counters"),
                    size: cell_u32_bytes.max(4),
                    usage: storage_rw,
                    mapped_at_creation: false,
                }));
            self.workgroup_sums_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ped_workgroup_sums"),
                size: wg_sums_bytes.max(4),
                usage: storage_rw,
                mapped_at_creation: false,
            }));
            self.staging_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ped_staging"),
                size: ped_bytes.max(32),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        if !pedestrians.is_empty() {
            queue.write_buffer(
                self.ped_buffer.as_ref().unwrap(),
                0,
                bytemuck::cast_slice(pedestrians),
            );
        }

        // Zero out cell_counts and scatter_counters (they use atomics)
        let zeros = vec![0u8; cell_u32_bytes as usize];
        queue.write_buffer(self.cell_counts_buffer.as_ref().unwrap(), 0, &zeros);
        queue.write_buffer(self.scatter_counters_buffer.as_ref().unwrap(), 0, &zeros);

        self.ped_count = ped_count;
    }

    /// Encode the 6-dispatch pedestrian adaptive pipeline into the command encoder.
    ///
    /// `social_params` must have `grid_w`, `grid_h`, `cell_size`, and `dt` set.
    /// `ped_count` and `workgroup_count` are filled in automatically.
    pub fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        social_params: &PedestrianAdaptiveParams,
    ) {
        if self.ped_count == 0 {
            return;
        }

        let grid_w = social_params.grid_w;
        let grid_h = social_params.grid_h;
        let cell_count = grid_w * grid_h;
        let prefix_wg_count = cell_count.div_ceil(WORKGROUP_SIZE);

        let params = PedestrianAdaptiveParams {
            ped_count: self.ped_count,
            cell_count,
            workgroup_count: prefix_wg_count,
            ..*social_params
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ped_adaptive_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.ped_buffer.as_ref().unwrap().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.cell_counts_buffer.as_ref().unwrap().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.cell_offsets_buffer.as_ref().unwrap().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self
                        .compacted_indices_buffer
                        .as_ref()
                        .unwrap()
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.cell_map_buffer.as_ref().unwrap().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: self
                        .scatter_counters_buffer
                        .as_ref()
                        .unwrap()
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: self
                        .workgroup_sums_buffer
                        .as_ref()
                        .unwrap()
                        .as_entire_binding(),
                },
            ],
        });

        let ped_wg_count = self.ped_count.div_ceil(WORKGROUP_SIZE);

        // Pass 1: Count pedestrians per cell
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ped_count_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.count_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(ped_wg_count, 1, 1);
        }

        // Pass 2a: Per-workgroup prefix sum
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ped_prefix_local_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.prefix_local_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(prefix_wg_count, 1, 1);
        }

        // Pass 2b: Scan workgroup sums (single workgroup)
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ped_prefix_wg_sums_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.prefix_wg_sums_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }

        // Pass 2c: Propagate scanned totals back
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ped_prefix_propagate_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.prefix_propagate_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(prefix_wg_count, 1, 1);
        }

        // Pass 3: Scatter
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ped_scatter_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.scatter_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(ped_wg_count, 1, 1);
        }

        // Pass 4: Social force (one workgroup per cell)
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ped_social_force_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.social_force_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(cell_count, 1, 1);
        }
    }

    /// Read back updated pedestrian states from GPU after adaptive dispatch.
    /// Blocks until GPU completes.
    pub fn readback(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Vec<GpuPedestrian> {
        let count = self.ped_count as usize;
        if count == 0 {
            return Vec::new();
        }

        let byte_size = (count * std::mem::size_of::<GpuPedestrian>()) as u64;
        let staging = self.staging_buffer.as_ref().unwrap();

        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(self.ped_buffer.as_ref().unwrap(), 0, staging, 0, byte_size);
        queue.submit(std::iter::once(encoder.finish()));

        let slice = staging.slice(..byte_size);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());

        let data = slice.get_mapped_range();
        let peds: Vec<GpuPedestrian> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        staging.unmap();

        peds
    }

    /// Classify pedestrian density and return appropriate cell size.
    ///
    /// - Dense (>100 peds/hectare): 2.0m cells
    /// - Medium (10-100 peds/hectare): 5.0m cells
    /// - Sparse (<10 peds/hectare): 10.0m cells
    pub fn classify_density(ped_count: u32, area_sq_m: f32) -> f32 {
        if area_sq_m < 1.0 {
            return 2.0;
        }
        let density_per_hectare = (ped_count as f32) / (area_sq_m / 10_000.0);
        if density_per_hectare > 100.0 {
            2.0
        } else if density_per_hectare > 10.0 {
            5.0
        } else {
            10.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_pedestrian_size() {
        assert_eq!(std::mem::size_of::<GpuPedestrian>(), 32);
    }

    #[test]
    fn pedestrian_params_size() {
        assert_eq!(std::mem::size_of::<PedestrianAdaptiveParams>(), 64);
    }

    #[test]
    fn classify_density_dense() {
        // 200 peds in 10000 sqm = 200/hectare -> dense -> 2.0
        assert_eq!(PedestrianAdaptivePipeline::classify_density(200, 10_000.0), 2.0);
    }

    #[test]
    fn classify_density_medium() {
        // 50 peds in 10000 sqm = 50/hectare -> medium -> 5.0
        assert_eq!(PedestrianAdaptivePipeline::classify_density(50, 10_000.0), 5.0);
    }

    #[test]
    fn classify_density_sparse() {
        // 5 peds in 10000 sqm = 5/hectare -> sparse -> 10.0
        assert_eq!(PedestrianAdaptivePipeline::classify_density(5, 10_000.0), 10.0);
    }

    #[test]
    fn classify_density_tiny_area() {
        assert_eq!(PedestrianAdaptivePipeline::classify_density(10, 0.5), 2.0);
    }

    #[test]
    fn default_params_match_spec() {
        let p = PedestrianAdaptiveParams::default();
        assert!((p.a_social - 2000.0).abs() < 1e-6);
        assert!((p.b_social - 0.08).abs() < 1e-6);
        assert!((p.tau - 0.5).abs() < 1e-6);
        assert!((p.desired_speed - 1.2).abs() < 1e-6);
        assert!((p.lambda - 0.5).abs() < 1e-6);
        assert!((p.max_force - 50.0).abs() < 1e-6);
        assert!((p.max_speed - 2.0).abs() < 1e-6);
        assert!((p.radius - 0.3).abs() < 1e-6);
    }
}
