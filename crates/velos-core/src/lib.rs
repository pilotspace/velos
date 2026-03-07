//! velos-core: ECS components and numerical utilities shared by all crates.

pub mod cfl;
pub mod components;
pub mod error;
pub mod fixed_point;

pub use cfl::cfl_check;
pub use components::{
    CarFollowingModel, GpuAgentState, Kinematics, Position, RoadPosition, Route, VehicleType,
    WaitState,
};
pub use error::CoreError;
pub use fixed_point::{FixLat, FixPos, FixSpd};
