//! CFL (Courant-Friedrichs-Lewy) numerical stability check.
//!
//! Validates that a simulation timestep `dt` does not allow an agent
//! to travel more than one cell in a single step. Uses f64 throughout.

/// Returns true if the CFL condition holds: `max_speed * dt / min_cell_size < 1.0`.
///
/// Call this before each simulation step. An assertion on the return value
/// is the standard usage pattern:
///
/// ```
/// use velos_core::cfl::cfl_check;
/// assert!(cfl_check(0.1, 33.3, 50.0), "CFL violation");
/// ```
pub fn cfl_check(dt: f64, max_speed: f64, min_cell_size: f64) -> bool {
    if min_cell_size <= 0.0 {
        return false;
    }
    (max_speed * dt) / min_cell_size < 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cfl_check_valid() {
        // 33.3 m/s * 0.1s = 3.33m < 50m cell
        assert!(cfl_check(0.1, 33.3, 50.0));
    }

    #[test]
    fn test_cfl_check_violation() {
        // 33.3 m/s * 1.0s = 33.3m > 10m cell
        assert!(!cfl_check(1.0, 33.3, 10.0));
    }

    #[test]
    fn test_cfl_check_zero_dt() {
        assert!(cfl_check(0.0, 33.3, 50.0));
    }

    #[test]
    fn test_cfl_check_zero_speed() {
        assert!(cfl_check(0.1, 0.0, 50.0));
    }

    #[test]
    fn test_cfl_check_invalid_cell_size() {
        assert!(!cfl_check(0.1, 33.3, 0.0));
    }

    #[test]
    fn test_cfl_check_boundary() {
        // Exactly at boundary (= 1.0) is a violation
        assert!(!cfl_check(1.0, 50.0, 50.0));
    }
}
