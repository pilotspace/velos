//! Pedestrian Helbing social force model.
//!
//! Computes acceleration for pedestrian agents using:
//! - Driving force toward destination (desired velocity / relaxation time)
//! - Repulsive forces from other pedestrians (exponential, anisotropic)
//! - Force and speed clamping to prevent numerical explosion
//!
//! All functions are pure (no ECS dependency) following the IDM/MOBIL pattern.
//!
//! Reference: Helbing & Molnar (1995), "Social force model for pedestrian dynamics"

/// Parameters for the Helbing social force model.
///
/// Default values tuned for HCMC pedestrian dynamics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SocialForceParams {
    /// Repulsion strength (N). Controls magnitude of pedestrian-pedestrian avoidance.
    pub a: f64,
    /// Repulsion range (m). Controls how quickly repulsion decays with distance.
    pub b: f64,
    /// Body radius of the ego pedestrian (m).
    pub radius: f64,
    /// Relaxation time (s). How quickly the agent adjusts to desired velocity.
    pub tau: f64,
    /// Desired walking speed (m/s). HCMC average walking speed.
    pub desired_speed: f64,
    /// Anisotropy parameter (0..1). 0.5 = forward-biased vision cone.
    /// Reduces influence of agents behind the pedestrian.
    pub lambda: f64,
    /// Maximum repulsive force per neighbor (N). Prevents explosion on overlap.
    pub max_force: f64,
    /// Maximum pedestrian speed (m/s). Hard speed clamp after integration.
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

/// Neighboring pedestrian state for social force computation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PedestrianNeighbor {
    /// World position [x, y] in metres.
    pub pos: [f64; 2],
    /// Velocity [vx, vy] in m/s.
    pub vel: [f64; 2],
    /// Body radius (m).
    pub radius: f64,
}

/// Trait for random number generation (allows deterministic testing).
pub trait Rng {
    /// Generate a random f64 in [0, 1).
    fn gen_f64(&mut self) -> f64;
}

/// Compute social force acceleration for a pedestrian.
///
/// Returns total acceleration `[ax, ay]` combining driving force and
/// repulsive forces from all neighbors.
///
/// # Arguments
/// * `pos` - ego position [x, y] (m)
/// * `vel` - ego velocity [vx, vy] (m/s)
/// * `destination` - target position [x, y] (m)
/// * `neighbors` - nearby pedestrian agents
/// * `params` - social force parameters
pub fn social_force_acceleration(
    pos: [f64; 2],
    vel: [f64; 2],
    destination: [f64; 2],
    neighbors: &[PedestrianNeighbor],
    params: &SocialForceParams,
) -> [f64; 2] {
    // Driving force: (v_desired - v_current) / tau
    let dx = destination[0] - pos[0];
    let dy = destination[1] - pos[1];
    let dist_to_dest = (dx * dx + dy * dy).sqrt();

    let driving = if dist_to_dest > 1e-6 {
        let dir_x = dx / dist_to_dest;
        let dir_y = dy / dist_to_dest;
        let desired_vx = params.desired_speed * dir_x;
        let desired_vy = params.desired_speed * dir_y;
        [
            (desired_vx - vel[0]) / params.tau,
            (desired_vy - vel[1]) / params.tau,
        ]
    } else {
        // Already at destination -- decelerate
        [-vel[0] / params.tau, -vel[1] / params.tau]
    };

    // Repulsive forces from neighbors
    let mut repulsion = [0.0_f64; 2];

    // Ego movement direction for anisotropy
    let ego_speed = (vel[0] * vel[0] + vel[1] * vel[1]).sqrt();
    let ego_dir = if ego_speed > 1e-6 {
        [vel[0] / ego_speed, vel[1] / ego_speed]
    } else {
        // Use direction to destination as fallback
        if dist_to_dest > 1e-6 {
            [dx / dist_to_dest, dy / dist_to_dest]
        } else {
            [1.0, 0.0] // arbitrary default
        }
    };

    for n in neighbors {
        let nx = pos[0] - n.pos[0]; // vector FROM neighbor TO ego
        let ny = pos[1] - n.pos[1];
        let dist = (nx * nx + ny * ny).sqrt();

        if dist < 1e-6 {
            // Overlapping -- apply max force in arbitrary direction
            repulsion[0] += params.max_force;
            continue;
        }

        let unit_x = nx / dist;
        let unit_y = ny / dist;

        // Sum of radii
        let r_sum = params.radius + n.radius;

        // Exponential repulsive force magnitude: A * exp((r_sum - d) / B)
        let force_mag = params.a * ((r_sum - dist) / params.b).exp();

        // Clamp per-neighbor force
        let force_mag = force_mag.min(params.max_force);

        // Anisotropic weighting: agents in front get full weight, behind get reduced
        // cos(phi) = dot(ego_dir, direction_to_neighbor)
        // direction_to_neighbor is FROM ego TO neighbor = [-unit_x, -unit_y]
        let cos_phi = -(ego_dir[0] * unit_x + ego_dir[1] * unit_y);
        let weight = params.lambda + (1.0 - params.lambda) * (1.0 + cos_phi) / 2.0;

        repulsion[0] += weight * force_mag * unit_x;
        repulsion[1] += weight * force_mag * unit_y;
    }

    [driving[0] + repulsion[0], driving[1] + repulsion[1]]
}

/// Integrate pedestrian velocity with speed clamping.
///
/// Applies acceleration to velocity via forward Euler, then clamps speed
/// to `max_speed` to prevent unrealistic sprinting.
///
/// # Arguments
/// * `vel` - current velocity [vx, vy] (m/s)
/// * `accel` - acceleration [ax, ay] (m/s^2)
/// * `dt` - timestep (s)
/// * `max_speed` - maximum allowed speed (m/s)
///
/// # Returns
/// `(new_vel, speed_magnitude)` where speed <= max_speed.
pub fn integrate_pedestrian(
    vel: [f64; 2],
    accel: [f64; 2],
    dt: f64,
    max_speed: f64,
) -> ([f64; 2], f64) {
    let new_vx = vel[0] + accel[0] * dt;
    let new_vy = vel[1] + accel[1] * dt;
    let speed = (new_vx * new_vx + new_vy * new_vy).sqrt();

    if speed > max_speed && speed > 1e-10 {
        let scale = max_speed / speed;
        ([new_vx * scale, new_vy * scale], max_speed)
    } else {
        ([new_vx, new_vy], speed)
    }
}

/// Decide whether a pedestrian should jaywalk.
///
/// Probability depends on context:
/// - At red light: 30% chance (HCMC-typical impatient crossing)
/// - Mid-block: 10% chance per opportunity window
///
/// Rejects crossing if time-to-collision with nearest vehicle is below
/// the gap acceptance threshold.
///
/// # Arguments
/// * `at_red_light` - true if pedestrian is at a signalized crossing on red
/// * `ttc_to_nearest_vehicle` - time-to-collision with nearest approaching vehicle (s)
/// * `gap_acceptance_time` - minimum acceptable TTC to cross (s, default 2.0)
/// * `rng` - random number generator
pub fn should_jaywalk(
    at_red_light: bool,
    ttc_to_nearest_vehicle: f64,
    gap_acceptance_time: f64,
    rng: &mut impl Rng,
) -> bool {
    // Safety check: reject if gap too small
    if ttc_to_nearest_vehicle < gap_acceptance_time {
        return false;
    }

    let probability = if at_red_light { 0.3 } else { 0.1 };
    rng.gen_f64() < probability
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_params_match_spec() {
        let p = SocialForceParams::default();
        assert!((p.a - 2000.0).abs() < 1e-10);
        assert!((p.b - 0.08).abs() < 1e-10);
        assert!((p.radius - 0.3).abs() < 1e-10);
        assert!((p.tau - 0.5).abs() < 1e-10);
        assert!((p.desired_speed - 1.2).abs() < 1e-10);
        assert!((p.lambda - 0.5).abs() < 1e-10);
        assert!((p.max_force - 50.0).abs() < 1e-10);
        assert!((p.max_speed - 2.0).abs() < 1e-10);
    }

    #[test]
    fn at_destination_decelerates() {
        let params = SocialForceParams::default();
        let pos = [10.0, 5.0];
        let vel = [1.0, 0.0];
        let dest = [10.0, 5.0]; // already there

        let accel = social_force_acceleration(pos, vel, dest, &[], &params);
        // Should decelerate: accel opposes current velocity
        assert!(accel[0] < 0.0, "should decelerate: ax={:.4}", accel[0]);
    }

    #[test]
    fn integrate_zero_accel() {
        let (new_vel, speed) = integrate_pedestrian([1.0, 0.0], [0.0, 0.0], 0.1, 2.0);
        assert!((new_vel[0] - 1.0).abs() < 1e-10);
        assert!((new_vel[1]).abs() < 1e-10);
        assert!((speed - 1.0).abs() < 1e-10);
    }
}
