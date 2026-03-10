//! 3D camera input handling extracted from app.rs to stay under 700 lines.
//!
//! Handles orbit camera controls: left-drag orbit, scroll zoom, middle-drag pan.
//! Also handles view mode toggling between 2D and 3D.

use glam::Vec2;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};

use crate::camera::Camera2D;
use crate::orbit_camera::{OrbitCamera, ViewMode, ViewTransition};

/// Sensitivity for orbit rotation (radians per pixel of mouse drag).
const ORBIT_SENSITIVITY: f32 = 0.005;

/// Sensitivity for pan (world units per pixel of mouse drag).
const PAN_SENSITIVITY: f32 = 1.0;

/// Zoom factor per scroll unit.
const SCROLL_ZOOM_FACTOR: f32 = 0.1;

/// Handle 3D orbit camera input events.
///
/// Returns `true` if the event was consumed by the 3D input handler.
pub fn handle_3d_input(
    event: &WindowEvent,
    orbit_camera: &mut OrbitCamera,
    left_pressed: &mut bool,
    middle_pressed: &mut bool,
    last_cursor_pos: &mut Option<(f32, f32)>,
) -> bool {
    match event {
        WindowEvent::MouseInput {
            state: btn_state,
            button,
            ..
        } => {
            match button {
                MouseButton::Left => {
                    *left_pressed = *btn_state == ElementState::Pressed;
                    true
                }
                MouseButton::Middle => {
                    *middle_pressed = *btn_state == ElementState::Pressed;
                    true
                }
                _ => false,
            }
        }

        WindowEvent::CursorMoved { position, .. } => {
            let new_pos = (position.x as f32, position.y as f32);
            if let Some((prev_x, prev_y)) = *last_cursor_pos {
                let dx = new_pos.0 - prev_x;
                let dy = new_pos.1 - prev_y;

                if *left_pressed {
                    // Left-drag: orbit camera
                    orbit_camera.orbit(-dx * ORBIT_SENSITIVITY, dy * ORBIT_SENSITIVITY);
                } else if *middle_pressed {
                    // Middle-drag: pan camera focus point
                    let scale = orbit_camera.distance * PAN_SENSITIVITY * 0.002;
                    orbit_camera.pan(-dx * scale, dy * scale);
                }
            }
            *last_cursor_pos = Some(new_pos);
            *left_pressed || *middle_pressed
        }

        WindowEvent::MouseWheel { delta, .. } => {
            let scroll_y = match delta {
                MouseScrollDelta::LineDelta(_, y) => *y,
                MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 20.0,
            };
            // Negative scroll = zoom in (smaller distance), positive = zoom out
            let factor = 1.0 - scroll_y * SCROLL_ZOOM_FACTOR;
            orbit_camera.zoom_by(factor);
            true
        }

        _ => false,
    }
}

/// Initiate a view mode toggle, returning the new transition.
///
/// When switching 2D -> 3D: creates OrbitCamera from Camera2D state.
/// When switching 3D -> 2D: maps orbit focus back to Camera2D center.
pub fn toggle_view_mode(
    current_mode: ViewMode,
    camera_2d: &mut Camera2D,
    orbit_camera: &mut OrbitCamera,
) -> ViewTransition {
    match current_mode {
        ViewMode::TopDown2D => {
            *orbit_camera = OrbitCamera::from_camera_2d(camera_2d);
            ViewTransition::new(ViewMode::TopDown2D, ViewMode::Perspective3D)
        }
        ViewMode::Perspective3D => {
            // Map orbit focus (x, 0, z) back to 2D center (x, z)
            camera_2d.center = Vec2::new(orbit_camera.focus.x, orbit_camera.focus.z);
            ViewTransition::new(ViewMode::Perspective3D, ViewMode::TopDown2D)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    #[test]
    fn toggle_2d_to_3d_creates_orbit_from_camera() {
        let mut cam2d = Camera2D::new(Vec2::new(1280.0, 720.0));
        cam2d.center = Vec2::new(500.0, 300.0);
        let mut orbit = OrbitCamera::new(Vec2::new(1280.0, 720.0));

        let transition = toggle_view_mode(ViewMode::TopDown2D, &mut cam2d, &mut orbit);

        assert_eq!(transition.from, ViewMode::TopDown2D);
        assert_eq!(transition.to, ViewMode::Perspective3D);
        // Focus should map from 2D center
        assert!((orbit.focus.x - 500.0).abs() < 0.01);
        assert!((orbit.focus.z - 300.0).abs() < 0.01);
    }

    #[test]
    fn toggle_3d_to_2d_maps_focus_back() {
        let mut cam2d = Camera2D::new(Vec2::new(1280.0, 720.0));
        let mut orbit = OrbitCamera::new(Vec2::new(1280.0, 720.0));
        orbit.focus = glam::Vec3::new(100.0, 0.0, 200.0);

        let transition = toggle_view_mode(ViewMode::Perspective3D, &mut cam2d, &mut orbit);

        assert_eq!(transition.from, ViewMode::Perspective3D);
        assert_eq!(transition.to, ViewMode::TopDown2D);
        assert!((cam2d.center.x - 100.0).abs() < 0.01);
        assert!((cam2d.center.y - 200.0).abs() < 0.01);
    }

    #[test]
    fn orbit_sensitivity_positive() {
        assert!(ORBIT_SENSITIVITY > 0.0);
    }

    #[test]
    fn scroll_zoom_in_reduces_distance() {
        let mut orbit = OrbitCamera::new(Vec2::new(800.0, 600.0));
        let initial = orbit.distance;
        // Simulate scroll up (positive y)
        let factor = 1.0 - 1.0 * SCROLL_ZOOM_FACTOR;
        orbit.zoom_by(factor);
        assert!(orbit.distance < initial, "Scroll up should zoom in");
    }

    #[test]
    fn scroll_zoom_out_increases_distance() {
        let mut orbit = OrbitCamera::new(Vec2::new(800.0, 600.0));
        let initial = orbit.distance;
        // Simulate scroll down (negative y)
        let factor = 1.0 - (-1.0) * SCROLL_ZOOM_FACTOR;
        orbit.zoom_by(factor);
        assert!(orbit.distance > initial, "Scroll down should zoom out");
    }
}
