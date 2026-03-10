//! DetectionService gRPC trait implementation.
//!
//! Implements the tonic-generated DetectionService trait, bridging gRPC
//! streaming to the simulation world via ApiBridge channels.

use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

use crate::aggregator::DetectionAggregator;
use crate::bridge::ApiCommand;
use crate::camera::CameraRegistry;
use crate::proto::velos::v2::detection_service_server::DetectionService;
use crate::proto::velos::v2::{
    CameraInfo, DetectionAck, DetectionBatch, ListCamerasRequest, ListCamerasResponse,
    RegisterCameraRequest, RegisterCameraResponse,
};

use rstar::RTree;
use velos_net::snap::EdgeSegment;
use velos_net::EquirectangularProjection;

/// gRPC DetectionService implementation.
///
/// Holds shared state for camera registry and detection aggregator,
/// plus a channel sender to forward commands to the simulation world.
pub struct DetectionServiceImpl {
    cmd_tx: mpsc::Sender<ApiCommand>,
    aggregator: Arc<Mutex<DetectionAggregator>>,
    registry: Arc<Mutex<CameraRegistry>>,
    edge_tree: Arc<RTree<EdgeSegment>>,
    projection: Arc<EquirectangularProjection>,
}

impl DetectionServiceImpl {
    /// Create a new DetectionService implementation.
    pub fn new(
        cmd_tx: mpsc::Sender<ApiCommand>,
        aggregator: Arc<Mutex<DetectionAggregator>>,
        registry: Arc<Mutex<CameraRegistry>>,
        edge_tree: Arc<RTree<EdgeSegment>>,
        projection: Arc<EquirectangularProjection>,
    ) -> Self {
        Self {
            cmd_tx,
            aggregator,
            registry,
            edge_tree,
            projection,
        }
    }
}

#[tonic::async_trait]
impl DetectionService for DetectionServiceImpl {
    type StreamDetectionsStream = ReceiverStream<Result<DetectionAck, Status>>;

    async fn stream_detections(
        &self,
        request: Request<Streaming<DetectionBatch>>,
    ) -> Result<Response<Self::StreamDetectionsStream>, Status> {
        let mut stream = request.into_inner();
        let (ack_tx, ack_rx) = mpsc::channel(64);
        let cmd_tx = self.cmd_tx.clone();
        let aggregator = Arc::clone(&self.aggregator);
        let registry = Arc::clone(&self.registry);

        tokio::spawn(async move {
            while let Ok(Some(batch)) = stream.message().await {
                let batch_id = batch.batch_id;

                // Validate all camera IDs and ingest events
                let status = {
                    let reg = registry.lock().unwrap();
                    let mut agg = aggregator.lock().unwrap();

                    let mut all_valid = true;
                    for event in &batch.events {
                        if !reg.contains(event.camera_id) {
                            all_valid = false;
                            break;
                        }
                        agg.ingest(event.camera_id, event);
                    }

                    if all_valid { 0 } else { 1 } // 0 = OK, 1 = UNKNOWN_CAMERA
                };

                // Forward batch to simulation world (fire-and-forget)
                if status == 0 {
                    let _ = cmd_tx
                        .send(ApiCommand::DetectionBatch {
                            batch: DetectionBatch {
                                batch_id,
                                events: batch.events,
                            },
                        })
                        .await;
                }

                // Send ack
                let ack = DetectionAck {
                    batch_id,
                    status,
                };
                if ack_tx.send(Ok(ack)).await.is_err() {
                    break; // Client disconnected
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(ack_rx)))
    }

    async fn register_camera(
        &self,
        request: Request<RegisterCameraRequest>,
    ) -> Result<Response<RegisterCameraResponse>, Status> {
        let req = request.into_inner();

        // Register camera locally (computes covered edges)
        let camera = {
            let mut reg = self.registry.lock().unwrap();
            reg.register(&req, &self.edge_tree, &self.projection)
        };

        // Forward to simulation world via bridge with oneshot reply
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(ApiCommand::RegisterCamera {
                request: req,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Status::unavailable("simulation disconnected"))?;

        // Await reply with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(5), reply_rx).await {
            Ok(Ok(_response)) => {
                // Use local camera data (SimWorld may enhance the response later)
                Ok(Response::new(RegisterCameraResponse {
                    camera_id: camera.id,
                    covered_edge_ids: camera.covered_edges,
                }))
            }
            Ok(Err(_)) => Err(Status::internal("simulation dropped reply channel")),
            Err(_) => Err(Status::deadline_exceeded("camera registration timed out")),
        }
    }

    async fn list_cameras(
        &self,
        _request: Request<ListCamerasRequest>,
    ) -> Result<Response<ListCamerasResponse>, Status> {
        let reg = self.registry.lock().unwrap();
        let cameras = reg
            .list()
            .into_iter()
            .map(|cam| CameraInfo {
                camera_id: cam.id,
                name: cam.name.clone(),
                lat: cam.lat,
                lon: cam.lon,
                heading_deg: cam.heading_deg,
                fov_deg: cam.fov_deg,
                range_m: cam.range_m,
                covered_edge_ids: cam.covered_edges.clone(),
            })
            .collect();

        Ok(Response::new(ListCamerasResponse { cameras }))
    }
}
