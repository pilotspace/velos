//! gRPC detection ingestion service with async-sync bridge for VELOS.
//!
//! This crate provides:
//! - Protobuf-generated types for the DetectionService gRPC contract
//! - Error types mapping to tonic::Status codes
//! - Async-sync bridge channel types for gRPC-to-SimWorld communication
//! - Camera registry with FOV-to-edge spatial mapping
//! - Windowed detection aggregation per camera per vehicle class
//! - DetectionService gRPC handler implementation
//! - Demand calibration overlay (Plan 03)

pub mod aggregator;
pub mod bridge;
pub mod calibration;
pub mod camera;
pub mod detection;
pub mod error;

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

// Re-export key types for external consumers.
pub use aggregator::DetectionAggregator;
pub use bridge::{ApiBridge, ApiCommand};
pub use camera::{Camera, CameraRegistry};
pub use detection::DetectionServiceImpl;
pub use error::ApiError;

use std::sync::{Arc, Mutex};

use proto::velos::v2::detection_service_server::DetectionServiceServer;
use rstar::RTree;
use tokio::sync::mpsc;
use velos_net::snap::EdgeSegment;
use velos_net::EquirectangularProjection;

/// Create a tonic DetectionServiceServer ready to be added to a gRPC server.
///
/// This factory function wires together all the shared state needed by the
/// DetectionService implementation.
pub fn create_detection_service(
    cmd_tx: mpsc::Sender<ApiCommand>,
    aggregator: Arc<Mutex<DetectionAggregator>>,
    registry: Arc<Mutex<CameraRegistry>>,
    edge_tree: Arc<RTree<EdgeSegment>>,
    projection: Arc<EquirectangularProjection>,
) -> DetectionServiceServer<DetectionServiceImpl> {
    let service = DetectionServiceImpl::new(cmd_tx, aggregator, registry, edge_tree, projection);
    DetectionServiceServer::new(service)
}
