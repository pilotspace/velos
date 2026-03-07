use velos_meso::queue_model::{MesoVehicle, SpatialQueue};
use velos_meso::zone_config::{ZoneConfig, ZoneType};

// ── BPR travel time tests ──────────────────────────────────────────

#[test]
fn bpr_free_flow_at_zero_vc() {
    let q = SpatialQueue::new(60.0, 20.0);
    // V/C = 0 -> travel_time = t_free * (1 + 0.15 * 0^4) = t_free
    let tt = q.travel_time();
    assert!(
        (tt - 60.0).abs() < 1e-9,
        "free flow should be t_free, got {tt}"
    );
}

#[test]
fn bpr_at_capacity() {
    let mut q = SpatialQueue::new(60.0, 20.0);
    // Fill to capacity (V/C = 1.0)
    for i in 0..20 {
        q.enter(MesoVehicle::new(i, 0.0, 0));
    }
    // t = t_free * (1 + 0.15 * 1^4) = 60 * 1.15 = 69.0
    let tt = q.travel_time();
    assert!(
        (tt - 69.0).abs() < 1e-9,
        "at-capacity should be 69.0, got {tt}"
    );
}

#[test]
fn bpr_over_capacity() {
    let mut q = SpatialQueue::new(60.0, 20.0);
    // Fill to 2x capacity (V/C = 2.0)
    for i in 0..40 {
        q.enter(MesoVehicle::new(i, 0.0, 0));
    }
    // t = 60 * (1 + 0.15 * 2^4) = 60 * (1 + 0.15 * 16) = 60 * 3.4 = 204.0
    let tt = q.travel_time();
    assert!(
        (tt - 204.0).abs() < 1e-9,
        "2x capacity should be 204.0, got {tt}"
    );
}

// ── FIFO exit ordering tests ───────────────────────────────────────

#[test]
fn try_exit_returns_none_on_empty_queue() {
    let mut q = SpatialQueue::new(10.0, 10.0);
    assert!(q.try_exit(100.0).is_none());
}

#[test]
fn try_exit_returns_none_when_not_enough_time() {
    let mut q = SpatialQueue::new(60.0, 10.0);
    q.enter(MesoVehicle::new(1, 0.0, 99));
    // At sim_time=30.0, vehicle entered at 0.0, travel_time=60.0 -> not ready
    assert!(q.try_exit(30.0).is_none());
}

#[test]
fn try_exit_returns_vehicle_when_time_elapsed() {
    // Use large capacity so single vehicle has negligible V/C impact on travel time
    let mut q = SpatialQueue::new(60.0, 10000.0);
    q.enter(MesoVehicle::new(42, 0.0, 99));
    // V/C = 1/10000 -> travel_time ~ 60.0 (essentially free flow)
    // At sim_time=61.0, well past travel_time -> ready
    let v = q.try_exit(61.0);
    assert!(v.is_some());
    let v = v.unwrap();
    assert_eq!(v.vehicle_id, 42);
    assert_eq!(v.exit_edge, 99);
}

#[test]
fn fifo_ordering_maintained() {
    // Large capacity so 3 vehicles have negligible V/C impact
    let mut q = SpatialQueue::new(10.0, 10000.0);
    q.enter(MesoVehicle::new(1, 0.0, 10));
    q.enter(MesoVehicle::new(2, 1.0, 20));
    q.enter(MesoVehicle::new(3, 2.0, 30));

    // At time 11.0, vehicle 1 is ready (entered at 0.0, travel_time ~10.0)
    let v1 = q.try_exit(11.0).unwrap();
    assert_eq!(v1.vehicle_id, 1);

    // At time 12.0, vehicle 2 is ready (entered at 1.0)
    let v2 = q.try_exit(12.0).unwrap();
    assert_eq!(v2.vehicle_id, 2);

    // At time 13.0, vehicle 3 is ready (entered at 2.0)
    let v3 = q.try_exit(13.0).unwrap();
    assert_eq!(v3.vehicle_id, 3);
}

// ── Vehicle count tracking ─────────────────────────────────────────

#[test]
fn vehicle_count_tracks_enter_and_exit() {
    let mut q = SpatialQueue::new(10.0, 10.0);
    assert_eq!(q.vehicle_count(), 0);

    q.enter(MesoVehicle::new(1, 0.0, 0));
    assert_eq!(q.vehicle_count(), 1);

    q.enter(MesoVehicle::new(2, 0.0, 0));
    assert_eq!(q.vehicle_count(), 2);

    q.try_exit(100.0); // exits one
    assert_eq!(q.vehicle_count(), 1);
}

// ── ZoneConfig tests ───────────────────────────────────────────────

#[test]
fn zone_config_defaults_to_micro() {
    let config = ZoneConfig::new();
    assert_eq!(config.zone_type(999), ZoneType::Micro);
}

#[test]
fn zone_config_set_and_get() {
    let mut config = ZoneConfig::new();
    config.set_zone(10, ZoneType::Meso);
    config.set_zone(20, ZoneType::Buffer);

    assert_eq!(config.zone_type(10), ZoneType::Meso);
    assert_eq!(config.zone_type(20), ZoneType::Buffer);
    assert_eq!(config.zone_type(30), ZoneType::Micro);
}

#[test]
fn zone_config_load_from_toml_string() {
    let toml_str = r#"
[[zones]]
edge_id = 1
zone = "meso"

[[zones]]
edge_id = 2
zone = "buffer"

[[zones]]
edge_id = 3
zone = "micro"
"#;
    let config = ZoneConfig::load_from_toml_str(toml_str).unwrap();
    assert_eq!(config.zone_type(1), ZoneType::Meso);
    assert_eq!(config.zone_type(2), ZoneType::Buffer);
    assert_eq!(config.zone_type(3), ZoneType::Micro);
    // Unconfigured edge defaults to Micro
    assert_eq!(config.zone_type(4), ZoneType::Micro);
}

// ── MesoVehicle construction ───────────────────────────────────────

#[test]
fn meso_vehicle_fields() {
    let v = MesoVehicle::new(42, 10.5, 99);
    assert_eq!(v.vehicle_id, 42);
    assert!((v.entry_time - 10.5).abs() < 1e-9);
    assert_eq!(v.exit_edge, 99);
}
