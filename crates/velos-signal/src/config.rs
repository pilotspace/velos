//! Signal configuration loading from TOML files.
//!
//! Provides [`SignalConfig`] for intersection-level signal controller
//! configuration, and [`load_signal_config`] for loading from disk
//! with graceful fallback to defaults on missing/invalid files.

use serde::Deserialize;

/// Top-level signal configuration loaded from TOML.
///
/// Contains a list of intersection configurations that override
/// the default fixed-time controller assignment. Intersections not
/// listed here default to `FixedTimeController`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SignalConfig {
    /// Per-intersection controller overrides.
    #[serde(default)]
    pub intersection: Vec<IntersectionConfig>,
}

/// Configuration for a single intersection's signal controller.
#[derive(Debug, Clone, Deserialize)]
pub struct IntersectionConfig {
    /// Road graph node ID for this intersection.
    pub node_id: u32,
    /// Controller type: "fixed", "actuated", or "adaptive".
    #[serde(default = "default_controller")]
    pub controller: String,
    /// Minimum green time (seconds). Actuated/adaptive only.
    #[serde(default = "default_min_green")]
    pub min_green: f64,
    /// Maximum green time (seconds). Actuated only.
    #[serde(default = "default_max_green")]
    pub max_green: f64,
    /// Gap-out threshold (seconds). Actuated only.
    #[serde(default = "default_gap_threshold")]
    pub gap_threshold: f64,
}

fn default_controller() -> String {
    "fixed".to_string()
}

fn default_min_green() -> f64 {
    7.0
}

fn default_max_green() -> f64 {
    60.0
}

fn default_gap_threshold() -> f64 {
    3.0
}

/// Default TOML config file path for signal configuration.
const DEFAULT_CONFIG_PATH: &str = "data/hcmc/signal_config.toml";

/// Environment variable to override the signal config file path.
const ENV_CONFIG_PATH: &str = "VELOS_SIGNAL_CONFIG";

/// Load signal configuration from disk.
///
/// Reads from the path specified by `VELOS_SIGNAL_CONFIG` env var,
/// falling back to `data/hcmc/signal_config.toml`. On missing file
/// or parse error, logs a warning and returns an empty config
/// (all intersections default to fixed-time).
pub fn load_signal_config() -> SignalConfig {
    let path = std::env::var(ENV_CONFIG_PATH).unwrap_or_else(|_| DEFAULT_CONFIG_PATH.to_string());

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!(
                "Signal config not found at '{}': {}. Using defaults (all fixed-time).",
                path,
                e
            );
            return SignalConfig::default();
        }
    };

    match toml::from_str::<SignalConfig>(&content) {
        Ok(config) => {
            log::info!(
                "Loaded signal config: {} intersection overrides from '{}'",
                config.intersection.len(),
                path
            );
            config
        }
        Err(e) => {
            log::warn!(
                "Failed to parse signal config '{}': {}. Using defaults.",
                path,
                e
            );
            SignalConfig::default()
        }
    }
}

/// Parse signal configuration from a TOML string (for tests).
pub fn load_signal_config_from_str(toml_str: &str) -> Result<SignalConfig, String> {
    toml::from_str(toml_str).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_file_returns_empty_config() {
        // Set env to a nonexistent path
        // Safety: test is single-threaded; no other thread reads this env var.
        unsafe {
            std::env::set_var("VELOS_SIGNAL_CONFIG", "/tmp/nonexistent_signal_config.toml");
        }
        let config = load_signal_config();
        assert!(config.intersection.is_empty());
        unsafe {
            std::env::remove_var("VELOS_SIGNAL_CONFIG");
        }
    }

    #[test]
    fn parse_valid_toml() {
        let toml = r#"
[[intersection]]
node_id = 42
controller = "actuated"
min_green = 10.0
max_green = 45.0
gap_threshold = 2.5

[[intersection]]
node_id = 99
controller = "adaptive"
"#;
        let config = load_signal_config_from_str(toml).unwrap();
        assert_eq!(config.intersection.len(), 2);

        let first = &config.intersection[0];
        assert_eq!(first.node_id, 42);
        assert_eq!(first.controller, "actuated");
        assert!((first.min_green - 10.0).abs() < f64::EPSILON);
        assert!((first.max_green - 45.0).abs() < f64::EPSILON);
        assert!((first.gap_threshold - 2.5).abs() < f64::EPSILON);

        let second = &config.intersection[1];
        assert_eq!(second.node_id, 99);
        assert_eq!(second.controller, "adaptive");
        // Check defaults
        assert!((second.min_green - 7.0).abs() < f64::EPSILON);
        assert!((second.max_green - 60.0).abs() < f64::EPSILON);
        assert!((second.gap_threshold - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_empty_toml_returns_empty_config() {
        let config = load_signal_config_from_str("").unwrap();
        assert!(config.intersection.is_empty());
    }

    #[test]
    fn parse_invalid_toml_returns_error() {
        let result = load_signal_config_from_str("{{invalid");
        assert!(result.is_err());
    }

    #[test]
    fn default_values_applied() {
        let toml = r#"
[[intersection]]
node_id = 1
"#;
        let config = load_signal_config_from_str(toml).unwrap();
        let ic = &config.intersection[0];
        assert_eq!(ic.controller, "fixed");
        assert!((ic.min_green - 7.0).abs() < f64::EPSILON);
        assert!((ic.max_green - 60.0).abs() < f64::EPSILON);
        assert!((ic.gap_threshold - 3.0).abs() < f64::EPSILON);
    }
}
