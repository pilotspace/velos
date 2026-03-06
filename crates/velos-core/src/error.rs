//! Error types for velos-core.

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("CFL violation: dt={dt}, max_speed={max_speed}, min_cell_size={min_cell_size}")]
    CflViolation {
        dt: f64,
        max_speed: f64,
        min_cell_size: f64,
    },
}
