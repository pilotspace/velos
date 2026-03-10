//! velos-core: ECS components and numerical utilities shared by all crates.

pub mod cfl;
pub mod components;
pub mod cost;
pub mod error;
pub mod fixed_point;
pub mod reroute;

pub use cfl::cfl_check;
pub use components::{
    CarFollowingModel, GpuAgentState, JunctionTraversal, JustExitedJunction, Kinematics, Position,
    RoadPosition, Route, VehicleType, WaitState, MAX_YIELD_TICKS,
};
pub use cost::{
    AgentProfile, CostWeights, EdgeAttributes, RoadClass as CostRoadClass,
    PROFILE_WEIGHTS, decode_profile_from_flags, default_edge_attributes,
    encode_profile_in_flags, route_cost,
};
pub use error::CoreError;
pub use fixed_point::{FixLat, FixPos, FixSpd};
pub use reroute::{
    evaluate_reroute, PerceptionSnapshot, RerouteConfig, RerouteResult, RerouteScheduler,
    RouteEvalContext,
};
