//! velos-gpu: GPU device management, compute dispatch, and rendering.
//! Exposes high-level API only -- no raw wgpu types in public API.

pub mod app;
pub mod buffers;
pub mod camera;
pub mod compute;
pub mod device;
pub mod error;
pub mod multi_gpu;
pub mod partition;
pub mod renderer;
pub mod sim;
mod sim_helpers;
mod sim_mobil;
mod sim_lifecycle;
mod sim_render;
pub mod sim_snapshot;

pub use app::VelosApp;
pub use buffers::{BufferPool, GpuKinematics, GpuPosition};
pub use camera::Camera2D;
pub use compute::ComputeDispatcher;
pub use device::GpuContext;
pub use error::GpuError;
pub use renderer::{AgentInstance, Renderer};
