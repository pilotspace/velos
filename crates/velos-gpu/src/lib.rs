//! velos-gpu: GPU device management, compute dispatch, and rendering.
//! Exposes high-level API only -- no raw wgpu types in public API.

pub mod app;
pub mod buffers;
pub mod camera;
pub mod compute;
mod compute_wave_front;
pub mod device;
pub mod ped_adaptive;
pub mod perception;
pub mod error;
pub mod multi_gpu;
pub mod partition;
pub mod map_tiles;
pub mod renderer;
pub mod sim;
pub mod sim_startup;
pub mod cpu_reference;
mod sim_helpers;
mod sim_mobil;
mod sim_lifecycle;
mod sim_render;
mod sim_bus;
pub mod sim_meso;
mod sim_pedestrians;
mod sim_perception;
mod sim_signals;
mod sim_vehicles;
mod sim_reroute;
pub mod sim_snapshot;

pub use app::VelosApp;
pub use buffers::{BufferPool, GpuKinematics, GpuPosition};
pub use camera::Camera2D;
pub use compute::ComputeDispatcher;
pub use ped_adaptive::{GpuPedestrian, PedestrianAdaptiveParams, PedestrianAdaptivePipeline};
pub use perception::{PerceptionBindings, PerceptionParams, PerceptionPipeline, PerceptionResult};
pub use device::GpuContext;
pub use error::GpuError;
pub use map_tiles::{MapTileRenderer, TileVertex};
pub use renderer::{AgentInstance, Renderer};
