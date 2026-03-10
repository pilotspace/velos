//! Async-sync bridge channel types for gRPC-to-SimWorld communication.
//!
//! The gRPC server runs on a tokio runtime in a background thread. Commands flow
//! from gRPC handlers (async) to the simulation frame loop (sync via `try_recv`).
//! Request-response patterns use oneshot channels embedded in the command variant.

use tokio::sync::{mpsc, oneshot};

use crate::proto::velos::v2::{DetectionBatch, RegisterCameraRequest, RegisterCameraResponse};

/// Default bounded channel capacity. Large enough to absorb detection bursts
/// without unbounded growth; small enough to apply backpressure when the
/// simulation cannot keep up.
pub const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Commands sent from gRPC handlers to the simulation world.
#[derive(Debug)]
pub enum ApiCommand {
    /// Register a new camera. The oneshot sender receives the response with
    /// assigned camera_id and covered edge IDs.
    RegisterCamera {
        /// The registration request from the gRPC client.
        request: RegisterCameraRequest,
        /// Oneshot channel for the response back to the gRPC handler.
        reply: oneshot::Sender<RegisterCameraResponse>,
    },

    /// A batch of detection events to ingest. Fire-and-forget; the gRPC handler
    /// sends an ack based on validation, not on simulation processing.
    DetectionBatch {
        /// The detection batch from the gRPC client.
        batch: DetectionBatch,
    },
}

/// Async-sync bridge between the gRPC server and the simulation world.
///
/// The simulation frame loop owns this struct and calls `try_recv()` each frame
/// to drain pending commands without blocking.
pub struct ApiBridge {
    /// Receiver end -- owned by the simulation frame loop.
    cmd_rx: mpsc::Receiver<ApiCommand>,
}

impl ApiBridge {
    /// Create a new bridge with the given channel capacity.
    ///
    /// Returns `(bridge, sender)` where:
    /// - `bridge` is owned by the simulation frame loop
    /// - `sender` is cloned into each gRPC service handler
    pub fn new(capacity: usize) -> (Self, mpsc::Sender<ApiCommand>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { cmd_rx: rx }, tx)
    }

    /// Non-blocking receive. Returns `None` if the channel is empty or all
    /// senders have been dropped.
    pub fn try_recv(&mut self) -> Option<ApiCommand> {
        self.cmd_rx.try_recv().ok()
    }

    /// Drain up to `budget` commands from the channel. Returns the commands
    /// received. Use this in the frame loop to cap per-frame processing.
    pub fn drain(&mut self, budget: usize) -> Vec<ApiCommand> {
        let mut commands = Vec::with_capacity(budget.min(64));
        for _ in 0..budget {
            match self.cmd_rx.try_recv() {
                Ok(cmd) => commands.push(cmd),
                Err(_) => break,
            }
        }
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bridge_send_recv_round_trip() {
        let (mut bridge, tx) = ApiBridge::new(16);

        let batch = DetectionBatch {
            batch_id: 1,
            events: vec![],
        };
        tx.send(ApiCommand::DetectionBatch { batch })
            .await
            .expect("send should succeed");

        let cmd = bridge.try_recv().expect("should receive command");
        match cmd {
            ApiCommand::DetectionBatch { batch } => {
                assert_eq!(batch.batch_id, 1);
            }
            _ => panic!("expected DetectionBatch"),
        }
    }

    #[tokio::test]
    async fn bridge_empty_returns_none() {
        let (mut bridge, _tx) = ApiBridge::new(16);
        assert!(bridge.try_recv().is_none());
    }

    #[tokio::test]
    async fn bridge_register_camera_with_oneshot() {
        let (mut bridge, tx) = ApiBridge::new(16);

        let (reply_tx, reply_rx) = oneshot::channel();
        let request = RegisterCameraRequest {
            lat: 10.775,
            lon: 106.700,
            heading_deg: 90.0,
            fov_deg: 60.0,
            range_m: 50.0,
            name: "test-cam".into(),
        };

        tx.send(ApiCommand::RegisterCamera {
            request,
            reply: reply_tx,
        })
        .await
        .expect("send should succeed");

        let cmd = bridge.try_recv().expect("should receive command");
        match cmd {
            ApiCommand::RegisterCamera { request, reply } => {
                assert_eq!(request.name, "test-cam");
                let response = RegisterCameraResponse {
                    camera_id: 1,
                    covered_edge_ids: vec![10, 20, 30],
                };
                reply.send(response).expect("reply should succeed");
            }
            _ => panic!("expected RegisterCamera"),
        }

        let response = reply_rx.await.expect("should receive response");
        assert_eq!(response.camera_id, 1);
        assert_eq!(response.covered_edge_ids, vec![10, 20, 30]);
    }

    #[tokio::test]
    async fn bridge_full_channel_applies_backpressure() {
        let (_bridge, tx) = ApiBridge::new(2);

        // Fill the channel.
        for i in 0..2 {
            tx.send(ApiCommand::DetectionBatch {
                batch: DetectionBatch {
                    batch_id: i,
                    events: vec![],
                },
            })
            .await
            .expect("send should succeed");
        }

        // Third send should not complete immediately (channel full).
        let result = tx.try_send(ApiCommand::DetectionBatch {
            batch: DetectionBatch {
                batch_id: 99,
                events: vec![],
            },
        });
        assert!(result.is_err(), "channel should be full");
    }

    #[tokio::test]
    async fn bridge_drain_respects_budget() {
        let (mut bridge, tx) = ApiBridge::new(16);

        for i in 0..5 {
            tx.send(ApiCommand::DetectionBatch {
                batch: DetectionBatch {
                    batch_id: i,
                    events: vec![],
                },
            })
            .await
            .expect("send should succeed");
        }

        let drained = bridge.drain(3);
        assert_eq!(drained.len(), 3);

        // Remaining 2 should still be in the channel.
        let remaining = bridge.drain(10);
        assert_eq!(remaining.len(), 2);
    }
}
