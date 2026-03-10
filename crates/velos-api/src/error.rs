//! API error types with tonic::Status mapping.

/// Errors returned by the detection ingestion API.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// Camera ID not found in the registry.
    #[error("unknown camera: {0}")]
    UnknownCamera(u32),

    /// Malformed detection event data.
    #[error("invalid data: {0}")]
    InvalidData(String),

    /// Bridge channel is at capacity; backpressure applied.
    #[error("channel full: detection pipeline is overloaded")]
    ChannelFull,

    /// Simulation side has disconnected from the bridge channel.
    #[error("channel closed: simulation disconnected")]
    ChannelClosed,
}

impl From<ApiError> for tonic::Status {
    fn from(err: ApiError) -> Self {
        match err {
            ApiError::UnknownCamera(id) => {
                tonic::Status::not_found(format!("unknown camera: {id}"))
            }
            ApiError::InvalidData(msg) => tonic::Status::invalid_argument(msg),
            ApiError::ChannelFull => {
                tonic::Status::unavailable("detection pipeline is overloaded")
            }
            ApiError::ChannelClosed => {
                tonic::Status::unavailable("simulation disconnected")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_camera_maps_to_not_found() {
        let status: tonic::Status = ApiError::UnknownCamera(42).into();
        assert_eq!(status.code(), tonic::Code::NotFound);
        assert!(status.message().contains("42"));
    }

    #[test]
    fn invalid_data_maps_to_invalid_argument() {
        let status: tonic::Status = ApiError::InvalidData("bad field".into()).into();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(status.message().contains("bad field"));
    }

    #[test]
    fn channel_full_maps_to_unavailable() {
        let status: tonic::Status = ApiError::ChannelFull.into();
        assert_eq!(status.code(), tonic::Code::Unavailable);
    }

    #[test]
    fn channel_closed_maps_to_unavailable() {
        let status: tonic::Status = ApiError::ChannelClosed.into();
        assert_eq!(status.code(), tonic::Code::Unavailable);
        assert!(status.message().contains("disconnected"));
    }
}
