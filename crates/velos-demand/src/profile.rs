//! Profile assignment for demand spawning.
//!
//! Maps vehicle types to agent profiles at spawn time. Fixed vehicle types
//! (Bus, Truck, Emergency, Bicycle) get 1:1 profile mappings. Car and Motorbike
//! are randomly distributed across Commuter, Tourist, Teen, and Senior profiles
//! per configurable percentages.

use rand::Rng;
use velos_core::cost::AgentProfile;

use crate::spawner::SpawnVehicleType;

/// Configurable distribution percentages for Car/Motorbike profile assignment.
///
/// These percentages control how Car and Motorbike agents are distributed
/// across the four variable profiles. Must sum to 1.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProfileDistribution {
    /// Fraction assigned Commuter profile (default 0.60).
    pub commuter_pct: f32,
    /// Fraction assigned Tourist profile (default 0.15).
    pub tourist_pct: f32,
    /// Fraction assigned Teen profile (default 0.15).
    pub teen_pct: f32,
    /// Fraction assigned Senior profile (default 0.10).
    pub senior_pct: f32,
}

impl Default for ProfileDistribution {
    fn default() -> Self {
        Self {
            commuter_pct: 0.60,
            tourist_pct: 0.15,
            teen_pct: 0.15,
            senior_pct: 0.10,
        }
    }
}

impl ProfileDistribution {
    /// Validate that percentages sum to approximately 1.0.
    ///
    /// Returns an error string if the sum deviates by more than 1e-3.
    pub fn validate(&self) -> Result<(), String> {
        let sum = self.commuter_pct + self.tourist_pct + self.teen_pct + self.senior_pct;
        if (sum - 1.0).abs() > 1e-3 {
            return Err(format!(
                "Profile distribution percentages sum to {sum}, expected 1.0"
            ));
        }
        Ok(())
    }
}

/// Assign an agent profile based on vehicle type and distribution config.
///
/// Fixed mappings:
/// - Bus -> Bus, Truck -> Truck, Emergency -> Emergency, Bicycle -> Cyclist, Pedestrian -> Commuter
///
/// Variable mappings (Car, Motorbike):
/// - Random selection from distribution using provided RNG
pub fn assign_profile(
    vehicle_type: SpawnVehicleType,
    distribution: &ProfileDistribution,
    rng: &mut impl Rng,
) -> AgentProfile {
    match vehicle_type {
        SpawnVehicleType::Bus => AgentProfile::Bus,
        SpawnVehicleType::Truck => AgentProfile::Truck,
        SpawnVehicleType::Emergency => AgentProfile::Emergency,
        SpawnVehicleType::Bicycle => AgentProfile::Cyclist,
        SpawnVehicleType::Pedestrian => AgentProfile::Commuter,
        SpawnVehicleType::Car | SpawnVehicleType::Motorbike => {
            let r: f32 = rng.r#gen();
            let c1 = distribution.commuter_pct;
            let c2 = c1 + distribution.tourist_pct;
            let c3 = c2 + distribution.teen_pct;

            if r < c1 {
                AgentProfile::Commuter
            } else if r < c2 {
                AgentProfile::Tourist
            } else if r < c3 {
                AgentProfile::Teen
            } else {
                AgentProfile::Senior
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn bus_always_gets_bus_profile() {
        let dist = ProfileDistribution::default();
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            assert_eq!(assign_profile(SpawnVehicleType::Bus, &dist, &mut rng), AgentProfile::Bus);
        }
    }

    #[test]
    fn truck_always_gets_truck_profile() {
        let dist = ProfileDistribution::default();
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            assert_eq!(assign_profile(SpawnVehicleType::Truck, &dist, &mut rng), AgentProfile::Truck);
        }
    }

    #[test]
    fn emergency_always_gets_emergency_profile() {
        let dist = ProfileDistribution::default();
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            assert_eq!(assign_profile(SpawnVehicleType::Emergency, &dist, &mut rng), AgentProfile::Emergency);
        }
    }

    #[test]
    fn bicycle_always_gets_cyclist_profile() {
        let dist = ProfileDistribution::default();
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            assert_eq!(assign_profile(SpawnVehicleType::Bicycle, &dist, &mut rng), AgentProfile::Cyclist);
        }
    }

    #[test]
    fn car_distributes_across_profiles() {
        let dist = ProfileDistribution::default();
        let mut rng = StdRng::seed_from_u64(42);
        let n = 10_000;
        let mut counts = [0u32; 4]; // commuter, tourist, teen, senior

        for _ in 0..n {
            match assign_profile(SpawnVehicleType::Car, &dist, &mut rng) {
                AgentProfile::Commuter => counts[0] += 1,
                AgentProfile::Tourist => counts[1] += 1,
                AgentProfile::Teen => counts[2] += 1,
                AgentProfile::Senior => counts[3] += 1,
                other => panic!("Unexpected profile {other:?} for Car"),
            }
        }

        let nf = n as f32;
        // Allow 3% tolerance for statistical variation
        assert!((counts[0] as f32 / nf - 0.60).abs() < 0.03, "Commuter: {}", counts[0] as f32 / nf);
        assert!((counts[1] as f32 / nf - 0.15).abs() < 0.03, "Tourist: {}", counts[1] as f32 / nf);
        assert!((counts[2] as f32 / nf - 0.15).abs() < 0.03, "Teen: {}", counts[2] as f32 / nf);
        assert!((counts[3] as f32 / nf - 0.10).abs() < 0.03, "Senior: {}", counts[3] as f32 / nf);
    }

    #[test]
    fn motorbike_distributes_across_profiles() {
        let dist = ProfileDistribution::default();
        let mut rng = StdRng::seed_from_u64(123);
        let n = 10_000;
        let mut counts = [0u32; 4];

        for _ in 0..n {
            match assign_profile(SpawnVehicleType::Motorbike, &dist, &mut rng) {
                AgentProfile::Commuter => counts[0] += 1,
                AgentProfile::Tourist => counts[1] += 1,
                AgentProfile::Teen => counts[2] += 1,
                AgentProfile::Senior => counts[3] += 1,
                other => panic!("Unexpected profile {other:?} for Motorbike"),
            }
        }

        let nf = n as f32;
        assert!((counts[0] as f32 / nf - 0.60).abs() < 0.03, "Commuter: {}", counts[0] as f32 / nf);
        assert!((counts[1] as f32 / nf - 0.15).abs() < 0.03, "Tourist: {}", counts[1] as f32 / nf);
        assert!((counts[2] as f32 / nf - 0.15).abs() < 0.03, "Teen: {}", counts[2] as f32 / nf);
        assert!((counts[3] as f32 / nf - 0.10).abs() < 0.03, "Senior: {}", counts[3] as f32 / nf);
    }

    #[test]
    fn default_distribution_has_correct_percentages() {
        let dist = ProfileDistribution::default();
        assert!((dist.commuter_pct - 0.60).abs() < f32::EPSILON);
        assert!((dist.tourist_pct - 0.15).abs() < f32::EPSILON);
        assert!((dist.teen_pct - 0.15).abs() < f32::EPSILON);
        assert!((dist.senior_pct - 0.10).abs() < f32::EPSILON);
        dist.validate().unwrap();
    }

    #[test]
    fn seeded_rng_produces_deterministic_assignment() {
        let dist = ProfileDistribution::default();

        let mut rng1 = StdRng::seed_from_u64(999);
        let mut rng2 = StdRng::seed_from_u64(999);

        for _ in 0..100 {
            let p1 = assign_profile(SpawnVehicleType::Car, &dist, &mut rng1);
            let p2 = assign_profile(SpawnVehicleType::Car, &dist, &mut rng2);
            assert_eq!(p1, p2, "Seeded RNG should produce identical results");
        }
    }

    #[test]
    fn invalid_distribution_rejected() {
        let dist = ProfileDistribution {
            commuter_pct: 0.50,
            tourist_pct: 0.50,
            teen_pct: 0.50,
            senior_pct: 0.50,
        };
        assert!(dist.validate().is_err());
    }
}
