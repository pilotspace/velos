//! Windowed detection aggregation per camera per vehicle class.
//!
//! Ingests detection events into time-windowed per-class counts with speed
//! averaging. Old windows are garbage-collected beyond the retention period.

use std::collections::HashMap;

use crate::proto::velos::v2::DetectionEvent;

/// Default window duration: 5 minutes in milliseconds.
const DEFAULT_WINDOW_DURATION_MS: i64 = 300_000;

/// Default retention period: 1 hour in milliseconds.
const DEFAULT_RETENTION_MS: i64 = 3_600_000;

/// A time window holding per-class detection counts and speed samples.
#[derive(Debug, Clone)]
pub struct TimeWindow {
    /// Window start time (inclusive), Unix epoch milliseconds.
    pub start_ms: i64,
    /// Window end time (exclusive), Unix epoch milliseconds.
    pub end_ms: i64,
    /// Per-class vehicle counts accumulated in this window.
    pub counts: HashMap<i32, u32>,
    /// Per-class speed samples: (sum_of_speed_times_count, total_count).
    pub speed_samples: HashMap<i32, (f32, u32)>,
}

impl TimeWindow {
    /// Compute the mean speed for a vehicle class in this window.
    ///
    /// Returns `None` if no speed samples exist for the class.
    pub fn mean_speed(&self, class: i32) -> Option<f32> {
        self.speed_samples.get(&class).and_then(|&(sum, count)| {
            if count > 0 {
                Some(sum / count as f32)
            } else {
                None
            }
        })
    }
}

/// Windowed detection aggregator per camera.
///
/// Groups detection events into fixed-size time windows and tracks per-class
/// counts and speed samples. Supports garbage collection of old windows.
#[derive(Debug)]
pub struct DetectionAggregator {
    /// Duration of each window in milliseconds.
    pub window_duration_ms: i64,
    /// How long to retain windows before GC, in milliseconds.
    pub retention_ms: i64,
    /// Per-camera time windows, ordered by start time.
    cameras: HashMap<u32, Vec<TimeWindow>>,
}

impl DetectionAggregator {
    /// Create a new aggregator with custom window duration and retention.
    pub fn new(window_duration_ms: i64, retention_ms: i64) -> Self {
        Self {
            window_duration_ms,
            retention_ms,
            cameras: HashMap::new(),
        }
    }

    /// Ingest a detection event, creating or updating the appropriate window.
    pub fn ingest(&mut self, camera_id: u32, event: &DetectionEvent) {
        let windows = self.cameras.entry(camera_id).or_default();
        let window_start =
            (event.timestamp_ms / self.window_duration_ms) * self.window_duration_ms;

        // Find or create the window
        let window = match windows.iter_mut().find(|w| w.start_ms == window_start) {
            Some(w) => w,
            None => {
                windows.push(TimeWindow {
                    start_ms: window_start,
                    end_ms: window_start + self.window_duration_ms,
                    counts: HashMap::new(),
                    speed_samples: HashMap::new(),
                });
                windows.last_mut().unwrap()
            }
        };

        // Accumulate count
        *window.counts.entry(event.vehicle_class).or_insert(0) += event.count;

        // Accumulate speed sample if present
        if let Some(speed) = event.speed_kmh {
            let entry = window
                .speed_samples
                .entry(event.vehicle_class)
                .or_insert((0.0, 0));
            entry.0 += speed * event.count as f32;
            entry.1 += event.count;
        }
    }

    /// Remove windows older than the retention period.
    pub fn gc(&mut self, now_ms: i64) {
        let cutoff = now_ms - self.retention_ms;
        for windows in self.cameras.values_mut() {
            windows.retain(|w| w.end_ms > cutoff);
        }
    }

    /// Get the most recent window for a camera (by start_ms).
    pub fn latest_window(&self, camera_id: u32) -> Option<&TimeWindow> {
        self.cameras
            .get(&camera_id)
            .and_then(|windows| windows.iter().max_by_key(|w| w.start_ms))
    }

    /// Sum counts across all retained windows for a camera and vehicle class.
    pub fn total_count(&self, camera_id: u32, class: i32) -> u32 {
        self.cameras
            .get(&camera_id)
            .map(|windows| {
                windows
                    .iter()
                    .filter_map(|w| w.counts.get(&class))
                    .sum()
            })
            .unwrap_or(0)
    }
}

impl Default for DetectionAggregator {
    fn default() -> Self {
        Self::new(DEFAULT_WINDOW_DURATION_MS, DEFAULT_RETENTION_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::velos::v2::VehicleClass;

    fn make_event(
        camera_id: u32,
        timestamp_ms: i64,
        vehicle_class: i32,
        count: u32,
        speed_kmh: Option<f32>,
    ) -> DetectionEvent {
        DetectionEvent {
            camera_id,
            timestamp_ms,
            vehicle_class,
            count,
            speed_kmh,
        }
    }

    #[test]
    fn default_window_duration_and_retention() {
        let agg = DetectionAggregator::default();
        assert_eq!(agg.window_duration_ms, 300_000, "default 5 min windows");
        assert_eq!(agg.retention_ms, 3_600_000, "default 1 hr retention");
    }

    #[test]
    fn ingest_single_event_creates_window() {
        let mut agg = DetectionAggregator::new(300_000, 3_600_000);
        let event = make_event(1, 1_000_000, VehicleClass::Motorbike as i32, 3, None);
        agg.ingest(1, &event);

        let window = agg.latest_window(1).expect("should have a window");
        assert_eq!(window.start_ms, 900_000); // floor(1_000_000 / 300_000) * 300_000
        assert_eq!(window.end_ms, 1_200_000);
        assert_eq!(
            *window.counts.get(&(VehicleClass::Motorbike as i32)).unwrap(),
            3
        );
    }

    #[test]
    fn ingest_two_events_same_window_accumulates() {
        let mut agg = DetectionAggregator::new(300_000, 3_600_000);
        let class = VehicleClass::Car as i32;

        let e1 = make_event(1, 1_000_000, class, 5, None);
        let e2 = make_event(1, 1_100_000, class, 3, None);
        agg.ingest(1, &e1);
        agg.ingest(1, &e2);

        let window = agg.latest_window(1).unwrap();
        assert_eq!(*window.counts.get(&class).unwrap(), 8);
    }

    #[test]
    fn ingest_events_in_different_windows() {
        let mut agg = DetectionAggregator::new(300_000, 3_600_000);
        let class = VehicleClass::Motorbike as i32;

        // Window 1: 0..300_000
        let e1 = make_event(1, 100_000, class, 2, None);
        // Window 2: 300_000..600_000
        let e2 = make_event(1, 400_000, class, 4, None);

        agg.ingest(1, &e1);
        agg.ingest(1, &e2);

        // Total across both windows
        assert_eq!(agg.total_count(1, class), 6);

        // Latest window should be the second one
        let latest = agg.latest_window(1).unwrap();
        assert_eq!(latest.start_ms, 300_000);
    }

    #[test]
    fn speed_averaging_works() {
        let mut agg = DetectionAggregator::new(300_000, 3_600_000);
        let class = VehicleClass::Motorbike as i32;

        // 3 events with speeds 30, 40, 50, each count=1
        agg.ingest(1, &make_event(1, 100_000, class, 1, Some(30.0)));
        agg.ingest(1, &make_event(1, 100_001, class, 1, Some(40.0)));
        agg.ingest(1, &make_event(1, 100_002, class, 1, Some(50.0)));

        let window = agg.latest_window(1).unwrap();
        let mean = window.mean_speed(class).expect("should have speed samples");
        assert!(
            (mean - 40.0).abs() < 0.01,
            "mean speed should be 40.0, got {mean}"
        );
    }

    #[test]
    fn gc_removes_old_windows() {
        let mut agg = DetectionAggregator::new(300_000, 3_600_000);
        let class = VehicleClass::Car as i32;

        // Window at time 0..300_000
        agg.ingest(1, &make_event(1, 100_000, class, 1, None));

        // GC at time 4_000_000 (window end 300_000 <= 4_000_000 - 3_600_000 = 400_000)
        agg.gc(4_000_000);
        assert!(
            agg.latest_window(1).is_none(),
            "old window should be GC'd"
        );
    }

    #[test]
    fn gc_retains_recent_windows() {
        let mut agg = DetectionAggregator::new(300_000, 3_600_000);
        let class = VehicleClass::Car as i32;

        // Window at 3_600_000..3_900_000
        agg.ingest(1, &make_event(1, 3_700_000, class, 1, None));

        // GC at time 4_000_000 (window end 3_900_000 > 4_000_000 - 3_600_000 = 400_000)
        agg.gc(4_000_000);
        assert!(
            agg.latest_window(1).is_some(),
            "recent window should be retained"
        );
    }

    #[test]
    fn speed_with_weighted_counts() {
        let mut agg = DetectionAggregator::new(300_000, 3_600_000);
        let class = VehicleClass::Motorbike as i32;

        // Event: 2 vehicles at 30 km/h, then 1 vehicle at 60 km/h
        // Weighted mean = (2*30 + 1*60) / 3 = 40
        agg.ingest(1, &make_event(1, 100_000, class, 2, Some(30.0)));
        agg.ingest(1, &make_event(1, 100_001, class, 1, Some(60.0)));

        let window = agg.latest_window(1).unwrap();
        let mean = window.mean_speed(class).unwrap();
        assert!(
            (mean - 40.0).abs() < 0.01,
            "weighted mean should be 40.0, got {mean}"
        );
    }
}
