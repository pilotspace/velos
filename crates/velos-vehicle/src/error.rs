//! Error types for the velos-vehicle crate.

use thiserror::Error;

/// Errors that can occur in vehicle model computations.
#[derive(Debug, Error)]
pub enum VehicleError {
    /// Invalid parameter value.
    #[error("invalid parameter `{name}`: {reason}")]
    InvalidParam {
        /// Parameter name.
        name: &'static str,
        /// Why it is invalid.
        reason: String,
    },
}
