//! R-tree spatial index for agent neighbor queries.
//!
//! Wraps `rstar::RTree` with a bulk-loaded index of agent positions.
//! Rebuilt each frame via `from_positions` (O(n log n) bulk load).

use rstar::{PointDistance, RTree, RTreeObject, AABB};

/// A point in the spatial index representing an agent.
#[derive(Debug, Clone, Copy)]
pub struct AgentPoint {
    /// Agent identifier.
    pub id: u32,
    /// Position in local metres [x, y].
    pub pos: [f64; 2],
}

impl RTreeObject for AgentPoint {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.pos)
    }
}

impl PointDistance for AgentPoint {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        let dx = self.pos[0] - point[0];
        let dy = self.pos[1] - point[1];
        dx * dx + dy * dy
    }
}

/// R-tree spatial index for fast neighbor queries on agent positions.
pub struct SpatialIndex {
    tree: RTree<AgentPoint>,
}

impl SpatialIndex {
    /// Build a spatial index from parallel slices of agent IDs and positions.
    ///
    /// Uses `RTree::bulk_load` for O(n log n) construction.
    pub fn from_positions(ids: &[u32], positions: &[[f64; 2]]) -> Self {
        assert_eq!(ids.len(), positions.len());
        let points: Vec<AgentPoint> = ids
            .iter()
            .zip(positions.iter())
            .map(|(&id, &pos)| AgentPoint { id, pos })
            .collect();
        Self {
            tree: RTree::bulk_load(points),
        }
    }

    /// Create an empty spatial index.
    pub fn empty() -> Self {
        Self {
            tree: RTree::new(),
        }
    }

    /// Find all agents within `radius` metres of `pos`.
    ///
    /// Returns references to matching `AgentPoint`s (unsorted).
    pub fn nearest_within_radius(&self, pos: [f64; 2], radius: f64) -> Vec<&AgentPoint> {
        let radius_sq = radius * radius;
        self.tree
            .locate_within_distance(pos, radius_sq)
            .collect()
    }

    /// Find agents within `radius` metres of `pos`, capped at `max_count` nearest.
    ///
    /// When more than `max_count` agents fall within the radius, the results
    /// are sorted by distance and truncated. This prevents O(n^2) behavior
    /// in dense clusters where dozens of agents overlap.
    pub fn nearest_within_radius_capped(
        &self,
        pos: [f64; 2],
        radius: f64,
        max_count: usize,
    ) -> Vec<&AgentPoint> {
        let radius_sq = radius * radius;
        let mut results: Vec<&AgentPoint> =
            self.tree.locate_within_distance(pos, radius_sq).collect();
        if results.len() > max_count {
            results.sort_by(|a, b| {
                let da = (a.pos[0] - pos[0]).powi(2) + (a.pos[1] - pos[1]).powi(2);
                let db = (b.pos[0] - pos[0]).powi(2) + (b.pos[1] - pos[1]).powi(2);
                da.partial_cmp(&db).unwrap()
            });
            results.truncate(max_count);
        }
        results
    }

    /// Find the single nearest agent to `pos`, if any.
    pub fn nearest_neighbor(&self, pos: [f64; 2]) -> Option<&AgentPoint> {
        self.tree.nearest_neighbor(&pos)
    }

    /// Number of agents in the index.
    pub fn len(&self) -> usize {
        self.tree.size()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.tree.size() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_index(points: &[(u32, f64, f64)]) -> SpatialIndex {
        let ids: Vec<u32> = points.iter().map(|(id, _, _)| *id).collect();
        let positions: Vec<[f64; 2]> = points.iter().map(|(_, x, y)| [*x, *y]).collect();
        SpatialIndex::from_positions(&ids, &positions)
    }

    #[test]
    fn nearest_within_radius_returns_all_in_range() {
        let idx = make_index(&[(1, 0.0, 0.0), (2, 1.0, 0.0), (3, 5.0, 0.0)]);
        let results = idx.nearest_within_radius([0.0, 0.0], 2.0);
        let ids: Vec<u32> = results.iter().map(|p| p.id).collect();
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
        assert!(!ids.contains(&3));
    }

    #[test]
    fn capped_returns_at_most_max_count() {
        // Place 10 agents all within 2m of origin
        let points: Vec<(u32, f64, f64)> = (0..10).map(|i| (i, 0.1 * i as f64, 0.0)).collect();
        let idx = make_index(&points);
        let results = idx.nearest_within_radius_capped([0.0, 0.0], 5.0, 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn capped_returns_nearest_agents() {
        // Agent 1 at 1m, agent 2 at 2m, agent 3 at 3m, agent 4 at 4m
        let idx = make_index(&[(1, 1.0, 0.0), (2, 2.0, 0.0), (3, 3.0, 0.0), (4, 4.0, 0.0)]);
        let results = idx.nearest_within_radius_capped([0.0, 0.0], 5.0, 2);
        let ids: Vec<u32> = results.iter().map(|p| p.id).collect();
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
        assert!(!ids.contains(&3));
        assert!(!ids.contains(&4));
    }

    #[test]
    fn capped_under_limit_returns_all() {
        let idx = make_index(&[(1, 1.0, 0.0), (2, 2.0, 0.0)]);
        let results = idx.nearest_within_radius_capped([0.0, 0.0], 5.0, 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn empty_index_returns_empty() {
        let idx = SpatialIndex::empty();
        assert!(idx.nearest_within_radius([0.0, 0.0], 10.0).is_empty());
        assert!(idx.nearest_within_radius_capped([0.0, 0.0], 10.0, 5).is_empty());
    }
}
