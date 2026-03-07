//! Tests for CarFollowingModel enum, GpuAgentState struct, and VehicleType enum.

use velos_core::components::{CarFollowingModel, GpuAgentState, VehicleType};

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
fn gpu_agent_state_size_is_40_bytes() {
    assert_eq!(
        std::mem::size_of::<GpuAgentState>(),
        40,
        "GpuAgentState must be exactly 40 bytes for GPU buffer alignment"
    );
}

#[test]
fn gpu_agent_state_vehicle_type_offset_is_32() {
    let state = GpuAgentState {
        edge_id: 0,
        lane_idx: 0,
        position: 0,
        lateral: 0,
        speed: 0,
        acceleration: 0,
        cf_model: 0,
        rng_state: 0,
        vehicle_type: 0xDEAD_BEEFu32,
        flags: 0,
    };
    let bytes: &[u8] = bytemuck::bytes_of(&state);
    // vehicle_type is at byte offset 32..36
    let vt_bytes = &bytes[32..36];
    let vt_val = u32::from_ne_bytes(vt_bytes.try_into().unwrap());
    assert_eq!(vt_val, 0xDEAD_BEEF, "vehicle_type field should be at byte offset 32");
}

#[test]
fn gpu_agent_state_flags_offset_is_36() {
    let state = GpuAgentState {
        edge_id: 0,
        lane_idx: 0,
        position: 0,
        lateral: 0,
        speed: 0,
        acceleration: 0,
        cf_model: 0,
        rng_state: 0,
        vehicle_type: 0,
        flags: 0xCAFE_BABEu32,
    };
    let bytes: &[u8] = bytemuck::bytes_of(&state);
    let flags_bytes = &bytes[36..40];
    let flags_val = u32::from_ne_bytes(flags_bytes.try_into().unwrap());
    assert_eq!(flags_val, 0xCAFE_BABE, "flags field should be at byte offset 36");
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
    assert_eq!(state.vehicle_type, 0);
    assert_eq!(state.flags, 0);
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
        vehicle_type: 1,  // Car
        flags: 0,
    };

    // Round-trip through bytes
    let bytes: &[u8] = bytemuck::bytes_of(&state);
    assert_eq!(bytes.len(), 40);

    let back: &GpuAgentState = bytemuck::from_bytes(bytes);
    assert_eq!(*back, state);
}

#[test]
fn gpu_agent_state_bytemuck_cast_slice() {
    let states = vec![
        GpuAgentState {
            edge_id: 0, lane_idx: 0, position: 0, lateral: 0,
            speed: 0, acceleration: 0, cf_model: 0, rng_state: 0,
            vehicle_type: 0, flags: 0,
        },
        GpuAgentState {
            edge_id: 1, lane_idx: 1, position: 1, lateral: 1,
            speed: 1, acceleration: 1, cf_model: 1, rng_state: 1,
            vehicle_type: 1, flags: 1,
        },
    ];

    let bytes: &[u8] = bytemuck::cast_slice(&states);
    assert_eq!(bytes.len(), 80); // 2 * 40 bytes

    let back: &[GpuAgentState] = bytemuck::cast_slice(bytes);
    assert_eq!(back.len(), 2);
    assert_eq!(back[0], states[0]);
    assert_eq!(back[1], states[1]);
}

#[test]
fn vehicle_type_has_7_variants() {
    // Verify all 7 variants exist by constructing each
    let variants = [
        VehicleType::Motorbike,
        VehicleType::Car,
        VehicleType::Bus,
        VehicleType::Bicycle,
        VehicleType::Truck,
        VehicleType::Emergency,
        VehicleType::Pedestrian,
    ];
    assert_eq!(variants.len(), 7);

    // Each variant should be distinct
    for (i, a) in variants.iter().enumerate() {
        for (j, b) in variants.iter().enumerate() {
            if i == j {
                assert_eq!(a, b);
            } else {
                assert_ne!(a, b);
            }
        }
    }
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
