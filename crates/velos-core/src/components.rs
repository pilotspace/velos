//! ECS component definitions for agent state.
//! All fields use f64 for CPU-side precision.

/// World-space position of an agent in metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    /// East-west coordinate (metres).
    pub x: f64,
    /// North-south coordinate (metres).
    pub y: f64,
}

/// Kinematic state of an agent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Kinematics {
    /// Velocity in the x direction (m/s).
    pub vx: f64,
    /// Velocity in the y direction (m/s).
    pub vy: f64,
    /// Scalar speed magnitude (m/s).
    pub speed: f64,
    /// Heading angle in radians (0 = east, CCW positive).
    pub heading: f64,
}
