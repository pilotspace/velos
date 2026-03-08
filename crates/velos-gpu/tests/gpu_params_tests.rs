//! Tests for GpuVehicleParams struct and VehicleConfig -> GPU buffer conversion.

use velos_gpu::compute::GpuVehicleParams;
use velos_vehicle::config::VehicleConfig;

#[test]
fn gpu_vehicle_params_size_is_224_bytes() {
    assert_eq!(
        std::mem::size_of::<GpuVehicleParams>(),
        224,
        "GpuVehicleParams must be 7 types * 8 f32 * 4 bytes = 224 bytes"
    );
}

#[test]
fn gpu_vehicle_params_from_config_motorbike_at_index_0() {
    let config = VehicleConfig::default();
    let gpu_params = GpuVehicleParams::from_config(&config);

    // Index 0 = Motorbike
    let moto = gpu_params.params[0];
    let eps = 0.01_f32;
    assert!((moto[0] - 11.1).abs() < eps, "motorbike v0");
    assert!((moto[1] - 1.0).abs() < eps, "motorbike s0");
    assert!((moto[2] - 0.8).abs() < eps, "motorbike t_headway");
    assert!((moto[3] - 2.0).abs() < eps, "motorbike a");
    assert!((moto[4] - 3.0).abs() < eps, "motorbike b");
    assert!((moto[5] - 2.0).abs() < eps, "motorbike krauss_accel");
    assert!((moto[6] - 3.0).abs() < eps, "motorbike krauss_decel");
    assert!((moto[7] - 0.3).abs() < eps, "motorbike krauss_sigma");
}

#[test]
fn gpu_vehicle_params_from_config_car_at_index_1() {
    let config = VehicleConfig::default();
    let gpu_params = GpuVehicleParams::from_config(&config);

    // Index 1 = Car
    let car = gpu_params.params[1];
    let eps = 0.01_f32;
    assert!((car[0] - 9.7).abs() < eps, "car v0");
    assert!((car[1] - 2.0).abs() < eps, "car s0");
    assert!((car[2] - 1.5).abs() < eps, "car t_headway");
    assert!((car[3] - 1.0).abs() < eps, "car a");
    assert!((car[4] - 2.0).abs() < eps, "car b");
    assert!((car[5] - 1.0).abs() < eps, "car krauss_accel");
    assert!((car[6] - 4.5).abs() < eps, "car krauss_decel");
    assert!((car[7] - 0.5).abs() < eps, "car krauss_sigma");
}

#[test]
fn gpu_vehicle_params_from_config_all_types_indexed_correctly() {
    let config = VehicleConfig::default();
    let gpu_params = GpuVehicleParams::from_config(&config);

    // Verify all 7 types have v0 matching config defaults
    let expected_v0: [f32; 7] = [
        11.1,  // 0: Motorbike
        9.7,   // 1: Car
        8.3,   // 2: Bus
        4.17,  // 3: Bicycle
        9.7,   // 4: Truck
        16.7,  // 5: Emergency
        1.2,   // 6: Pedestrian (desired_speed)
    ];

    let eps = 0.01_f32;
    for (i, &expected) in expected_v0.iter().enumerate() {
        assert!(
            (gpu_params.params[i][0] - expected).abs() < eps,
            "vehicle type {} v0: expected {}, got {}",
            i,
            expected,
            gpu_params.params[i][0],
        );
    }
}

#[test]
fn gpu_vehicle_params_pedestrian_at_index_6() {
    let config = VehicleConfig::default();
    let gpu_params = GpuVehicleParams::from_config(&config);

    // Index 6 = Pedestrian (mapped from PedestrianParams)
    let ped = gpu_params.params[6];
    let eps = 0.01_f32;
    assert!((ped[0] - 1.2).abs() < eps, "pedestrian v0 = desired_speed");
    assert!((ped[1] - 0.5).abs() < eps, "pedestrian s0 = personal_space");
}
