//! Historical pattern matcher with time-of-day and day-type buckets.
//!
//! Stores recorded travel times indexed by `[edge_idx * 96 + hour * 4 + day_type]`.
//! Day types: 0=weekday, 1=saturday, 2=sunday, 3=holiday.
//! Returns recorded values when available, free-flow fallback otherwise.

/// Number of day-type buckets.
const DAY_TYPES: usize = 4;
/// Number of hours in a day.
const HOURS: usize = 24;
/// Slots per edge: 24 hours * 4 day_types = 96.
const SLOTS_PER_EDGE: usize = HOURS * DAY_TYPES;

/// Historical travel time pattern matcher.
///
/// Maintains a flat lookup table indexed by edge, hour, and day type.
/// Missing entries (value == 0.0) fall back to free-flow travel time.
#[derive(Debug, Clone)]
pub struct HistoricalMatcher {
    /// Flat Vec indexed by `[edge_idx * 96 + hour * 4 + day_type]`.
    data: Vec<f32>,
    edge_count: usize,
}

impl HistoricalMatcher {
    /// Create a new matcher with all entries initialized to 0.0 (no data).
    pub fn new(edge_count: usize) -> Self {
        Self {
            data: vec![0.0; edge_count * SLOTS_PER_EDGE],
            edge_count,
        }
    }

    /// Record a travel time observation for a specific edge, hour, and day type.
    ///
    /// Overwrites any previous value. Hour must be 0..23, day_type 0..3.
    pub fn record(&mut self, edge_idx: usize, hour: u8, day_type: u8, travel_time: f32) {
        debug_assert!(edge_idx < self.edge_count);
        debug_assert!((hour as usize) < HOURS);
        debug_assert!((day_type as usize) < DAY_TYPES);
        let idx = edge_idx * SLOTS_PER_EDGE + (hour as usize) * DAY_TYPES + day_type as usize;
        self.data[idx] = travel_time;
    }

    /// Predict travel times for all edges at the given hour and day type.
    ///
    /// Returns recorded value if > 0.0, otherwise falls back to free-flow time.
    pub fn predict(&self, hour: u8, day_type: u8, free_flow: &[f32]) -> Vec<f32> {
        debug_assert_eq!(free_flow.len(), self.edge_count);
        debug_assert!((hour as usize) < HOURS);
        debug_assert!((day_type as usize) < DAY_TYPES);

        (0..self.edge_count)
            .map(|edge_idx| {
                let idx =
                    edge_idx * SLOTS_PER_EDGE + (hour as usize) * DAY_TYPES + day_type as usize;
                let recorded = self.data[idx];
                if recorded > 0.0 {
                    recorded
                } else {
                    free_flow[edge_idx]
                }
            })
            .collect()
    }

    /// Number of edges tracked.
    pub fn edge_count(&self) -> usize {
        self.edge_count
    }
}
