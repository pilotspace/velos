//! gRPC detection ingestion service with async-sync bridge for VELOS.
//!
//! This crate provides:
//! - Protobuf-generated types for the DetectionService gRPC contract
//! - Error types mapping to tonic::Status codes
//! - Async-sync bridge channel types for gRPC-to-SimWorld communication
//! - Detection service implementation (Plan 02)
//! - Camera registry with FOV-to-edge mapping (Plan 02)
//! - Windowed detection aggregation (Plan 03)
//! - Demand calibration overlay (Plan 03)

pub mod bridge;
pub mod error;

// Stub modules -- implemented in subsequent plans.
pub mod aggregator;
pub mod calibration;
pub mod camera;
pub mod detection;

/// Generated protobuf types for the velos.v2 package.
pub mod proto {
    pub mod velos {
        pub mod v2 {
            tonic::include_proto!("velos.v2");
        }
    }
}

// Re-export commonly used proto types at crate root for convenience.
pub use proto::velos::v2::{
    CameraInfo, DetectionAck, DetectionBatch, DetectionEvent, ListCamerasRequest,
    ListCamerasResponse, RegisterCameraRequest, RegisterCameraResponse, VehicleClass,
};
