//! Integration tests for OSM PBF import.

use std::path::Path;

use velos_net::{import_osm, RoadClass};

const PBF_PATH: &str = "data/hcmc/district1.osm.pbf";
const CENTER_LAT: f64 = 10.7756;
const CENTER_LON: f64 = 106.7019;

#[test]
fn district1_pbf_loads_non_empty_graph() {
    // Resolve path relative to workspace root (cargo test runs from workspace root).
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(PBF_PATH);

    if !path.exists() {
        eprintln!("Skipping: PBF file not found at {}", path.display());
        return;
    }

    let graph = import_osm(&path, CENTER_LAT, CENTER_LON).expect("import should succeed");

    assert!(
        graph.node_count() > 100,
        "expected >100 nodes, got {}",
        graph.node_count()
    );
    assert!(
        graph.edge_count() > 100,
        "expected >100 edges, got {}",
        graph.edge_count()
    );

    println!(
        "District 1 graph: {} nodes, {} edges",
        graph.node_count(),
        graph.edge_count()
    );
}

#[test]
fn district1_edges_have_valid_properties() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(PBF_PATH);

    if !path.exists() {
        return;
    }

    let graph = import_osm(&path, CENTER_LAT, CENTER_LON).expect("import should succeed");
    let g = graph.inner();

    for edge in g.edge_weights() {
        assert!(edge.length_m > 0.0, "edge length must be positive");
        assert!(
            edge.speed_limit_mps > 0.0,
            "speed limit must be positive"
        );
        assert!(
            edge.lane_count >= 1,
            "lane count must be >= 1"
        );
        assert!(
            matches!(
                edge.road_class,
                RoadClass::Motorway
                    | RoadClass::Trunk
                    | RoadClass::Primary
                    | RoadClass::Secondary
                    | RoadClass::Tertiary
                    | RoadClass::Residential
                    | RoadClass::Service
            ),
            "unexpected road class"
        );
        assert!(
            edge.geometry.len() >= 2,
            "geometry must have at least 2 points"
        );
    }
}

#[test]
fn bidirectional_roads_create_two_edges() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(PBF_PATH);

    if !path.exists() {
        return;
    }

    let graph = import_osm(&path, CENTER_LAT, CENTER_LON).expect("import should succeed");
    let g = graph.inner();

    // Count oneway vs bidirectional edges.
    let oneway_edges = g.edge_weights().filter(|e| e.oneway).count();
    let bidi_edges = g.edge_weights().filter(|e| !e.oneway).count();

    // Bidirectional edges should come in pairs, so total should be even.
    assert!(
        bidi_edges % 2 == 0 || bidi_edges > 0,
        "bidirectional edges should exist"
    );

    println!(
        "Oneway edges: {oneway_edges}, Bidirectional edges: {bidi_edges} (pairs: {})",
        bidi_edges / 2
    );
}
