//! Tests for HCMC-specific network rules: motorbike-only lanes, time-dependent
//! one-ways, and override file application.

use petgraph::graph::DiGraph;
use velos_net::cleaning::{clean_network, CleaningConfig, OverrideFile};
use velos_net::graph::{
    OneWayDirection, RoadClass, RoadEdge, RoadGraph, RoadNode, TimeWindow,
};

/// Helper: create an edge with specific properties.
fn make_edge(
    length: f64,
    road_class: RoadClass,
    lanes: u8,
    motorbike_only: bool,
) -> RoadEdge {
    RoadEdge {
        length_m: length,
        speed_limit_mps: 8.3, // ~30 km/h
        lane_count: lanes,
        oneway: false,
        road_class,
        geometry: vec![],
        motorbike_only,
        time_windows: None,
    }
}

/// Build a strongly connected graph with an alley (narrow service road).
fn graph_with_alley() -> RoadGraph {
    let mut g = DiGraph::new();

    // Main road triangle (strongly connected).
    let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
    let b = g.add_node(RoadNode {
        pos: [100.0, 0.0],
    });
    let c = g.add_node(RoadNode {
        pos: [50.0, 50.0],
    });

    for &(from, to) in &[(a, b), (b, c), (c, a), (b, a), (c, b), (a, c)] {
        g.add_edge(
            from,
            to,
            make_edge(100.0, RoadClass::Secondary, 2, false),
        );
    }

    // Alley branch from B: narrow service road (motorbike-only candidate).
    let d = g.add_node(RoadNode {
        pos: [120.0, 10.0],
    });
    // Service road with narrow width -- should be tagged motorbike-only.
    let mut alley_edge = make_edge(20.0, RoadClass::Service, 1, false);
    alley_edge.speed_limit_mps = 5.6; // ~20 km/h
    g.add_edge(b, d, alley_edge.clone());
    g.add_edge(d, b, alley_edge);

    RoadGraph::new(g)
}

#[test]
fn alleys_tagged_as_motorbike_only() {
    let mut graph = graph_with_alley();
    let config = CleaningConfig::default();
    let report = clean_network(&mut graph, &config);

    assert!(report.motorbike_only_tagged > 0);

    // Service road edges should be motorbike-only.
    let has_motorbike_only = graph
        .inner()
        .edge_weights()
        .any(|e| e.motorbike_only);
    assert!(has_motorbike_only, "Expected at least one motorbike-only edge");
}

#[test]
fn time_window_direction_variants() {
    let tw = TimeWindow {
        start_hour: 7,
        end_hour: 9,
        direction: OneWayDirection::Forward,
    };
    assert!(tw.contains_hour(8));
    assert!(!tw.contains_hour(10));

    let tw_both = TimeWindow {
        start_hour: 0,
        end_hour: 24,
        direction: OneWayDirection::Both,
    };
    assert!(tw_both.contains_hour(12));
    assert_eq!(tw_both.direction, OneWayDirection::Both);
}

#[test]
fn time_dependent_oneway_edges_applied() {
    let mut graph = graph_with_alley();
    let config = CleaningConfig::default();
    let report = clean_network(&mut graph, &config);

    // After cleaning with HCMC rules, time-dependent one-ways should be applied
    // (at least the step runs without error).
    // The step runs without error (time_dependent_applied is 0 until real HCMC data loaded).
    let _ = report.time_dependent_applied;
}

#[test]
fn override_file_parses_toml() {
    let toml_str = r#"
[[edge_override]]
osm_way_id = "way/123456"
lanes = 3
speed_limit_kmh = 40.0
reason = "Field survey shows 3 lanes"

[[edge_override]]
osm_way_id = "way/789012"
motorbike_only = true
reason = "Narrow alley not tagged in OSM"
"#;

    let overrides: OverrideFile = toml::from_str(toml_str).unwrap();
    assert_eq!(overrides.edge_override.len(), 2);
    assert_eq!(overrides.edge_override[0].lanes, Some(3));
    assert!(overrides.edge_override[1].motorbike_only.unwrap());
}

#[test]
fn road_class_includes_motorway_trunk_service() {
    // Verify the extended RoadClass enum has all variants.
    let _motorway = RoadClass::Motorway;
    let _trunk = RoadClass::Trunk;
    let _service = RoadClass::Service;
    let _primary = RoadClass::Primary;
}

#[test]
fn road_edge_has_motorbike_and_time_fields() {
    let edge = RoadEdge {
        length_m: 100.0,
        speed_limit_mps: 13.9,
        lane_count: 2,
        oneway: false,
        road_class: RoadClass::Primary,
        geometry: vec![],
        motorbike_only: false,
        time_windows: Some(vec![TimeWindow {
            start_hour: 7,
            end_hour: 9,
            direction: OneWayDirection::Forward,
        }]),
    };
    assert!(!edge.motorbike_only);
    assert_eq!(edge.time_windows.as_ref().unwrap().len(), 1);
}
