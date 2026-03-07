//! Tests for CarFollowingModel enum and GpuAgentState struct.

use velos_core::components::{CarFollowingModel, GpuAgentState};

#[test]
fn car_following_model_discriminants() {
    assert_eq!(CarFollowingModel::Idm as u8, 0);
    assert_eq!(CarFollowingModel::Krauss as u8, 1);
}

#[test]
fn car_following_model_equality() {
    assert_eq!(CarFollowingModel::Idm, CarFollowingModel::Idm);
    assert_ne!(CarFollowingModel::Idm, CarFollowingModel::Krauss);
}

#[test]
fn gpu_agent_state_size_is_32_bytes() {
    assert_eq!(
        std::mem::size_of::<GpuAgentState>(),
        32,
        "GpuAgentState must be exactly 32 bytes for GPU buffer alignment"
    );
}

#[test]
fn gpu_agent_state_is_pod() {
    // Verify bytemuck::Pod works by zero-initializing
    let state = GpuAgentState::zeroed();
    assert_eq!(state.edge_id, 0);
    assert_eq!(state.lane_idx, 0);
    assert_eq!(state.position, 0);
    assert_eq!(state.lateral, 0);
    assert_eq!(state.speed, 0);
    assert_eq!(state.acceleration, 0);
    assert_eq!(state.cf_model, 0);
    assert_eq!(state.rng_state, 0);
}

#[test]
fn gpu_agent_state_bytemuck_cast() {
    let state = GpuAgentState {
        edge_id: 42,
        lane_idx: 1,
        position: 65536,  // 1.0 in Q16.16
        lateral: 256,     // 1.0 in Q8.8
        speed: 1048576,   // 1.0 in Q12.20
        acceleration: 0,
        cf_model: 1,      // Krauss
        rng_state: 0xDEAD_BEEF,
    };

    // Round-trip through bytes
    let bytes: &[u8] = bytemuck::bytes_of(&state);
    assert_eq!(bytes.len(), 32);

    let back: &GpuAgentState = bytemuck::from_bytes(bytes);
    assert_eq!(*back, state);
}

/// Helper trait to call zeroed() on Pod types.
trait Zeroed: bytemuck::Zeroable {
    fn zeroed() -> Self;
}

impl<T: bytemuck::Zeroable> Zeroed for T {
    fn zeroed() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}
