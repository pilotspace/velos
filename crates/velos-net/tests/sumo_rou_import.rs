//! Integration tests for SUMO .rou.xml demand import.

use std::path::Path;
use velos_net::sumo_demand::{import_sumo_routes, CarFollowModelType};

/// Path to the test fixture relative to workspace root.
fn fixture_path() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/simple.rou.xml"
    ))
}

#[test]
fn parses_vehicle_count() {
    let (vehicles, _persons, _warnings) = import_sumo_routes(fixture_path()).unwrap();
    // 3 explicit vehicles + 2 trips + 100 flow vehicles = 105
    assert_eq!(vehicles.len(), 105);
}

#[test]
fn parses_explicit_vehicle_routes() {
    let (vehicles, _persons, _warnings) = import_sumo_routes(fixture_path()).unwrap();
    let veh0 = vehicles.iter().find(|v| v.id == "veh_0").unwrap();
    assert_eq!(veh0.route, vec!["e0", "e1"]);
    assert!((veh0.depart - 0.0).abs() < 0.01);
    assert_eq!(veh0.vtype, "car_krauss");
}

#[test]
fn parses_vehicle_with_idm_route() {
    let (vehicles, _persons, _warnings) = import_sumo_routes(fixture_path()).unwrap();
    let veh1 = vehicles.iter().find(|v| v.id == "veh_1").unwrap();
    assert_eq!(veh1.route, vec!["e0", "e1", "e3"]);
    assert!((veh1.depart - 10.0).abs() < 0.01);
}

#[test]
fn parses_trip_elements() {
    let (vehicles, _persons, _warnings) = import_sumo_routes(fixture_path()).unwrap();
    let trip0 = vehicles.iter().find(|v| v.id == "trip_0").unwrap();
    // Trips store [from, to] as route (routing is external).
    assert_eq!(trip0.route, vec!["e0", "e3"]);
    assert!((trip0.depart - 5.0).abs() < 0.01);
}

#[test]
fn expands_flow_to_individual_vehicles() {
    let (vehicles, _persons, _warnings) = import_sumo_routes(fixture_path()).unwrap();
    let flow_vehicles: Vec<_> = vehicles
        .iter()
        .filter(|v| v.id.starts_with("flow_0_"))
        .collect();
    assert_eq!(flow_vehicles.len(), 100);

    // First vehicle departs at begin=0.
    let first = flow_vehicles.iter().min_by(|a, b| a.depart.partial_cmp(&b.depart).unwrap()).unwrap();
    assert!((first.depart - 0.0).abs() < 0.01);

    // Last vehicle departs near end=3600 (but not at 3600 exactly).
    let last = flow_vehicles.iter().max_by(|a, b| a.depart.partial_cmp(&b.depart).unwrap()).unwrap();
    assert!(last.depart < 3600.0);
    assert!(last.depart > 3500.0);
}

#[test]
fn maps_krauss_car_follow_model() {
    let (vehicles, _persons, _warnings) = import_sumo_routes(fixture_path()).unwrap();
    let veh0 = vehicles.iter().find(|v| v.id == "veh_0").unwrap();
    assert!(matches!(veh0.params.car_follow_model, CarFollowModelType::Krauss));
}

#[test]
fn maps_idm_car_follow_model() {
    let (vehicles, _persons, _warnings) = import_sumo_routes(fixture_path()).unwrap();
    let veh1 = vehicles.iter().find(|v| v.id == "veh_1").unwrap();
    assert!(matches!(veh1.params.car_follow_model, CarFollowModelType::IDM));
}

#[test]
fn parses_vtype_parameters() {
    let (vehicles, _persons, _warnings) = import_sumo_routes(fixture_path()).unwrap();
    let veh0 = vehicles.iter().find(|v| v.id == "veh_0").unwrap();
    assert!((veh0.params.accel - 2.6).abs() < 0.01);
    assert!((veh0.params.decel - 4.5).abs() < 0.01);
    assert!((veh0.params.max_speed - 33.33).abs() < 0.01);
    assert_eq!(veh0.params.sigma, Some(0.5));
    assert!((veh0.params.min_gap - 2.5).abs() < 0.01);
    assert!((veh0.params.length - 5.0).abs() < 0.01);
}

#[test]
fn parses_person_with_walk() {
    let (_vehicles, persons, _warnings) = import_sumo_routes(fixture_path()).unwrap();
    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].id, "ped_0");
    assert!((persons[0].depart - 30.0).abs() < 0.01);
    assert_eq!(persons[0].stages.len(), 1);
}

#[test]
fn warns_on_unmapped_attributes() {
    let (_vehicles, _persons, warnings) = import_sumo_routes(fixture_path()).unwrap();
    // guiShape, color, departLane, tau, via, output, freq, pos are unmapped.
    let unmapped_count = warnings
        .iter()
        .filter(|w| w.contains("Unmapped") || w.contains("unmapped"))
        .count();
    assert!(unmapped_count >= 4, "Expected at least 4 unmapped warnings, got {}: {:?}", unmapped_count, warnings);
}

#[test]
fn warns_on_calibrator() {
    let (_vehicles, _persons, warnings) = import_sumo_routes(fixture_path()).unwrap();
    let has_calibrator_warning = warnings.iter().any(|w| w.contains("calibrator"));
    assert!(has_calibrator_warning, "Should warn about calibrator. Got: {:?}", warnings);
}

#[test]
fn returns_error_for_missing_file() {
    let result = import_sumo_routes(Path::new("/nonexistent/path.rou.xml"));
    assert!(result.is_err());
}
