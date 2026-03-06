//! Tests for the R-tree spatial index.

use velos_net::SpatialIndex;

#[test]
fn empty_index_returns_no_neighbors() {
    let index = SpatialIndex::empty();
    let results = index.nearest_within_radius([0.0, 0.0], 100.0);
    assert!(results.is_empty());
    assert!(index.nearest_neighbor([0.0, 0.0]).is_none());
}

#[test]
fn bulk_load_and_query_within_radius() {
    let ids: Vec<u32> = (0..10).collect();
    let positions: Vec<[f64; 2]> = (0..10).map(|i| [i as f64 * 10.0, 0.0]).collect();
    let index = SpatialIndex::from_positions(&ids, &positions);

    assert_eq!(index.len(), 10);

    // Query around origin with radius 25m -- should find agents at x=0, 10, 20
    let results = index.nearest_within_radius([0.0, 0.0], 25.0);
    let mut found_ids: Vec<u32> = results.iter().map(|p| p.id).collect();
    found_ids.sort();
    assert_eq!(found_ids, vec![0, 1, 2]);
}

#[test]
fn nearest_neighbor_returns_closest() {
    let ids = vec![0, 1, 2];
    let positions = vec![[10.0, 10.0], [20.0, 20.0], [5.0, 5.0]];
    let index = SpatialIndex::from_positions(&ids, &positions);

    let nearest = index.nearest_neighbor([6.0, 6.0]).unwrap();
    assert_eq!(nearest.id, 2, "agent at (5,5) is closest to (6,6)");
}

#[test]
fn query_radius_excludes_distant_agents() {
    let ids = vec![0, 1];
    let positions = vec![[0.0, 0.0], [1000.0, 1000.0]];
    let index = SpatialIndex::from_positions(&ids, &positions);

    let results = index.nearest_within_radius([0.0, 0.0], 10.0);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, 0);
}

#[test]
fn large_bulk_load() {
    let n = 1000;
    let ids: Vec<u32> = (0..n).collect();
    let positions: Vec<[f64; 2]> = (0..n)
        .map(|i| {
            let angle = (i as f64) * 0.1;
            [angle.cos() * (i as f64), angle.sin() * (i as f64)]
        })
        .collect();
    let index = SpatialIndex::from_positions(&ids, &positions);
    assert_eq!(index.len(), n as usize);

    // Just verify it doesn't crash and returns something
    let results = index.nearest_within_radius([0.0, 0.0], 50.0);
    assert!(!results.is_empty());
}
