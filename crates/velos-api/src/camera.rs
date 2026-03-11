//! Camera registry with FOV-to-edge spatial mapping.
//!
//! Stores registered cameras with their position, heading, FOV, and range.
//! Computes covered road edges via spatial cone query against an rstar R-tree.

use std::collections::HashMap;

use rstar::{RTree, AABB};
use velos_net::snap::EdgeSegment;
use velos_net::EquirectangularProjection;

use crate::proto::velos::v2::RegisterCameraRequest;

/// A registered camera with computed covered edges.
#[derive(Debug, Clone)]
pub struct Camera {
    pub id: u32,
    pub lat: f64,
    pub lon: f64,
    pub heading_deg: f32,
    pub fov_deg: f32,
    pub range_m: f32,
    pub name: String,
    pub covered_edges: Vec<u32>,
}

/// Registry of cameras with sequential ID assignment.
#[derive(Debug)]
pub struct CameraRegistry {
    cameras: HashMap<u32, Camera>,
    next_id: u32,
}

impl CameraRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            cameras: HashMap::new(),
            next_id: 1,
        }
    }

    /// Register a camera, computing its covered edges via FOV spatial query.
    ///
    /// Returns the newly created `Camera` with assigned ID and covered edges.
    pub fn register(
        &mut self,
        request: &RegisterCameraRequest,
        edge_tree: &RTree<EdgeSegment>,
        projection: &EquirectangularProjection,
    ) -> Camera {
        let id = self.next_id;
        self.next_id += 1;

        let (cam_x, cam_y) = projection.project(request.lat, request.lon);
        let cam_pos = [cam_x, cam_y];

        let heading_rad = (request.heading_deg as f64).to_radians();
        let half_angle_rad = (request.fov_deg as f64 / 2.0).to_radians();
        let range = request.range_m as f64;

        let covered_edges = edges_in_fov(cam_pos, heading_rad, half_angle_rad, range, edge_tree);

        let camera = Camera {
            id,
            lat: request.lat,
            lon: request.lon,
            heading_deg: request.heading_deg,
            fov_deg: request.fov_deg,
            range_m: request.range_m,
            name: request.name.clone(),
            covered_edges,
        };

        self.cameras.insert(id, camera.clone());
        camera
    }

    /// Get a camera by ID.
    pub fn get(&self, id: u32) -> Option<&Camera> {
        self.cameras.get(&id)
    }

    /// Check if a camera ID exists.
    pub fn contains(&self, id: u32) -> bool {
        self.cameras.contains_key(&id)
    }

    /// List all registered cameras.
    pub fn list(&self) -> Vec<&Camera> {
        self.cameras.values().collect()
    }

    /// Insert a camera directly without spatial query (test/internal use).
    ///
    /// Assigns the next sequential ID and stores the camera. Use this when
    /// no R-tree or projection is available (e.g., unit tests in downstream crates).
    pub fn insert_camera(&mut self, name: &str, covered_edges: Vec<u32>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.cameras.insert(
            id,
            Camera {
                id,
                lat: 0.0,
                lon: 0.0,
                heading_deg: 0.0,
                fov_deg: 60.0,
                range_m: 40.0,
                name: name.to_string(),
                covered_edges,
            },
        );
        id
    }
}

impl Default for CameraRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Find edge IDs whose midpoint falls within a camera's FOV cone.
///
/// The cone is defined by `cam_pos` (local metres), `heading_rad` (angle from
/// positive X axis), `half_angle_rad` (half the FOV), and `range_m`.
///
/// Algorithm:
/// 1. Bounding-box query for circle of radius `range_m` around `cam_pos`
/// 2. For each segment, compute midpoint distance and angle
/// 3. Check distance < range_m and angular difference < half_angle
/// 4. Deduplicate edge IDs
pub fn edges_in_fov(
    cam_pos: [f64; 2],
    heading_rad: f64,
    half_angle_rad: f64,
    range_m: f64,
    tree: &RTree<EdgeSegment>,
) -> Vec<u32> {
    let envelope = AABB::from_corners(
        [cam_pos[0] - range_m, cam_pos[1] - range_m],
        [cam_pos[0] + range_m, cam_pos[1] + range_m],
    );

    let mut edges = Vec::new();
    for seg in tree.locate_in_envelope(&envelope) {
        let mid = [
            (seg.segment_start[0] + seg.segment_end[0]) / 2.0,
            (seg.segment_start[1] + seg.segment_end[1]) / 2.0,
        ];

        let dx = mid[0] - cam_pos[0];
        let dy = mid[1] - cam_pos[1];
        let dist = (dx * dx + dy * dy).sqrt();

        if dist > range_m {
            continue;
        }

        // Compute angle to segment midpoint
        let angle_to_seg = dy.atan2(dx);

        // Normalize angle difference to [-PI, PI]
        let mut diff = angle_to_seg - heading_rad;
        diff = ((diff + std::f64::consts::PI) % (2.0 * std::f64::consts::PI))
            - std::f64::consts::PI;
        // Handle negative modulo (Rust % can return negative)
        if diff < -std::f64::consts::PI {
            diff += 2.0 * std::f64::consts::PI;
        }

        if diff.abs() <= half_angle_rad {
            edges.push(seg.edge_id);
        }
    }

    edges.sort_unstable();
    edges.dedup();
    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an RTree with segments at known positions for testing.
    fn make_test_tree(segments: Vec<EdgeSegment>) -> RTree<EdgeSegment> {
        RTree::bulk_load(segments)
    }

    /// Create an edge segment with a midpoint at a known position.
    fn seg_at(edge_id: u32, cx: f64, cy: f64) -> EdgeSegment {
        // 10m segment centered at (cx, cy), horizontal
        EdgeSegment {
            edge_id,
            segment_start: [cx - 5.0, cy],
            segment_end: [cx + 5.0, cy],
            offset_along_edge: 0.0,
        }
    }

    #[test]
    fn register_camera_assigns_sequential_ids() {
        let tree = make_test_tree(vec![]);
        let proj = EquirectangularProjection::new(10.7756, 106.7019);
        let mut registry = CameraRegistry::new();

        let req1 = RegisterCameraRequest {
            lat: 10.775,
            lon: 106.700,
            heading_deg: 90.0,
            fov_deg: 60.0,
            range_m: 50.0,
            name: "cam-1".into(),
        };
        let cam1 = registry.register(&req1, &tree, &proj);
        assert_eq!(cam1.id, 1);

        let req2 = RegisterCameraRequest {
            lat: 10.776,
            lon: 106.701,
            heading_deg: 180.0,
            fov_deg: 90.0,
            range_m: 100.0,
            name: "cam-2".into(),
        };
        let cam2 = registry.register(&req2, &tree, &proj);
        assert_eq!(cam2.id, 2);
    }

    #[test]
    fn edges_in_fov_finds_edges_within_cone() {
        // Camera at origin, heading east (0 rad), 60deg FOV (30deg half-angle), 100m range
        let cam_pos = [0.0, 0.0];
        let heading_rad = 0.0; // east
        let half_angle_rad = 30.0_f64.to_radians();
        let range_m = 100.0;

        // Edge at (50, 10) -- within cone (angle ~11deg from east, within 30deg half)
        // Edge at (50, 60) -- outside cone (angle ~50deg from east, beyond 30deg half)
        // Edge at (150, 0) -- beyond range
        let tree = make_test_tree(vec![
            seg_at(1, 50.0, 10.0),  // inside cone
            seg_at(2, 50.0, 60.0),  // outside angular bounds
            seg_at(3, 150.0, 0.0),  // outside range
        ]);

        let result = edges_in_fov(cam_pos, heading_rad, half_angle_rad, range_m, &tree);
        assert_eq!(result, vec![1], "only edge 1 should be in the FOV cone");
    }

    #[test]
    fn edges_in_fov_handles_heading_wraparound() {
        // Camera heading nearly north: heading_rad ~= PI/2 (90 deg from east = north)
        // Actually, let's use heading in atan2 convention: north = PI/2
        // Edge at angle 80deg from east (slightly west of north)
        // FOV half-angle = 30deg
        // So edges at 60..120 deg from east should be included

        let cam_pos = [0.0, 0.0];
        // heading_deg = 10 deg from east (nearly east-northeast)
        // heading_rad ~ 0.1745 rad
        // half_angle = 30 deg = 0.5236 rad
        // So edges from -20 to +40 deg from east

        // Edge at 350 deg from east = -10 deg -- should be IN with heading 10, half 30
        let heading_rad = 10.0_f64.to_radians();
        let half_angle_rad = 30.0_f64.to_radians();
        let range_m = 100.0;

        // Place edge at angle 350 deg (= -10 deg) from east, 50m away
        // x = 50 * cos(350 deg) = 50 * cos(-10 deg) ~ 49.24
        // y = 50 * sin(350 deg) = 50 * sin(-10 deg) ~ -8.68
        let x = 50.0 * (-10.0_f64).to_radians().cos();
        let y = 50.0 * (-10.0_f64).to_radians().sin();

        let tree = make_test_tree(vec![seg_at(1, x, y)]);

        let result = edges_in_fov(cam_pos, heading_rad, half_angle_rad, range_m, &tree);
        assert_eq!(
            result,
            vec![1],
            "edge at 350deg should be within FOV of heading=10deg, half=30deg"
        );
    }

    #[test]
    fn edges_in_fov_excludes_edges_outside_range() {
        let cam_pos = [0.0, 0.0];
        let heading_rad = 0.0;
        let half_angle_rad = std::f64::consts::PI; // 180 deg = full hemisphere
        let range_m = 50.0;

        // Edge at (60, 0) -- within angular bounds but beyond 50m range
        let tree = make_test_tree(vec![seg_at(1, 60.0, 0.0)]);

        let result = edges_in_fov(cam_pos, heading_rad, half_angle_rad, range_m, &tree);
        assert!(
            result.is_empty(),
            "edge at 60m should be excluded when range is 50m"
        );
    }

    #[test]
    fn get_camera_returns_correct_camera() {
        let tree = make_test_tree(vec![]);
        let proj = EquirectangularProjection::new(10.7756, 106.7019);
        let mut registry = CameraRegistry::new();

        let req = RegisterCameraRequest {
            lat: 10.775,
            lon: 106.700,
            heading_deg: 90.0,
            fov_deg: 60.0,
            range_m: 50.0,
            name: "test-cam".into(),
        };
        registry.register(&req, &tree, &proj);

        let cam = registry.get(1).expect("camera 1 should exist");
        assert_eq!(cam.name, "test-cam");
        assert_eq!(cam.heading_deg, 90.0);

        assert!(registry.get(999).is_none());
    }

    #[test]
    fn list_cameras_returns_all() {
        let tree = make_test_tree(vec![]);
        let proj = EquirectangularProjection::new(10.7756, 106.7019);
        let mut registry = CameraRegistry::new();

        for i in 0..3 {
            let req = RegisterCameraRequest {
                lat: 10.775,
                lon: 106.700,
                heading_deg: 0.0,
                fov_deg: 60.0,
                range_m: 50.0,
                name: format!("cam-{i}"),
            };
            registry.register(&req, &tree, &proj);
        }

        let all = registry.list();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn edges_in_fov_deduplicates() {
        let cam_pos = [0.0, 0.0];
        let heading_rad = 0.0;
        let half_angle_rad = std::f64::consts::PI;
        let range_m = 100.0;

        // Two segments of the same edge (edge_id=1) both within cone
        let tree = make_test_tree(vec![
            EdgeSegment {
                edge_id: 1,
                segment_start: [20.0, 0.0],
                segment_end: [30.0, 0.0],
                offset_along_edge: 0.0,
            },
            EdgeSegment {
                edge_id: 1,
                segment_start: [30.0, 0.0],
                segment_end: [40.0, 0.0],
                offset_along_edge: 10.0,
            },
        ]);

        let result = edges_in_fov(cam_pos, heading_rad, half_angle_rad, range_m, &tree);
        assert_eq!(result, vec![1], "duplicate edge IDs should be deduplicated");
    }
}
