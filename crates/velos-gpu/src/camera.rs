//! Camera2D: orthographic 2D camera with zoom and pan.
//!
//! Coordinate system: world space in metres, Y-up.
//! Projection maps world space to wgpu NDC (Y-down clip space).

use glam::{Mat4, Vec2};

const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 100.0;

/// 2D orthographic camera with zoom (scroll wheel) and pan (left-drag or horizontal scroll).
pub struct Camera2D {
    /// World-space center of the view (metres).
    pub center: Vec2,
    /// Zoom level: pixels per world unit. Default 1.0.
    pub zoom: f32,
    /// Window size in pixels.
    pub viewport: Vec2,
    /// Pan state.
    is_panning: bool,
    last_cursor: Vec2,
}

impl Camera2D {
    /// Create a camera centered at origin for the given viewport size.
    pub fn new(viewport: Vec2) -> Self {
        Self {
            center: Vec2::ZERO,
            zoom: 1.0,
            viewport,
            is_panning: false,
            last_cursor: Vec2::ZERO,
        }
    }

    /// Compute the orthographic view-projection matrix.
    /// Maps world-space coordinates to wgpu clip space (Y-down, Z in [0,1]).
    pub fn view_proj_matrix(&self) -> Mat4 {
        let half_w = self.viewport.x / (2.0 * self.zoom);
        let half_h = self.viewport.y / (2.0 * self.zoom);
        Mat4::orthographic_rh(
            self.center.x - half_w,
            self.center.x + half_w,
            self.center.y - half_h,
            self.center.y + half_h,
            -1.0,
            1.0,
        )
    }

    /// Update viewport size (call on window resize).
    pub fn resize(&mut self, new_viewport: Vec2) {
        self.viewport = new_viewport;
    }

    /// Zoom by a multiplicative factor. Clamped to [MIN_ZOOM, MAX_ZOOM].
    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    }

    /// Handle scroll wheel delta. Positive delta = zoom in.
    /// `lines` is the number of scroll lines (1.0 = one click).
    pub fn scroll(&mut self, lines: f32) {
        let factor = 1.1_f32.powf(lines);
        self.zoom_by(factor);
    }

    /// Begin a pan operation at the given cursor position (pixels).
    pub fn begin_pan(&mut self, pos: Vec2) {
        self.is_panning = true;
        self.last_cursor = pos;
    }

    /// Update the pan with the new cursor position.
    /// Translates camera.center by the pixel delta divided by zoom.
    pub fn update_pan(&mut self, pos: Vec2) {
        if self.is_panning {
            let delta = pos - self.last_cursor;
            // Invert X delta (drag right = move world right = camera moves left).
            // Y: winit Y-down, world Y-up -> invert Y delta.
            self.center.x -= delta.x / self.zoom;
            self.center.y += delta.y / self.zoom;
            self.last_cursor = pos;
        }
    }

    /// End the pan operation.
    pub fn end_pan(&mut self) {
        self.is_panning = false;
    }

    /// Returns true if a pan drag is currently active.
    pub fn is_panning(&self) -> bool {
        self.is_panning
    }

    /// Pan the camera by a pixel-space delta (e.g. from scroll X or touch).
    /// Positive dx moves the world right (camera center moves right).
    /// Positive dy moves the world down in screen space (camera center moves up in world space).
    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.center.x += dx / self.zoom;
        self.center.y -= dy / self.zoom;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    #[test]
    fn test_camera_default_zoom() {
        let cam = Camera2D::new(Vec2::new(1280.0, 720.0));
        assert_eq!(cam.zoom, 1.0);
        assert_eq!(cam.center, Vec2::ZERO);
    }

    #[test]
    fn test_camera_zoom_by() {
        let mut cam = Camera2D::new(Vec2::new(1280.0, 720.0));
        cam.zoom_by(2.0);
        assert!((cam.zoom - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_camera_zoom_clamp_max() {
        let mut cam = Camera2D::new(Vec2::new(1280.0, 720.0));
        cam.zoom_by(10_000.0);
        assert_eq!(cam.zoom, 100.0);
    }

    #[test]
    fn test_camera_zoom_clamp_min() {
        let mut cam = Camera2D::new(Vec2::new(1280.0, 720.0));
        cam.zoom_by(0.0001);
        assert_eq!(cam.zoom, 0.1);
    }

    #[test]
    fn test_camera_pan() {
        let mut cam = Camera2D::new(Vec2::new(1280.0, 720.0));
        cam.begin_pan(Vec2::new(100.0, 100.0));
        cam.update_pan(Vec2::new(150.0, 100.0)); // 50px right
        // zoom=1.0 -> center.x -= 50 / 1.0 = -50
        assert!((cam.center.x - (-50.0)).abs() < 1e-4, "center.x={}", cam.center.x);
    }

    #[test]
    fn test_view_proj_matrix_origin() {
        let cam = Camera2D::new(Vec2::new(1280.0, 720.0));
        let m = cam.view_proj_matrix();
        // World origin (0,0,0,1) should map to NDC (0,0,?,1) approximately
        let ndc = m * glam::Vec4::new(0.0, 0.0, 0.0, 1.0);
        assert!((ndc.x).abs() < 1e-5, "NDC x at origin: {}", ndc.x);
        assert!((ndc.y).abs() < 1e-5, "NDC y at origin: {}", ndc.y);
    }

    #[test]
    fn test_view_proj_matrix_zoom_halves_world() {
        let mut cam = Camera2D::new(Vec2::new(1280.0, 720.0));
        let m1 = cam.view_proj_matrix();
        cam.zoom_by(2.0);
        let m2 = cam.view_proj_matrix();
        // With zoom=2, the same world point is further from center in NDC
        let world_pt = glam::Vec4::new(100.0, 0.0, 0.0, 1.0);
        let ndc1 = m1 * world_pt;
        let ndc2 = m2 * world_pt;
        assert!(
            ndc2.x.abs() > ndc1.x.abs(),
            "Zoom=2 should produce larger NDC x: ndc1={} ndc2={}",
            ndc1.x,
            ndc2.x
        );
    }
}
