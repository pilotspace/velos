//! Tests for VehicleConfig TOML loading, validation, and parameter conversion.

use velos_vehicle::config::{load_vehicle_config_from_str, VehicleConfig};
use velos_vehicle::types::VehicleType;

/// Load the real TOML config file contents for testing.
fn real_toml() -> &'static str {
    include_str!("../../../data/hcmc/vehicle_params.toml")
}

// ---------------------------------------------------------------------------
// TOML parsing
// ---------------------------------------------------------------------------

#[test]
fn toml_parses_without_error() {
    let config = load_vehicle_config_from_str(real_toml()).expect("TOML should parse");
    // Smoke test: motorbike section exists
    assert!(config.motorbike.v0 > 0.0);
}

#[test]
fn each_vehicle_type_has_all_required_fields() {
    let config = load_vehicle_config_from_str(real_toml()).unwrap();

    // Check all six vehicle type sections have the core IDM fields
    for (name, vt) in [
        ("motorbike", VehicleType::Motorbike),
        ("car", VehicleType::Car),
        ("bus", VehicleType::Bus),
        ("truck", VehicleType::Truck),
        ("bicycle", VehicleType::Bicycle),
        ("emergency", VehicleType::Emergency),
    ] {
        let p = config.for_vehicle_type(vt);
        assert!(p.v0 > 0.0, "{name}.v0 missing or zero");
        assert!(p.s0 > 0.0, "{name}.s0 missing or zero");
        assert!(p.t_headway > 0.0, "{name}.t_headway missing or zero");
        assert!(p.a > 0.0, "{name}.a missing or zero");
        assert!(p.b > 0.0, "{name}.b missing or zero");
        assert!(p.delta > 0.0, "{name}.delta missing or zero");
        assert!(p.krauss_sigma >= 0.0, "{name}.krauss_sigma missing");
        assert!(p.politeness >= 0.0, "{name}.politeness missing");
        assert!(p.gap_acceptance_ttc >= 0.0, "{name}.gap_acceptance_ttc missing");
    }

    // Pedestrian section
    assert!(config.pedestrian.desired_speed > 0.0);
    assert!(config.pedestrian.personal_space > 0.0);
}

// ---------------------------------------------------------------------------
// HCMC-calibrated value ranges
// ---------------------------------------------------------------------------

#[test]
fn motorbike_v0_in_hcmc_range() {
    let config = load_vehicle_config_from_str(real_toml()).unwrap();
    let v0 = config.motorbike.v0;
    // 35-45 km/h = 9.7-12.5 m/s
    assert!(
        (9.7..=12.5).contains(&v0),
        "motorbike v0={v0} not in 9.7-12.5 m/s range"
    );
}

#[test]
fn car_v0_in_hcmc_range() {
    let config = load_vehicle_config_from_str(real_toml()).unwrap();
    let v0 = config.car.v0;
    // 30-40 km/h = 8.3-11.1 m/s
    assert!(
        (8.3..=11.1).contains(&v0),
        "car v0={v0} not in 8.3-11.1 m/s range"
    );
}

#[test]
fn truck_v0_in_hcmc_range_not_90kmh() {
    let config = load_vehicle_config_from_str(real_toml()).unwrap();
    let v0 = config.truck.v0;
    // 30-40 km/h = 8.3-11.1 m/s, NOT 25.0 m/s (90 km/h)
    assert!(
        (8.3..=11.1).contains(&v0),
        "truck v0={v0} not in 8.3-11.1 m/s range (must NOT be 25.0)"
    );
    assert!(
        (v0 - 25.0).abs() > 1.0,
        "truck v0={v0} is still the old 90 km/h value"
    );
}

#[test]
fn bus_v0_in_hcmc_range() {
    let config = load_vehicle_config_from_str(real_toml()).unwrap();
    let v0 = config.bus.v0;
    // 25-35 km/h = 6.9-9.7 m/s
    assert!(
        (6.9..=9.7).contains(&v0),
        "bus v0={v0} not in 6.9-9.7 m/s range"
    );
}

#[test]
fn bicycle_v0_in_hcmc_range() {
    let config = load_vehicle_config_from_str(real_toml()).unwrap();
    let v0 = config.bicycle.v0;
    // 12-18 km/h = 3.3-5.0 m/s
    assert!(
        (3.3..=5.0).contains(&v0),
        "bicycle v0={v0} not in 3.3-5.0 m/s range"
    );
}

#[test]
fn pedestrian_desired_speed_in_range() {
    let config = load_vehicle_config_from_str(real_toml()).unwrap();
    let speed = config.pedestrian.desired_speed;
    assert!(
        (1.0..=1.4).contains(&speed),
        "pedestrian desired_speed={speed} not in 1.0-1.4 m/s range"
    );
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[test]
fn validate_rejects_v0_zero() {
    let toml = real_toml().replace("v0 = 11.1", "v0 = 0.0");
    let err = load_vehicle_config_from_str(&toml).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("v0") && msg.contains("positive"),
        "error should mention v0: {msg}"
    );
}

#[test]
fn validate_rejects_negative_v0() {
    let toml = real_toml().replace("v0 = 11.1", "v0 = -5.0");
    let err = load_vehicle_config_from_str(&toml).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("v0"),
        "error should mention v0: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Conversion methods
// ---------------------------------------------------------------------------

#[test]
fn vehicle_type_params_to_idm_params() {
    let config = VehicleConfig::default();
    let idm = config.car.to_idm_params();
    assert!((idm.v0 - 9.7).abs() < 0.01);
    assert!((idm.s0 - 2.0).abs() < 0.01);
    assert!((idm.t_headway - 1.5).abs() < 0.01);
    assert!((idm.a - 1.0).abs() < 0.01);
    assert!((idm.b - 2.0).abs() < 0.01);
    assert!((idm.delta - 4.0).abs() < 0.01);
}

#[test]
fn vehicle_type_params_to_krauss_params() {
    let config = VehicleConfig::default();
    let k = config.car.to_krauss_params();
    assert!((k.accel - 1.0).abs() < 0.01);
    assert!((k.decel - 4.5).abs() < 0.01);
    assert!((k.sigma - 0.5).abs() < 0.01);
    assert!((k.tau - 1.0).abs() < 0.01);
    assert!((k.max_speed - 9.7).abs() < 0.01);
    assert!((k.min_gap - 2.0).abs() < 0.01);
}

#[test]
fn vehicle_type_params_to_mobil_params() {
    let config = VehicleConfig::default();
    let m = config.car.to_mobil_params();
    assert!((m.politeness - 0.3).abs() < 0.01);
    assert!((m.threshold - 0.2).abs() < 0.01);
    assert!((m.safe_decel - (-4.0)).abs() < 0.01);
    assert!((m.right_bias - 0.1).abs() < 0.01);
}

#[test]
fn vehicle_type_params_to_sublane_params_some_for_motorbike() {
    let config = VehicleConfig::default();
    let sl = config.motorbike.to_sublane_params();
    assert!(sl.is_some(), "motorbike should have sublane params");
    let sl = sl.unwrap();
    assert!((sl.min_filter_gap - 0.5).abs() < 0.01);
    assert!((sl.max_lateral_speed - 1.2).abs() < 0.01);
    assert!((sl.half_width - 0.25).abs() < 0.01);
    assert!((sl.swarm_lateral_speed - 0.8).abs() < 0.01);
}

#[test]
fn vehicle_type_params_to_sublane_params_none_for_car() {
    let config = VehicleConfig::default();
    let sl = config.car.to_sublane_params();
    assert!(sl.is_none(), "car should not have sublane params");
}

// ---------------------------------------------------------------------------
// Default fallback matches TOML
// ---------------------------------------------------------------------------

#[test]
fn default_matches_toml_file() {
    let from_toml = load_vehicle_config_from_str(real_toml()).unwrap();
    let from_default = VehicleConfig::default();

    assert!(
        (from_toml.motorbike.v0 - from_default.motorbike.v0).abs() < 0.01,
        "TOML motorbike.v0={} != default {}",
        from_toml.motorbike.v0,
        from_default.motorbike.v0
    );
    assert!(
        (from_toml.truck.v0 - from_default.truck.v0).abs() < 0.01,
        "TOML truck.v0={} != default {}",
        from_toml.truck.v0,
        from_default.truck.v0
    );
    assert!(
        (from_toml.pedestrian.desired_speed - from_default.pedestrian.desired_speed).abs() < 0.01,
        "TOML ped speed != default"
    );
}
