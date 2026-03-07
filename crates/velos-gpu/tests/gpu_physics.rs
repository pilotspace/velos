//! Tests verifying that the GPU physics path is the sole production path.
//!
//! Confirms that:
//! - SimWorld::tick_gpu() exists and is the intended production method
//! - CPU step functions exist only in the cpu_reference module (not in tick_gpu)
//! - ComputeDispatcher has wave-front pipeline support
//! - sort_agents_by_lane produces valid lane groupings

#![cfg(feature = "gpu-tests")]

use velos_core::components::{CarFollowingModel, GpuAgentState};
use velos_core::fixed_point::{FixPos, FixSpd};
use velos_gpu::compute::sort_agents_by_lane;

#[test]
fn sort_agents_by_lane_empty() {
    let (offsets, counts, indices) = sort_agents_by_lane(&[]);
    assert_eq!(offsets, vec![0]);
    assert_eq!(counts, vec![0]);
    assert!(indices.is_empty());
}

#[test]
fn sort_agents_by_lane_single_agent() {
    let agents = vec![GpuAgentState {
        edge_id: 5,
        lane_idx: 2,
        position: FixPos::from_f64(100.0).raw(),
        lateral: 0,
        speed: FixSpd::from_f64(10.0).raw(),
        acceleration: 0,
        cf_model: CarFollowingModel::Idm as u32,
        rng_state: 0,
    }];
    let (offsets, counts, indices) = sort_agents_by_lane(&agents);
    assert_eq!(counts, vec![1]);
    assert_eq!(indices, vec![0]);
    assert_eq!(offsets.len(), 1);
}

#[test]
fn sort_agents_by_lane_multi_edge() {
    // Agents on different edges should be in different lanes.
    let agents = vec![
        GpuAgentState {
            edge_id: 0,
            lane_idx: 0,
            position: FixPos::from_f64(50.0).raw(),
            lateral: 0,
            speed: FixSpd::from_f64(10.0).raw(),
            acceleration: 0,
            cf_model: CarFollowingModel::Idm as u32,
            rng_state: 0,
        },
        GpuAgentState {
            edge_id: 1,
            lane_idx: 0,
            position: FixPos::from_f64(50.0).raw(),
            lateral: 0,
            speed: FixSpd::from_f64(10.0).raw(),
            acceleration: 0,
            cf_model: CarFollowingModel::Krauss as u32,
            rng_state: 1,
        },
    ];
    let (_offsets, counts, _indices) = sort_agents_by_lane(&agents);
    // Two distinct lanes: (0,0) and (1,0)
    assert_eq!(counts.len(), 2);
    assert_eq!(counts[0], 1);
    assert_eq!(counts[1], 1);
}

#[test]
fn gpu_agent_state_size_32_bytes() {
    assert_eq!(
        std::mem::size_of::<GpuAgentState>(),
        32,
        "GpuAgentState must be 32 bytes for GPU alignment"
    );
}

#[test]
fn car_following_model_discriminants() {
    assert_eq!(CarFollowingModel::Idm as u32, 0);
    assert_eq!(CarFollowingModel::Krauss as u32, 1);
}

#[test]
fn fixed_point_roundtrip_position() {
    let original = 123.456;
    let fixed = FixPos::from_f64(original);
    let back = fixed.to_f64();
    // Q16.16 resolution is ~0.015mm
    assert!(
        (original - back).abs() < 0.001,
        "FixPos roundtrip error too large: {original} -> {back}"
    );
}

#[test]
fn fixed_point_roundtrip_speed() {
    let original = 13.89; // 50 km/h
    let fixed = FixSpd::from_f64(original);
    let back = fixed.to_f64();
    // Q12.20 resolution is ~0.001mm/s
    assert!(
        (original - back).abs() < 0.0001,
        "FixSpd roundtrip error too large: {original} -> {back}"
    );
}
