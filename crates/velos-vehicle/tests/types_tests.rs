//! Tests for VehicleType enum and default IDM parameters.

use velos_vehicle::types::{default_idm_params, VehicleType};

#[test]
fn vehicle_type_has_7_variants() {
    let variants = [
        VehicleType::Motorbike,
        VehicleType::Car,
        VehicleType::Bus,
        VehicleType::Bicycle,
        VehicleType::Truck,
        VehicleType::Emergency,
        VehicleType::Pedestrian,
    ];
    assert_eq!(variants.len(), 7);
}

#[test]
fn idm_params_motorbike() {
    let p = default_idm_params(VehicleType::Motorbike);
    assert!((p.v0 - 11.1).abs() < 0.01);
    assert!((p.s0 - 1.0).abs() < 0.01);
    assert!((p.t_headway - 1.0).abs() < 0.01);
    assert!((p.a - 2.0).abs() < 0.01);
    assert!((p.b - 3.0).abs() < 0.01);
    assert!((p.delta - 4.0).abs() < 0.01);
}

#[test]
fn idm_params_car() {
    let p = default_idm_params(VehicleType::Car);
    assert!((p.v0 - 13.9).abs() < 0.01);
    assert!((p.s0 - 2.0).abs() < 0.01);
    assert!((p.t_headway - 1.5).abs() < 0.01);
    assert!((p.a - 1.0).abs() < 0.01);
    assert!((p.b - 2.0).abs() < 0.01);
}

#[test]
fn idm_params_bus() {
    let p = default_idm_params(VehicleType::Bus);
    assert!((p.v0 - 11.1).abs() < 0.01, "Bus v0 should be 11.1 (40km/h), got {}", p.v0);
    assert!((p.s0 - 3.0).abs() < 0.01, "Bus s0 should be 3.0, got {}", p.s0);
    assert!((p.t_headway - 1.5).abs() < 0.01, "Bus t_headway should be 1.5, got {}", p.t_headway);
    assert!((p.a - 1.0).abs() < 0.01, "Bus a should be 1.0, got {}", p.a);
    assert!((p.b - 2.5).abs() < 0.01, "Bus b should be 2.5, got {}", p.b);
    assert!((p.delta - 4.0).abs() < 0.01);
}

#[test]
fn idm_params_bicycle() {
    let p = default_idm_params(VehicleType::Bicycle);
    assert!((p.v0 - 4.17).abs() < 0.01, "Bicycle v0 should be 4.17 (15km/h), got {}", p.v0);
    assert!((p.s0 - 1.5).abs() < 0.01, "Bicycle s0 should be 1.5, got {}", p.s0);
    assert!((p.t_headway - 1.0).abs() < 0.01, "Bicycle t_headway should be 1.0, got {}", p.t_headway);
    assert!((p.a - 1.0).abs() < 0.01, "Bicycle a should be 1.0, got {}", p.a);
    assert!((p.b - 3.0).abs() < 0.01, "Bicycle b should be 3.0, got {}", p.b);
    assert!((p.delta - 4.0).abs() < 0.01);
}

#[test]
fn idm_params_truck() {
    let p = default_idm_params(VehicleType::Truck);
    assert!((p.v0 - 25.0).abs() < 0.01, "Truck v0 should be 25.0 (90km/h), got {}", p.v0);
    assert!((p.s0 - 4.0).abs() < 0.01, "Truck s0 should be 4.0, got {}", p.s0);
    assert!((p.t_headway - 2.0).abs() < 0.01, "Truck t_headway should be 2.0, got {}", p.t_headway);
    assert!((p.a - 1.0).abs() < 0.01, "Truck a should be 1.0, got {}", p.a);
    assert!((p.b - 2.5).abs() < 0.01, "Truck b should be 2.5, got {}", p.b);
    assert!((p.delta - 4.0).abs() < 0.01);
}

#[test]
fn idm_params_emergency() {
    let p = default_idm_params(VehicleType::Emergency);
    assert!((p.v0 - 16.7).abs() < 0.01, "Emergency v0 should be 16.7 (60km/h), got {}", p.v0);
    assert!((p.s0 - 2.0).abs() < 0.01, "Emergency s0 should be 2.0, got {}", p.s0);
    assert!((p.t_headway - 1.2).abs() < 0.01, "Emergency t_headway should be 1.2, got {}", p.t_headway);
    assert!((p.a - 2.0).abs() < 0.01, "Emergency a should be 2.0, got {}", p.a);
    assert!((p.b - 3.5).abs() < 0.01, "Emergency b should be 3.5, got {}", p.b);
    assert!((p.delta - 4.0).abs() < 0.01);
}

#[test]
fn idm_params_pedestrian() {
    let p = default_idm_params(VehicleType::Pedestrian);
    assert!((p.v0 - 1.4).abs() < 0.01);
    assert!((p.s0 - 0.5).abs() < 0.01);
}
