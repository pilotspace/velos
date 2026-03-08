//! Vehicle behavior configuration loaded from TOML.
//!
//! Provides [`VehicleConfig`] as the single source of truth for all
//! per-vehicle-type behavior parameters. Values are HCMC-calibrated
//! defaults that replace the hardcoded literature values.

use serde::Deserialize;

use crate::error::VehicleError;
use crate::idm::IdmParams;
use crate::krauss::KraussParams;
use crate::mobil::MobilParams;
use crate::sublane::SublaneParams;
use crate::types::VehicleType;

/// Top-level vehicle configuration loaded from `vehicle_params.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct VehicleConfig {
    pub motorbike: VehicleTypeParams,
    pub car: VehicleTypeParams,
    pub bus: VehicleTypeParams,
    pub truck: VehicleTypeParams,
    pub bicycle: VehicleTypeParams,
    pub emergency: VehicleTypeParams,
    pub pedestrian: PedestrianParams,
}

/// Per-vehicle-type behavior parameters (IDM + Krauss + MOBIL + sublane).
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct VehicleTypeParams {
    /// Desired free-flow speed (m/s).
    pub v0: f64,
    /// Minimum gap at standstill (m).
    pub s0: f64,
    /// Desired time headway (s).
    pub t_headway: f64,
    /// Maximum acceleration (m/s^2).
    pub a: f64,
    /// Comfortable braking deceleration (m/s^2, positive).
    pub b: f64,
    /// Free acceleration exponent (typically 4.0).
    pub delta: f64,
    /// Krauss maximum acceleration (m/s^2).
    pub krauss_accel: f64,
    /// Krauss maximum deceleration (m/s^2, positive).
    pub krauss_decel: f64,
    /// Krauss driver imperfection [0.0, 1.0].
    pub krauss_sigma: f64,
    /// Krauss reaction time (s).
    pub krauss_tau: f64,
    /// Krauss maximum speed (m/s).
    pub krauss_max_speed: f64,
    /// Krauss minimum gap at standstill (m).
    pub krauss_min_gap: f64,
    /// MOBIL politeness factor [0.0, 1.0].
    pub politeness: f64,
    /// MOBIL minimum acceleration advantage threshold (m/s^2).
    pub threshold: f64,
    /// MOBIL maximum safe deceleration for new follower (m/s^2, negative).
    pub safe_decel: f64,
    /// MOBIL right-lane bias (m/s^2).
    pub right_bias: f64,
    /// Gap acceptance time-to-collision (s).
    pub gap_acceptance_ttc: f64,
    /// Sublane minimum lateral gap for filtering (m). None for lane-based vehicles.
    pub min_filter_gap: Option<f64>,
    /// Sublane maximum lateral drift speed (m/s). None for lane-based vehicles.
    pub max_lateral_speed: Option<f64>,
    /// Sublane half-width of vehicle body (m). None for lane-based vehicles.
    pub half_width: Option<f64>,
    /// Sublane lateral drift speed during red-light swarming (m/s).
    pub swarm_lateral_speed: Option<f64>,
}

/// Pedestrian-specific parameters (social force model).
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PedestrianParams {
    /// Desired walking speed (m/s).
    pub desired_speed: f64,
    /// Personal space radius (m).
    pub personal_space: f64,
    /// Jaywalking probability on arterial roads [0.0, 1.0].
    pub jaywalking_rate_arterial: f64,
    /// Jaywalking probability on local streets [0.0, 1.0].
    pub jaywalking_rate_local: f64,
    /// Gap acceptance time-to-collision for crossing (s).
    pub gap_acceptance_ttc: f64,
}

/// Load vehicle configuration from a TOML file path.
pub fn load_vehicle_config(path: &str) -> Result<VehicleConfig, VehicleError> {
    let content = std::fs::read_to_string(path).map_err(|e| VehicleError::ConfigLoad {
        path: path.to_string(),
        reason: e.to_string(),
    })?;
    load_vehicle_config_from_str(&content)
}

/// Load vehicle configuration from a TOML string (for tests).
pub fn load_vehicle_config_from_str(toml_str: &str) -> Result<VehicleConfig, VehicleError> {
    let config: VehicleConfig =
        toml::from_str(toml_str).map_err(|e| VehicleError::ConfigParse(e.to_string()))?;
    config.validate()?;
    Ok(config)
}

impl VehicleConfig {
    /// Look up parameters for a specific vehicle type.
    ///
    /// Panics for `VehicleType::Pedestrian` -- use `self.pedestrian` directly.
    pub fn for_vehicle_type(&self, vt: VehicleType) -> &VehicleTypeParams {
        match vt {
            VehicleType::Motorbike => &self.motorbike,
            VehicleType::Car => &self.car,
            VehicleType::Bus => &self.bus,
            VehicleType::Bicycle => &self.bicycle,
            VehicleType::Truck => &self.truck,
            VehicleType::Emergency => &self.emergency,
            VehicleType::Pedestrian => {
                panic!("Use VehicleConfig::pedestrian for pedestrian params")
            }
        }
    }

    /// Validate all parameter ranges. Returns errors for any out-of-range values.
    pub fn validate(&self) -> Result<(), VehicleError> {
        let mut errors = Vec::new();

        let types: &[(&str, &VehicleTypeParams)] = &[
            ("motorbike", &self.motorbike),
            ("car", &self.car),
            ("bus", &self.bus),
            ("truck", &self.truck),
            ("bicycle", &self.bicycle),
            ("emergency", &self.emergency),
        ];

        for (name, p) in types {
            if p.v0 <= 0.0 {
                errors.push(format!("{name}.v0 must be positive, got {}", p.v0));
            }
            if p.s0 <= 0.0 {
                errors.push(format!("{name}.s0 must be positive, got {}", p.s0));
            }
            if p.t_headway <= 0.0 {
                errors.push(format!(
                    "{name}.t_headway must be positive, got {}",
                    p.t_headway
                ));
            }
            if p.a <= 0.0 {
                errors.push(format!("{name}.a must be positive, got {}", p.a));
            }
            if p.b <= 0.0 {
                errors.push(format!("{name}.b must be positive, got {}", p.b));
            }
            if p.delta <= 0.0 {
                errors.push(format!("{name}.delta must be positive, got {}", p.delta));
            }
            if p.krauss_sigma < 0.0 || p.krauss_sigma > 1.0 {
                errors.push(format!(
                    "{name}.krauss_sigma must be in [0.0, 1.0], got {}",
                    p.krauss_sigma
                ));
            }
            if p.politeness < 0.0 || p.politeness > 1.0 {
                errors.push(format!(
                    "{name}.politeness must be in [0.0, 1.0], got {}",
                    p.politeness
                ));
            }
            if p.gap_acceptance_ttc < 0.0 {
                errors.push(format!(
                    "{name}.gap_acceptance_ttc must be non-negative, got {}",
                    p.gap_acceptance_ttc
                ));
            }
        }

        // Pedestrian validation
        let ped = &self.pedestrian;
        if ped.desired_speed <= 0.0 {
            errors.push(format!(
                "pedestrian.desired_speed must be positive, got {}",
                ped.desired_speed
            ));
        }
        if ped.personal_space <= 0.0 {
            errors.push(format!(
                "pedestrian.personal_space must be positive, got {}",
                ped.personal_space
            ));
        }
        if ped.jaywalking_rate_arterial < 0.0 || ped.jaywalking_rate_arterial > 1.0 {
            errors.push(format!(
                "pedestrian.jaywalking_rate_arterial must be in [0.0, 1.0], got {}",
                ped.jaywalking_rate_arterial
            ));
        }
        if ped.jaywalking_rate_local < 0.0 || ped.jaywalking_rate_local > 1.0 {
            errors.push(format!(
                "pedestrian.jaywalking_rate_local must be in [0.0, 1.0], got {}",
                ped.jaywalking_rate_local
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(VehicleError::ConfigValidation(errors))
        }
    }
}

impl Default for VehicleConfig {
    fn default() -> Self {
        Self {
            motorbike: VehicleTypeParams {
                v0: 11.1,
                s0: 1.0,
                t_headway: 0.8,
                a: 2.0,
                b: 3.0,
                delta: 4.0,
                krauss_accel: 2.0,
                krauss_decel: 3.0,
                krauss_sigma: 0.3,
                krauss_tau: 1.0,
                krauss_max_speed: 11.1,
                krauss_min_gap: 1.0,
                politeness: 0.1,
                threshold: 0.2,
                safe_decel: -4.0,
                right_bias: 0.1,
                gap_acceptance_ttc: 1.0,
                min_filter_gap: Some(0.5),
                max_lateral_speed: Some(1.2),
                half_width: Some(0.25),
                swarm_lateral_speed: Some(0.8),
            },
            car: VehicleTypeParams {
                v0: 9.7,
                s0: 2.0,
                t_headway: 1.5,
                a: 1.0,
                b: 2.0,
                delta: 4.0,
                krauss_accel: 1.0,
                krauss_decel: 4.5,
                krauss_sigma: 0.5,
                krauss_tau: 1.0,
                krauss_max_speed: 9.7,
                krauss_min_gap: 2.0,
                politeness: 0.3,
                threshold: 0.2,
                safe_decel: -4.0,
                right_bias: 0.1,
                gap_acceptance_ttc: 1.5,
                min_filter_gap: None,
                max_lateral_speed: None,
                half_width: None,
                swarm_lateral_speed: None,
            },
            bus: VehicleTypeParams {
                v0: 8.3,
                s0: 3.0,
                t_headway: 1.5,
                a: 1.0,
                b: 2.5,
                delta: 4.0,
                krauss_accel: 1.0,
                krauss_decel: 4.5,
                krauss_sigma: 0.4,
                krauss_tau: 1.0,
                krauss_max_speed: 8.3,
                krauss_min_gap: 3.0,
                politeness: 0.5,
                threshold: 0.2,
                safe_decel: -4.0,
                right_bias: 0.1,
                gap_acceptance_ttc: 1.8,
                min_filter_gap: None,
                max_lateral_speed: None,
                half_width: None,
                swarm_lateral_speed: None,
            },
            truck: VehicleTypeParams {
                v0: 9.7,
                s0: 4.0,
                t_headway: 2.0,
                a: 1.0,
                b: 2.5,
                delta: 4.0,
                krauss_accel: 1.0,
                krauss_decel: 4.5,
                krauss_sigma: 0.4,
                krauss_tau: 1.0,
                krauss_max_speed: 9.7,
                krauss_min_gap: 4.0,
                politeness: 0.4,
                threshold: 0.2,
                safe_decel: -4.0,
                right_bias: 0.1,
                gap_acceptance_ttc: 2.0,
                min_filter_gap: None,
                max_lateral_speed: None,
                half_width: None,
                swarm_lateral_speed: None,
            },
            bicycle: VehicleTypeParams {
                v0: 4.17,
                s0: 1.5,
                t_headway: 1.0,
                a: 1.0,
                b: 3.0,
                delta: 4.0,
                krauss_accel: 1.0,
                krauss_decel: 3.0,
                krauss_sigma: 0.3,
                krauss_tau: 1.0,
                krauss_max_speed: 4.17,
                krauss_min_gap: 1.5,
                politeness: 0.2,
                threshold: 0.2,
                safe_decel: -4.0,
                right_bias: 0.1,
                gap_acceptance_ttc: 1.2,
                min_filter_gap: Some(0.5),
                max_lateral_speed: Some(0.8),
                half_width: Some(0.25),
                swarm_lateral_speed: Some(0.6),
            },
            emergency: VehicleTypeParams {
                v0: 16.7,
                s0: 2.0,
                t_headway: 1.2,
                a: 2.0,
                b: 3.5,
                delta: 4.0,
                krauss_accel: 2.0,
                krauss_decel: 4.5,
                krauss_sigma: 0.2,
                krauss_tau: 1.0,
                krauss_max_speed: 16.7,
                krauss_min_gap: 2.0,
                politeness: 0.0,
                threshold: 0.2,
                safe_decel: -4.0,
                right_bias: 0.1,
                gap_acceptance_ttc: 0.5,
                min_filter_gap: None,
                max_lateral_speed: None,
                half_width: None,
                swarm_lateral_speed: None,
            },
            pedestrian: PedestrianParams {
                desired_speed: 1.2,
                personal_space: 0.5,
                jaywalking_rate_arterial: 0.1,
                jaywalking_rate_local: 0.3,
                gap_acceptance_ttc: 2.0,
            },
        }
    }
}

impl VehicleTypeParams {
    /// Convert to IDM parameters.
    pub fn to_idm_params(&self) -> IdmParams {
        IdmParams {
            v0: self.v0,
            s0: self.s0,
            t_headway: self.t_headway,
            a: self.a,
            b: self.b,
            delta: self.delta,
        }
    }

    /// Convert to Krauss parameters.
    pub fn to_krauss_params(&self) -> KraussParams {
        KraussParams {
            accel: self.krauss_accel,
            decel: self.krauss_decel,
            sigma: self.krauss_sigma,
            tau: self.krauss_tau,
            max_speed: self.krauss_max_speed,
            min_gap: self.krauss_min_gap,
        }
    }

    /// Convert to MOBIL parameters.
    pub fn to_mobil_params(&self) -> MobilParams {
        MobilParams {
            politeness: self.politeness,
            threshold: self.threshold,
            safe_decel: self.safe_decel,
            right_bias: self.right_bias,
        }
    }

    /// Convert to sublane parameters, if this vehicle type has sublane fields.
    ///
    /// Returns `None` for lane-based vehicles (car, bus, truck, emergency).
    pub fn to_sublane_params(&self) -> Option<SublaneParams> {
        match (
            self.min_filter_gap,
            self.max_lateral_speed,
            self.half_width,
            self.swarm_lateral_speed,
        ) {
            (Some(mfg), Some(mls), Some(hw), Some(sls)) => Some(SublaneParams {
                min_filter_gap: mfg,
                max_lateral_speed: mls,
                half_width: hw,
                swarm_lateral_speed: sls,
            }),
            _ => None,
        }
    }
}
