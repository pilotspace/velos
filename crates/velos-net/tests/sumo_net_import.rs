//! Integration tests for SUMO .net.xml import.

use std::path::Path;
use velos_net::sumo_import::import_sumo_net;

/// Path to the test fixture relative to workspace root.
fn fixture_path() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/simple.net.xml"
    ))
}

#[test]
fn imports_correct_node_count() {
    let (graph, _signals, _warnings) = import_sumo_net(fixture_path()).unwrap();
    // 3 junctions: j0, j1, j2
    assert_eq!(graph.node_count(), 3);
}

#[test]
fn imports_correct_edge_count() {
    let (graph, _signals, _warnings) = import_sumo_net(fixture_path()).unwrap();
    // 4 external edges: e0, e1, e2, e3. Internal edges (:j1_0, :j1_1) filtered.
    assert_eq!(graph.edge_count(), 4);
}

#[test]
fn filters_internal_edges() {
    let (_graph, _signals, warnings) = import_sumo_net(fixture_path()).unwrap();
    // Internal edges should be silently skipped (not a warning -- expected behavior).
    // Verify no internal edge IDs appear in warnings as "imported".
    for w in &warnings {
        assert!(
            !w.contains("imported internal edge"),
            "Internal edge should not be imported"
        );
    }
}

#[test]
fn parses_lane_count_from_child_elements() {
    let (graph, _signals, _warnings) = import_sumo_net(fixture_path()).unwrap();
    let inner = graph.inner();

    // Find edges by checking lane counts.
    // e0 has 2 lanes, e1/e2/e3 have 1 lane each.
    let mut lane_counts: Vec<u8> = inner
        .edge_weights()
        .map(|e| e.lane_count)
        .collect();
    lane_counts.sort();
    assert_eq!(lane_counts, vec![1, 1, 1, 2]);
}

#[test]
fn parses_speed_limit_as_max_lane_speed() {
    let (graph, _signals, _warnings) = import_sumo_net(fixture_path()).unwrap();
    let inner = graph.inner();

    // e0 has speed 13.89 on both lanes -> speed_limit = 13.89
    let has_primary_speed = inner
        .edge_weights()
        .any(|e| (e.speed_limit_mps - 13.89).abs() < 0.01);
    assert!(has_primary_speed, "Should have an edge with speed 13.89 m/s");
}

#[test]
fn parses_junction_positions() {
    let (graph, _signals, _warnings) = import_sumo_net(fixture_path()).unwrap();
    let inner = graph.inner();

    let mut positions: Vec<[f64; 2]> = inner
        .node_weights()
        .map(|n| n.pos)
        .collect();
    positions.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());

    // j0: (0, 50), j1: (100, 50), j2: (200, 100)
    assert!((positions[0][0] - 0.0).abs() < 0.01);
    assert!((positions[0][1] - 50.0).abs() < 0.01);
    assert!((positions[1][0] - 100.0).abs() < 0.01);
    assert!((positions[1][1] - 50.0).abs() < 0.01);
    assert!((positions[2][0] - 200.0).abs() < 0.01);
    assert!((positions[2][1] - 100.0).abs() < 0.01);
}

#[test]
fn parses_signal_plans() {
    let (_graph, signals, _warnings) = import_sumo_net(fixture_path()).unwrap();
    assert_eq!(signals.len(), 1, "Should have 1 tlLogic");
    assert_eq!(signals[0].junction_id, "tl_j1");
    // SUMO has 4 phases (green, yellow, green, yellow) but yellow phases
    // are merged as amber_duration into the preceding green phase.
    assert_eq!(signals[0].plan.phases.len(), 2);
    // Phase 0: 30s green + 5s amber
    assert!((signals[0].plan.phases[0].green_duration - 30.0).abs() < 0.01);
    assert!((signals[0].plan.phases[0].amber_duration - 5.0).abs() < 0.01);
    // Phase 1: 25s green + 5s amber
    assert!((signals[0].plan.phases[1].green_duration - 25.0).abs() < 0.01);
    assert!((signals[0].plan.phases[1].amber_duration - 5.0).abs() < 0.01);
    // Cycle time = 30+5+25+5 = 65
    assert!((signals[0].plan.cycle_time - 65.0).abs() < 0.01);
}

#[test]
fn detects_roundabout_edges() {
    let (_graph, _signals, warnings) = import_sumo_net(fixture_path()).unwrap();
    // The roundabout element should produce informational output.
    let has_roundabout_info = warnings
        .iter()
        .any(|w| w.contains("roundabout"));
    assert!(has_roundabout_info, "Should log roundabout detection");
}

#[test]
fn warns_on_unmapped_attributes() {
    let (_graph, _signals, warnings) = import_sumo_net(fixture_path()).unwrap();
    // spreadType on edge e0, customParam on junction j1 should produce warnings.
    let has_unmapped = warnings
        .iter()
        .any(|w| w.contains("unmapped") || w.contains("Unmapped"));
    assert!(has_unmapped, "Should warn about unmapped attributes. Got: {:?}", warnings);
}

#[test]
fn returns_error_for_missing_file() {
    let result = import_sumo_net(Path::new("/nonexistent/path.net.xml"));
    assert!(result.is_err());
}

#[test]
fn maps_road_class_from_edge_type() {
    let (graph, _signals, _warnings) = import_sumo_net(fixture_path()).unwrap();
    let inner = graph.inner();

    use velos_net::RoadClass;
    let classes: Vec<RoadClass> = inner.edge_weights().map(|e| e.road_class).collect();
    assert!(classes.contains(&RoadClass::Primary), "Should have Primary road class");
    assert!(classes.contains(&RoadClass::Secondary), "Should have Secondary road class");
    assert!(classes.contains(&RoadClass::Tertiary), "Should have Tertiary road class");
    assert!(classes.contains(&RoadClass::Residential), "Should have Residential road class");
}
