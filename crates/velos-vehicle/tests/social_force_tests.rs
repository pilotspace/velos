//! Tests for pedestrian Helbing social force model.

use velos_vehicle::social_force::{
    integrate_pedestrian, should_jaywalk, social_force_acceleration, PedestrianNeighbor,
    SocialForceParams,
};

fn default_params() -> SocialForceParams {
    SocialForceParams::default()
}

// ---------- Driving force ----------

#[test]
fn driving_force_accelerates_toward_destination() {
    let params = default_params();
    let pos = [0.0, 0.0];
    let vel = [0.0, 0.0]; // stationary
    let dest = [10.0, 0.0]; // destination to the east

    let accel = social_force_acceleration(pos, vel, dest, &[], &params);
    // Should accelerate in +x direction (toward destination)
    assert!(
        accel[0] > 0.0,
        "driving force x should be positive: {:.4}",
        accel[0]
    );
    assert!(
        accel[1].abs() < 1e-6,
        "driving force y should be near zero: {:.4}",
        accel[1]
    );
}

#[test]
fn driving_force_direction_is_correct() {
    let params = default_params();
    let pos = [5.0, 5.0];
    let vel = [0.0, 0.0];
    let dest = [5.0, 10.0]; // destination to the north

    let accel = social_force_acceleration(pos, vel, dest, &[], &params);
    assert!(
        accel[1] > 0.0,
        "should accelerate in +y (toward dest): {:.4}",
        accel[1]
    );
    assert!(
        accel[0].abs() < 1e-6,
        "x component should be zero: {:.4}",
        accel[0]
    );
}

// ---------- Repulsion ----------

#[test]
fn repulsion_pushes_pedestrians_apart() {
    let params = default_params();
    let pos = [0.0, 0.0];
    let vel = [0.0, 0.0];
    let dest = [10.0, 0.0]; // going east

    let neighbor = PedestrianNeighbor {
        pos: [0.5, 0.0], // very close to the east
        vel: [0.0, 0.0],
        radius: 0.3,
    };

    let accel = social_force_acceleration(pos, vel, dest, &[neighbor], &params);
    // Repulsion from neighbor to the right should push us left (negative x relative to neighbor)
    // But driving force also pushes us east. Repulsion at close range should dominate or
    // at least reduce the eastward acceleration significantly.
    let accel_no_neighbor = social_force_acceleration(pos, vel, dest, &[], &params);
    assert!(
        accel[0] < accel_no_neighbor[0],
        "repulsion should reduce eastward accel: with={:.4}, without={:.4}",
        accel[0],
        accel_no_neighbor[0]
    );
}

// ---------- Anisotropic vision ----------

#[test]
fn anisotropic_weighting_reduces_force_from_behind() {
    let params = default_params();
    let pos = [5.0, 0.0];
    let vel = [1.0, 0.0]; // walking east

    // Neighbor ahead (east)
    let neighbor_ahead = PedestrianNeighbor {
        pos: [5.8, 0.0],
        vel: [0.0, 0.0],
        radius: 0.3,
    };

    // Neighbor behind (west), same distance
    let neighbor_behind = PedestrianNeighbor {
        pos: [4.2, 0.0],
        vel: [0.0, 0.0],
        radius: 0.3,
    };

    let dest = [20.0, 0.0];

    let accel_ahead = social_force_acceleration(pos, vel, dest, &[neighbor_ahead], &params);
    let accel_behind = social_force_acceleration(pos, vel, dest, &[neighbor_behind], &params);

    // Driving force is same in both cases, so difference is due to repulsion magnitude.
    // Agent ahead should cause stronger repulsion than agent behind (anisotropic).
    let accel_no_neighbor = social_force_acceleration(pos, vel, dest, &[], &params);

    let repulsion_from_ahead = (accel_ahead[0] - accel_no_neighbor[0]).abs();
    let repulsion_from_behind = (accel_behind[0] - accel_no_neighbor[0]).abs();

    assert!(
        repulsion_from_ahead > repulsion_from_behind,
        "repulsion from ahead ({:.4}) should be stronger than from behind ({:.4})",
        repulsion_from_ahead,
        repulsion_from_behind
    );
}

// ---------- Force clamping ----------

#[test]
fn force_clamped_to_max_force() {
    let params = default_params();
    let pos = [0.0, 0.0];
    let vel = [0.0, 0.0];
    let dest = [10.0, 0.0];

    // Overlapping neighbor -- should produce very large repulsion, but clamped
    let neighbor = PedestrianNeighbor {
        pos: [0.1, 0.0], // almost on top of us
        vel: [0.0, 0.0],
        radius: 0.3,
    };

    let accel = social_force_acceleration(pos, vel, dest, &[neighbor], &params);
    let accel_mag = (accel[0] * accel[0] + accel[1] * accel[1]).sqrt();
    // Total accel = driving + repulsive. The repulsive alone should be clamped.
    // Driving force magnitude = desired_speed / tau = 1.2 / 0.5 = 2.4
    // So total magnitude should be at most max_force + driving_force_mag
    let max_total = params.max_force + params.desired_speed / params.tau + 1.0; // small buffer
    assert!(
        accel_mag < max_total,
        "accel magnitude {:.2} should not explode (max ~{:.2})",
        accel_mag,
        max_total
    );
}

// ---------- Speed clamping ----------

#[test]
fn speed_clamped_to_max_speed() {
    let params = default_params();
    // Very high velocity, should be clamped after integration
    let vel = [5.0, 5.0];
    let accel = [10.0, 10.0];

    let (new_vel, speed) = integrate_pedestrian(vel, accel, 1.0, params.max_speed);
    assert!(
        speed <= params.max_speed + 1e-10,
        "speed {:.4} must be <= max_speed {:.1}",
        speed,
        params.max_speed
    );
    let new_speed = (new_vel[0] * new_vel[0] + new_vel[1] * new_vel[1]).sqrt();
    assert!(
        (new_speed - speed).abs() < 1e-10,
        "returned speed should match velocity magnitude"
    );
}

// ---------- No neighbors ----------

#[test]
fn no_neighbors_returns_pure_driving_force() {
    let params = default_params();
    let pos = [0.0, 0.0];
    let vel = [0.5, 0.0];
    let dest = [10.0, 0.0];

    let accel = social_force_acceleration(pos, vel, dest, &[], &params);
    // Driving force = (desired_speed * direction - vel) / tau
    let desired_vx = params.desired_speed; // direction is [1,0]
    let expected_ax = (desired_vx - 0.5) / params.tau;
    assert!(
        (accel[0] - expected_ax).abs() < 1e-6,
        "pure driving force: got {:.4}, expected {:.4}",
        accel[0],
        expected_ax
    );
}

// ---------- Jaywalking ----------

#[test]
fn jaywalking_red_light_probability() {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut count = 0;
    for i in 0..1000 {
        let mut hasher = DefaultHasher::new();
        (i as u64 * 12345 + 67890).hash(&mut hasher);
        let seed = hasher.finish();
        let mut rng = simple_rng(seed);
        if should_jaywalk(true, 10.0, 2.0, &mut rng) {
            count += 1;
        }
    }
    // Expected ~300 (30% at red light), tolerance +/- 80
    assert!(
        (220..=380).contains(&count),
        "red light jaywalking: {count}/1000 should be ~300 (30%)"
    );
}

#[test]
fn jaywalking_midblock_probability() {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut count = 0;
    for i in 0..1000 {
        let mut hasher = DefaultHasher::new();
        (i as u64 * 54321 + 98765).hash(&mut hasher);
        let seed = hasher.finish();
        let mut rng = simple_rng(seed);
        if should_jaywalk(false, 10.0, 2.0, &mut rng) {
            count += 1;
        }
    }
    // Expected ~100 (10% mid-block), tolerance +/- 60
    assert!(
        (40..=160).contains(&count),
        "mid-block jaywalking: {count}/1000 should be ~100 (10%)"
    );
}

#[test]
fn jaywalking_rejects_when_ttc_too_low() {
    let seed = 42u64;
    let mut rng = simple_rng(seed);
    // TTC = 1.5s < gap_acceptance = 2.0s -- should always reject
    let result = should_jaywalk(true, 1.5, 2.0, &mut rng);
    assert!(!result, "should reject jaywalking when TTC < gap_acceptance");
}

/// Simple deterministic RNG for tests (xorshift64).
struct SimpleRng(u64);

fn simple_rng(seed: u64) -> SimpleRng {
    SimpleRng(if seed == 0 { 1 } else { seed })
}

impl SimpleRng {
    fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 as f64) / (u64::MAX as f64)
    }
}

/// Trait adapter so `should_jaywalk` can accept our test RNG.
impl velos_vehicle::social_force::Rng for SimpleRng {
    fn gen_f64(&mut self) -> f64 {
        self.next_f64()
    }
}
