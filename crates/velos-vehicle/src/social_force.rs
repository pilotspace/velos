//! Pedestrian Helbing social force model.
//!
//! Stub implementation for TDD RED phase.

/// Parameters for the social force model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SocialForceParams {
    pub a: f64,
    pub b: f64,
    pub radius: f64,
    pub tau: f64,
    pub desired_speed: f64,
    pub lambda: f64,
    pub max_force: f64,
    pub max_speed: f64,
}

impl Default for SocialForceParams {
    fn default() -> Self {
        Self {
            a: 2000.0,
            b: 0.08,
            radius: 0.3,
            tau: 0.5,
            desired_speed: 1.2,
            lambda: 0.5,
            max_force: 50.0,
            max_speed: 2.0,
        }
    }
}

/// Neighboring pedestrian state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PedestrianNeighbor {
    pub pos: [f64; 2],
    pub vel: [f64; 2],
    pub radius: f64,
}

/// Trait for random number generation (allows deterministic testing).
pub trait Rng {
    fn gen_f64(&mut self) -> f64;
}

/// Stub -- returns [0, 0].
pub fn social_force_acceleration(
    _pos: [f64; 2],
    _vel: [f64; 2],
    _destination: [f64; 2],
    _neighbors: &[PedestrianNeighbor],
    _params: &SocialForceParams,
) -> [f64; 2] {
    [0.0, 0.0]
}

/// Stub -- returns ([0, 0], 0).
pub fn integrate_pedestrian(
    _vel: [f64; 2],
    _accel: [f64; 2],
    _dt: f64,
    _max_speed: f64,
) -> ([f64; 2], f64) {
    ([0.0, 0.0], 0.0)
}

/// Stub -- returns false.
pub fn should_jaywalk(
    _at_red_light: bool,
    _ttc: f64,
    _gap_acceptance: f64,
    _rng: &mut impl Rng,
) -> bool {
    false
}
