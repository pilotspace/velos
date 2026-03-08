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

    /// Failed to load config file from disk.
    #[error("failed to load config from `{path}`: {reason}")]
    ConfigLoad {
        /// File path attempted.
        path: String,
        /// IO error description.
        reason: String,
    },

    /// Failed to parse TOML config.
    #[error("config parse error: {0}")]
    ConfigParse(String),

    /// Config values out of valid range.
    #[error("config validation failed: {}", .0.join("; "))]
    ConfigValidation(Vec<String>),
}
