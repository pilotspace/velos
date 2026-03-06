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
