//! OrbitCamera: perspective 3D camera with orbit controls.
//!
//! **Coordinate convention:** 2D (x, y) maps to 3D (x, 0, y) -- Y is up.
//! This matches the existing Camera2D world-space convention where the 2D
//! world plane becomes the 3D XZ ground plane with Y as the vertical axis.
//!
//! OrbitCamera produces a perspective view-projection matrix consumed by all
//! 3D shaders (ground plane, mesh instances, billboards). The existing 2D
//! Camera2D and Renderer remain untouched.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3};

use crate::camera::Camera2D;

// --- Pitch clamp constants (radians) ---
const MIN_PITCH: f32 = 5.0 * std::f32::consts::PI / 180.0;
const MAX_PITCH: f32 = 89.0 * std::f32::consts::PI / 180.0;

// --- Orbit distance clamp ---
const MIN_DISTANCE: f32 = 1.0;
const MAX_DISTANCE: f32 = 50_000.0;

// --- View transition ---
const VIEW_TRANSITION_DURATION: f32 = 0.5;

// --- LOD constants ---
/// Distance threshold below which full 3D meshes are rendered.
pub const LOD_MESH_THRESHOLD: f32 = 50.0;
/// Distance threshold below which billboard sprites are rendered (beyond mesh range).
pub const LOD_BILLBOARD_THRESHOLD: f32 = 200.0;
/// Hysteresis factor to prevent LOD flickering at boundary distances.
pub const HYSTERESIS_FACTOR: f32 = 1.1;

/// View mode for the rendering system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// Traditional top-down orthographic 2D view.
    TopDown2D,
    /// Perspective 3D view with orbit camera.
    Perspective3D,
}

/// Animated transition between view modes.
#[derive(Debug, Clone)]
pub struct ViewTransition {
    /// Starting view mode.
    pub from: ViewMode,
    /// Target view mode.
    pub to: ViewMode,
    /// Normalized progress [0.0, 1.0].
    pub progress: f32,
    /// Elapsed time in seconds.
    pub elapsed: f32,
}

impl ViewTransition {
    /// Create a new transition.
    pub fn new(from: ViewMode, to: ViewMode) -> Self {
        Self {
            from,
            to,
            progress: 0.0,
            elapsed: 0.0,
        }
    }

    /// Advance the transition by `dt` seconds. Returns `true` when complete.
    pub fn tick(&mut self, dt: f32) -> bool {
        self.elapsed += dt;
        self.progress = (self.elapsed / VIEW_TRANSITION_DURATION).min(1.0);
        self.progress >= 1.0
    }
}

/// Orbit camera with perspective projection for 3D rendering.
///
/// Uses spherical coordinates (distance, yaw, pitch) around a focus point.
/// Pitch is clamped to [5, 89] degrees to prevent gimbal lock and underground views.
pub struct OrbitCamera {
    /// World-space focus point (camera orbits around this).
    pub focus: Vec3,
    /// Distance from focus point to eye.
    pub distance: f32,
    /// Horizontal rotation in radians (around Y axis).
    pub yaw: f32,
    /// Vertical rotation in radians (elevation above ground plane).
    pub pitch: f32,
    /// Vertical field of view in radians.
    pub fov_y: f32,
    /// Near clipping plane distance.
    pub near: f32,
    /// Far clipping plane distance.
    pub far: f32,
    /// Viewport size in pixels.
    pub viewport: Vec2,
}

impl OrbitCamera {
    /// Create a new orbit camera with sensible defaults.
    ///
    /// Defaults: focus at origin, distance 500m, yaw 0, pitch 45deg,
    /// fov_y 45deg, near 0.1, far 10000.
    pub fn new(viewport: Vec2) -> Self {
        Self {
            focus: Vec3::ZERO,
            distance: 500.0,
            yaw: 0.0,
            pitch: 45.0_f32.to_radians(),
            fov_y: 45.0_f32.to_radians(),
            near: 1.0,
            far: 100_000.0,
            viewport,
        }
    }

    /// Compute eye position in world space from spherical coordinates.
    ///
    /// Spherical to Cartesian: the eye orbits around `focus` with `distance`,
    /// `yaw` (horizontal angle), and `pitch` (elevation angle).
    pub fn eye_position(&self) -> Vec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.cos();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.sin();
        self.focus + Vec3::new(x, y, z)
    }

    /// Compute the combined view-projection matrix (right-handed, reverse-Z).
    ///
    /// Uses `look_at_rh` for the view matrix and infinite reverse-Z projection.
    /// Reverse-Z maps near plane to depth=1.0 and infinity to depth=0.0,
    /// giving much better depth precision at distance (critical for large scenes
    /// with roads/ground at similar Y values). Requires `GreaterEqual` depth
    /// compare and depth clear to 0.0.
    pub fn view_proj_matrix(&self) -> Mat4 {
        let eye = self.eye_position();
        let aspect = self.viewport.x / self.viewport.y;
        let view = Mat4::look_at_rh(eye, self.focus, Vec3::Y);
        let proj = Mat4::perspective_infinite_reverse_rh(self.fov_y, aspect, self.near);
        proj * view
    }

    /// Update viewport size (call on window resize).
    pub fn resize(&mut self, new_viewport: Vec2) {
        self.viewport = new_viewport;
    }

    /// Orbit the camera by delta yaw and pitch (radians).
    /// Pitch is clamped to [5deg, 89deg] to prevent gimbal lock.
    pub fn orbit(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(MIN_PITCH, MAX_PITCH);
    }

    /// Zoom by a multiplicative factor. Distance clamped to [1.0, 50000.0].
    pub fn zoom_by(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// Pan the focus point in camera-relative directions.
    ///
    /// `dx` moves along the camera's right vector projected onto the ground plane.
    /// `dy` moves along the camera's forward vector projected onto the ground plane.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        // Camera right vector (perpendicular to look direction on ground plane)
        let right = Vec3::new((-self.yaw).sin(), 0.0, (-self.yaw).cos());
        // Camera forward vector projected onto ground (XZ plane)
        let forward = Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin());
        self.focus += right * dx + forward * dy;
    }

    /// Create an OrbitCamera from an existing Camera2D state.
    ///
    /// Maps 2D center (x, y) to 3D focus (x, 0, y).
    /// Maps 2D zoom level to orbit distance (inverse relationship).
    /// Sets pitch to 45deg and yaw to 0 for a natural initial 3D view.
    pub fn from_camera_2d(cam: &Camera2D) -> Self {
        // Camera2D zoom is pixels-per-world-unit. Higher zoom = closer view.
        // Map to orbit distance: inverse relationship.
        // A reasonable mapping: distance ~= viewport_height / (2 * zoom * tan(fov/2))
        let fov_y = 45.0_f32.to_radians();
        let half_fov_tan = (fov_y / 2.0).tan();
        let distance = (cam.viewport.y / (2.0 * cam.zoom * half_fov_tan)).max(MIN_DISTANCE);

        Self {
            focus: Vec3::new(cam.center.x, 0.0, cam.center.y),
            distance,
            yaw: 0.0,
            pitch: 45.0_f32.to_radians(),
            fov_y,
            near: 0.1,
            far: 10_000.0,
            viewport: cam.viewport,
        }
    }
}

/// Create a depth texture for 3D rendering.
///
/// Returns a `TextureView` for Depth32Float format at the given dimensions.
/// Used as the depth attachment in 3D render passes.
pub fn create_depth_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth_texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

// --- GPU instance types for 3D rendering (used by Plans 02, 03) ---

/// Per-instance data for 3D mesh rendering.
///
/// 32 bytes total: position (12) + heading (4) + color (16).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MeshInstance3D {
    /// World-space position (x, y, z).
    pub world_pos: [f32; 3],
    /// Heading angle in radians.
    pub heading: f32,
    /// RGBA color.
    pub color: [f32; 4],
}

/// Per-instance data for 3D billboard rendering.
///
/// 40 bytes total: position (12) + size (8) + color (16) + padding (4).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct BillboardInstance3D {
    /// World-space position (x, y, z).
    pub world_pos: [f32; 3],
    /// Billboard size (width, height) in world units.
    pub size: [f32; 2],
    /// RGBA color.
    pub color: [f32; 4],
    /// Padding to align to 8-byte boundary.
    pub _pad: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    #[test]
    fn test_orbit_camera_new_produces_valid_matrix() {
        let cam = OrbitCamera::new(Vec2::new(800.0, 600.0));
        let m = cam.view_proj_matrix();
        let cols = m.to_cols_array();
        // No NaN or Inf values
        for (i, v) in cols.iter().enumerate() {
            assert!(v.is_finite(), "Matrix element {} is not finite: {}", i, v);
        }
        // Determinant should be non-zero (invertible matrix)
        assert!(m.determinant().abs() > 1e-10, "Matrix determinant is ~0");
    }

    #[test]
    fn test_pitch_clamp_lower() {
        let mut cam = OrbitCamera::new(Vec2::new(800.0, 600.0));
        // Set pitch to 0 degrees (below minimum of 5 degrees)
        cam.pitch = 0.0;
        cam.orbit(0.0, 0.0); // trigger clamp via orbit with zero delta
        let min_pitch_rad = 5.0_f32.to_radians();
        assert!(
            (cam.pitch - min_pitch_rad).abs() < 1e-6,
            "Pitch should clamp to 5deg ({}rad), got {}",
            min_pitch_rad,
            cam.pitch
        );
    }

    #[test]
    fn test_pitch_clamp_upper() {
        let mut cam = OrbitCamera::new(Vec2::new(800.0, 600.0));
        // Set pitch to 90 degrees (above maximum of 89 degrees)
        cam.pitch = std::f32::consts::FRAC_PI_2; // 90 deg
        cam.orbit(0.0, 0.0); // trigger clamp
        let max_pitch_rad = 89.0_f32.to_radians();
        assert!(
            (cam.pitch - max_pitch_rad).abs() < 1e-6,
            "Pitch should clamp to 89deg ({}rad), got {}",
            max_pitch_rad,
            cam.pitch
        );
    }

    #[test]
    fn test_eye_position_at_known_angles() {
        let mut cam = OrbitCamera::new(Vec2::new(800.0, 600.0));
        cam.focus = Vec3::ZERO;
        cam.distance = 100.0;
        cam.yaw = 0.0;
        cam.pitch = 45.0_f32.to_radians();

        let eye = cam.eye_position();
        let cos45 = 45.0_f32.to_radians().cos();
        let sin45 = 45.0_f32.to_radians().sin();

        // x = 100 * cos(45) * cos(0) = 100 * cos(45)
        assert!(
            (eye.x - 100.0 * cos45).abs() < 0.1,
            "eye.x={}, expected={}",
            eye.x,
            100.0 * cos45
        );
        // y = 100 * sin(45)
        assert!(
            (eye.y - 100.0 * sin45).abs() < 0.1,
            "eye.y={}, expected={}",
            eye.y,
            100.0 * sin45
        );
        // z = 100 * cos(45) * sin(0) = 0
        assert!(eye.z.abs() < 0.1, "eye.z={}, expected=0", eye.z);
    }

    #[test]
    fn test_from_camera_2d() {
        let cam2d = Camera2D::new(Vec2::new(1280.0, 720.0));
        let orbit = OrbitCamera::from_camera_2d(&cam2d);

        // Focus should map 2D center (0,0) to 3D (0, 0, 0)
        assert_eq!(orbit.focus, Vec3::new(0.0, 0.0, 0.0));
        // Pitch should be 45 degrees
        assert!(
            (orbit.pitch - 45.0_f32.to_radians()).abs() < 1e-6,
            "pitch={}, expected 45deg",
            orbit.pitch.to_degrees()
        );
        // Yaw should be 0
        assert_eq!(orbit.yaw, 0.0);
        // Distance should be positive and finite
        assert!(orbit.distance > 0.0 && orbit.distance.is_finite());
    }

    #[test]
    fn test_from_camera_2d_with_offset() {
        let mut cam2d = Camera2D::new(Vec2::new(1280.0, 720.0));
        cam2d.center = Vec2::new(100.0, 200.0);
        let orbit = OrbitCamera::from_camera_2d(&cam2d);
        // 2D (100, 200) -> 3D (100, 0, 200)
        assert_eq!(orbit.focus, Vec3::new(100.0, 0.0, 200.0));
    }

    #[test]
    fn test_view_mode_enum() {
        let _top = ViewMode::TopDown2D;
        let _persp = ViewMode::Perspective3D;
        assert_ne!(ViewMode::TopDown2D, ViewMode::Perspective3D);
    }

    #[test]
    fn test_view_transition_tick() {
        let mut t = ViewTransition::new(ViewMode::TopDown2D, ViewMode::Perspective3D);
        assert!(!t.tick(0.1)); // 0.1s of 0.5s => not done
        assert!(t.progress < 1.0);
        assert!(!t.tick(0.1)); // 0.2s
        assert!(!t.tick(0.1)); // 0.3s
        assert!(!t.tick(0.1)); // 0.4s
        assert!(t.tick(0.1)); // 0.5s => done
        assert!((t.progress - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_mesh_instance_3d_size() {
        assert_eq!(
            std::mem::size_of::<MeshInstance3D>(),
            32,
            "MeshInstance3D should be 32 bytes"
        );
    }

    #[test]
    fn test_billboard_instance_3d_size() {
        assert_eq!(
            std::mem::size_of::<BillboardInstance3D>(),
            40,
            "BillboardInstance3D should be 40 bytes"
        );
    }

    #[test]
    fn test_zoom_clamp() {
        let mut cam = OrbitCamera::new(Vec2::new(800.0, 600.0));
        cam.zoom_by(0.0001); // try to get very close
        assert!(cam.distance >= 1.0, "Distance should clamp to minimum 1.0");
        cam.zoom_by(1_000_000.0); // try to get very far
        assert!(
            cam.distance <= 50_000.0,
            "Distance should clamp to maximum 50000"
        );
    }

    #[test]
    fn test_resize() {
        let mut cam = OrbitCamera::new(Vec2::new(800.0, 600.0));
        cam.resize(Vec2::new(1920.0, 1080.0));
        assert_eq!(cam.viewport, Vec2::new(1920.0, 1080.0));
        // Matrix should still be valid after resize
        let m = cam.view_proj_matrix();
        assert!(m.determinant().abs() > 1e-10);
    }
}
