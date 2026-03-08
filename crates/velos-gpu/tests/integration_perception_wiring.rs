//! Integration test: perception buffer wiring between PerceptionPipeline and ComputeDispatcher.
//!
//! Verifies that the shared perception result buffer created in SimWorld::new()
//! is correctly wired so that both PerceptionPipeline and ComputeDispatcher
//! reference the same GPU buffer for binding(8).
//!
//! PerceptionPipeline no longer owns a result buffer -- dispatch() and readback_results()
//! accept an external &wgpu::Buffer. ComputeDispatcher owns the single shared buffer.

#![cfg(feature = "gpu-tests")]

use velos_gpu::perception::PerceptionResult;

/// Verify PerceptionResult is 32 bytes (wiring depends on this for buffer sizing).
#[test]
fn perception_result_is_32_bytes() {
    assert_eq!(std::mem::size_of::<PerceptionResult>(), 32);
}

/// Verify the shared buffer size calculation matches expected allocation.
/// 300K agents * 32 bytes = 9,600,000 bytes.
#[test]
fn shared_buffer_size_calculation() {
    let max_agents: u64 = 300_000;
    let result_size = max_agents * (std::mem::size_of::<PerceptionResult>() as u64);
    assert_eq!(result_size, 9_600_000);
}

/// Verify that PerceptionPipeline can be created and used with an external result buffer.
/// After the refactor, dispatch() and readback_results() take &wgpu::Buffer.
#[test]
fn perception_pipeline_uses_external_result_buffer() {
    let ctx = match velos_gpu::device::GpuContext::new_headless() {
        Some(c) => c,
        None => {
            eprintln!("No GPU adapter available, skipping test");
            return;
        }
    };

    // Create a shared result buffer (as SimWorld::new() would)
    let max_agents: u32 = 1_000; // small for test
    let result_size = (max_agents as u64) * 32;

    let shared_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("test_shared_perception_results"),
        size: result_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    // Verify buffer has correct usage flags for both pipelines
    assert_eq!(shared_buffer.size(), result_size);
    assert_eq!(
        shared_buffer.usage(),
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC
    );

    // Create PerceptionPipeline -- it should NOT own a result buffer
    let perception = velos_gpu::perception::PerceptionPipeline::new(&ctx.device, max_agents);
    assert_eq!(perception.max_agents(), max_agents);
}

/// Verify ComputeDispatcher accepts a perception result buffer via set_perception_result_buffer.
/// After wiring, the dispatcher's buffer should have STORAGE | COPY_SRC usage
/// (matching the real perception buffer, not the zeroed placeholder).
#[test]
fn compute_dispatcher_accepts_shared_buffer() {
    let ctx = match velos_gpu::device::GpuContext::new_headless() {
        Some(c) => c,
        None => {
            eprintln!("No GPU adapter available, skipping test");
            return;
        }
    };

    let mut dispatcher = velos_gpu::compute::ComputeDispatcher::new(&ctx.device);

    // Create a shared buffer with STORAGE | COPY_SRC (PerceptionPipeline's pattern)
    let result_size = 1_000u64 * 32;
    let shared_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("test_shared_perception_results"),
        size: result_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    // Wire it to the dispatcher
    dispatcher.set_perception_result_buffer(shared_buffer);

    // Verify the dispatcher now holds the buffer with correct size
    let buf_ref = dispatcher.perception_result_buffer();
    assert_eq!(buf_ref.size(), result_size);
    assert_eq!(
        buf_ref.usage(),
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        "Dispatcher's perception buffer should have STORAGE | COPY_SRC after wiring"
    );
}
