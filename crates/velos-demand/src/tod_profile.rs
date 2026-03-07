//! Time-of-day demand scaling profiles with piecewise-linear interpolation.

use crate::od_matrix::NamedZone;
use crate::od_matrix::Zone;

/// A time-of-day profile defined by (hour, factor) control points.
///
/// Interpolates linearly between points. Before the first point, uses first
/// point's factor. After the last point, uses last point's factor.
#[derive(Debug, Clone)]
pub struct TodProfile {
    /// Sorted (hour, factor) control points. Must have at least one point.
    points: Vec<(f64, f64)>,
}

impl TodProfile {
    /// Create a profile from (hour, factor) control points.
    ///
    /// Points are sorted by hour on construction.
    ///
    /// # Panics
    /// Panics if `points` is empty.
    pub fn new(mut points: Vec<(f64, f64)>) -> Self {
        assert!(!points.is_empty(), "TodProfile requires at least one point");
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        Self { points }
    }

    /// Factory: HCMC weekday demand profile with AM/PM peaks.
    ///
    /// Based on HCMC traffic survey data:
    /// - AM peak: 07:00-08:00 (factor 1.0)
    /// - PM peak: 17:00-18:00 (factor 1.0)
    /// - Midday plateau: 12:00 (factor 0.7)
    /// - Late night trough: 00:00-05:00 (factor 0.05-0.10)
    pub fn hcmc_weekday() -> Self {
        Self::new(vec![
            (0.0, 0.05),
            (5.0, 0.10),
            (6.0, 0.40),
            (7.0, 1.00),
            (8.0, 1.00),
            (9.0, 0.50),
            (12.0, 0.70),
            (13.0, 0.50),
            (17.0, 1.00),
            (18.0, 1.00),
            (19.0, 0.50),
            (22.0, 0.10),
        ])
    }

    /// Factory: 5-district HCMC weekday demand profiles.
    ///
    /// Each district has a distinct demand curve:
    /// - District 1 (CBD): sharp AM/PM peaks (commuter-dominated)
    /// - District 3: moderate peaks (mixed commercial/residential)
    /// - District 5 (Cholon): broader morning peak (market activity)
    /// - District 10: broad gentle peaks (residential/university area)
    /// - Binh Thanh: asymmetric (strong AM outflow, moderate PM return)
    ///
    /// Factors represent scaling multipliers on the base OD matrix rates.
    pub fn hcmc_5district_weekday() -> Vec<(NamedZone, Self)> {
        vec![
            (
                NamedZone {
                    zone: Zone::District1,
                    name: "District 1".to_string(),
                },
                // CBD: sharp peaks, deep midday valley.
                Self::new(vec![
                    (0.0, 0.05),
                    (5.0, 0.10),
                    (6.0, 0.50),
                    (7.0, 2.00),
                    (8.0, 2.00),
                    (9.0, 0.80),
                    (12.0, 0.60),
                    (13.0, 0.50),
                    (16.0, 0.70),
                    (17.0, 2.00),
                    (18.0, 1.80),
                    (19.0, 0.60),
                    (22.0, 0.10),
                ]),
            ),
            (
                NamedZone {
                    zone: Zone::District3,
                    name: "District 3".to_string(),
                },
                // Mixed commercial/residential: moderate peaks.
                Self::new(vec![
                    (0.0, 0.05),
                    (5.0, 0.10),
                    (6.0, 0.40),
                    (7.0, 1.60),
                    (8.0, 1.50),
                    (9.0, 0.70),
                    (12.0, 0.60),
                    (13.0, 0.50),
                    (16.0, 0.60),
                    (17.0, 1.50),
                    (18.0, 1.40),
                    (19.0, 0.50),
                    (22.0, 0.10),
                ]),
            ),
            (
                NamedZone {
                    zone: Zone::District5,
                    name: "District 5".to_string(),
                },
                // Cholon (market district): early morning activity, broader AM peak.
                Self::new(vec![
                    (0.0, 0.05),
                    (4.0, 0.15),
                    (5.0, 0.30),
                    (6.0, 0.80),
                    (7.0, 1.50),
                    (8.0, 1.40),
                    (9.0, 0.80),
                    (12.0, 0.60),
                    (13.0, 0.50),
                    (16.0, 0.60),
                    (17.0, 1.30),
                    (18.0, 1.20),
                    (19.0, 0.50),
                    (22.0, 0.10),
                ]),
            ),
            (
                NamedZone {
                    zone: Zone::District10,
                    name: "District 10".to_string(),
                },
                // Residential/university: broad gentle peaks.
                Self::new(vec![
                    (0.0, 0.05),
                    (5.0, 0.10),
                    (6.0, 0.40),
                    (7.0, 1.30),
                    (8.0, 1.20),
                    (9.0, 0.60),
                    (12.0, 0.55),
                    (13.0, 0.45),
                    (16.0, 0.55),
                    (17.0, 1.20),
                    (18.0, 1.10),
                    (19.0, 0.50),
                    (22.0, 0.10),
                ]),
            ),
            (
                NamedZone {
                    zone: Zone::BinhThanh,
                    name: "Binh Thanh".to_string(),
                },
                // Residential with commuter outflow: strong AM, moderate PM.
                Self::new(vec![
                    (0.0, 0.05),
                    (5.0, 0.10),
                    (6.0, 0.50),
                    (7.0, 1.70),
                    (8.0, 1.50),
                    (9.0, 0.60),
                    (12.0, 0.55),
                    (13.0, 0.45),
                    (16.0, 0.55),
                    (17.0, 1.40),
                    (18.0, 1.30),
                    (19.0, 0.50),
                    (22.0, 0.10),
                ]),
            ),
        ]
    }

    /// Factory: 5-district HCMC weekend demand profiles.
    ///
    /// Weekend profiles are uniformly lower (~0.6x weekday peak) with
    /// flatter, later peaks reflecting leisure/shopping patterns.
    pub fn hcmc_5district_weekend() -> Vec<(NamedZone, Self)> {
        vec![
            (
                NamedZone {
                    zone: Zone::District1,
                    name: "District 1".to_string(),
                },
                Self::new(vec![
                    (0.0, 0.05),
                    (6.0, 0.10),
                    (8.0, 0.40),
                    (10.0, 1.00),
                    (12.0, 0.90),
                    (14.0, 0.80),
                    (17.0, 1.00),
                    (19.0, 0.70),
                    (22.0, 0.10),
                ]),
            ),
            (
                NamedZone {
                    zone: Zone::District3,
                    name: "District 3".to_string(),
                },
                Self::new(vec![
                    (0.0, 0.05),
                    (6.0, 0.10),
                    (8.0, 0.35),
                    (10.0, 0.90),
                    (12.0, 0.80),
                    (14.0, 0.70),
                    (17.0, 0.85),
                    (19.0, 0.60),
                    (22.0, 0.10),
                ]),
            ),
            (
                NamedZone {
                    zone: Zone::District5,
                    name: "District 5".to_string(),
                },
                Self::new(vec![
                    (0.0, 0.05),
                    (5.0, 0.15),
                    (7.0, 0.50),
                    (9.0, 1.00),
                    (12.0, 0.85),
                    (14.0, 0.70),
                    (17.0, 0.80),
                    (19.0, 0.50),
                    (22.0, 0.10),
                ]),
            ),
            (
                NamedZone {
                    zone: Zone::District10,
                    name: "District 10".to_string(),
                },
                Self::new(vec![
                    (0.0, 0.05),
                    (6.0, 0.10),
                    (8.0, 0.30),
                    (10.0, 0.80),
                    (12.0, 0.70),
                    (14.0, 0.65),
                    (17.0, 0.75),
                    (19.0, 0.50),
                    (22.0, 0.10),
                ]),
            ),
            (
                NamedZone {
                    zone: Zone::BinhThanh,
                    name: "Binh Thanh".to_string(),
                },
                Self::new(vec![
                    (0.0, 0.05),
                    (6.0, 0.10),
                    (8.0, 0.35),
                    (10.0, 0.85),
                    (12.0, 0.75),
                    (14.0, 0.65),
                    (17.0, 0.80),
                    (19.0, 0.55),
                    (22.0, 0.10),
                ]),
            ),
        ]
    }

    /// Get the demand scaling factor at the given hour via linear interpolation.
    ///
    /// - Before the first control point: returns first point's factor.
    /// - After the last control point: returns last point's factor.
    /// - Between two points: linearly interpolates.
    /// - Exactly on a point: returns that point's factor.
    pub fn factor_at(&self, hour: f64) -> f64 {
        let first = self.points.first().unwrap();
        let last = self.points.last().unwrap();

        if hour <= first.0 {
            return first.1;
        }
        if hour >= last.0 {
            return last.1;
        }

        // Find the bracketing points via binary search.
        // We want the rightmost point with hour <= target.
        let idx = self
            .points
            .partition_point(|&(h, _)| h <= hour)
            .saturating_sub(1);

        let (h0, f0) = self.points[idx];
        let (h1, f1) = self.points[idx + 1];

        let t = (hour - h0) / (h1 - h0);
        f0 + t * (f1 - f0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_point_returns_exact_factor() {
        let tod = TodProfile::new(vec![(0.0, 0.0), (5.0, 0.5), (10.0, 1.0)]);
        assert!((tod.factor_at(5.0) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn quarter_interpolation() {
        let tod = TodProfile::new(vec![(0.0, 0.0), (10.0, 1.0)]);
        assert!((tod.factor_at(2.5) - 0.25).abs() < 0.001);
    }

    #[test]
    #[should_panic(expected = "at least one point")]
    fn empty_points_panics() {
        TodProfile::new(vec![]);
    }
}
