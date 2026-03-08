//! Bus spawn generation driven by GTFS trip departure schedules.
//!
//! `BusSpawner` maps GTFS routes to simulation bus stops and generates
//! `BusSpawnRequest`s time-gated by trip departure times. Each request
//! contains the `stop_indices` into the global `bus_stops` Vec so the
//! simulation can construct a `BusState` for the spawned agent.

use std::collections::HashMap;

use crate::gtfs::BusSchedule;

/// A request to spawn a bus agent at a specific simulation time.
#[derive(Debug, Clone, PartialEq)]
pub struct BusSpawnRequest {
    /// GTFS route identifier.
    pub route_id: String,
    /// GTFS trip identifier.
    pub trip_id: String,
    /// Ordered stop indices into the global `bus_stops` Vec.
    pub stop_indices: Vec<usize>,
}

/// Time-gated bus spawn generator driven by GTFS schedules.
///
/// Sorts trips by first-stop departure time and advances a cursor to emit
/// spawn requests as simulation time progresses. Each trip is spawned
/// exactly once.
pub struct BusSpawner {
    /// Per-route ordered stop indices into the global bus_stops Vec.
    route_stops: HashMap<String, Vec<usize>>,
    /// Schedules sorted by first stop departure time (seconds from midnight).
    schedules: Vec<BusSchedule>,
    /// Index of the next unspawned trip in `schedules`.
    next_trip_index: usize,
}

impl BusSpawner {
    /// Create a new `BusSpawner`.
    ///
    /// # Arguments
    /// * `route_stop_ids` - For each route, the ordered list of stop_ids along the route.
    /// * `stop_id_to_index` - Maps GTFS stop_id to index in the global `bus_stops` Vec.
    /// * `schedules` - GTFS trip schedules to be sorted by departure time.
    ///
    /// The `stop_id_to_index` map is built during GTFS stop snapping and decouples
    /// the spawner from the snapping logic.
    pub fn new(
        route_stop_ids: &HashMap<String, Vec<String>>,
        stop_id_to_index: &HashMap<String, usize>,
        mut schedules: Vec<BusSchedule>,
    ) -> Self {
        // Build route_stops by resolving stop_ids to bus_stop indices
        let mut route_stops = HashMap::new();
        for (route_id, stop_ids) in route_stop_ids {
            let indices: Vec<usize> = stop_ids
                .iter()
                .filter_map(|sid| stop_id_to_index.get(sid).copied())
                .collect();
            route_stops.insert(route_id.clone(), indices);
        }

        // Sort schedules by first stop departure time
        schedules.sort_by_key(|s| {
            s.stop_times
                .first()
                .map(|st| st.departure_s)
                .unwrap_or(u32::MAX)
        });

        Self {
            route_stops,
            schedules,
            next_trip_index: 0,
        }
    }

    /// Generate bus spawn requests for all trips whose first departure is at or
    /// before the current simulation time.
    ///
    /// `sim_time_s` is seconds from simulation start. Converted to
    /// seconds-of-day via `% 86400` for matching GTFS departure times.
    ///
    /// Each trip is emitted exactly once; the internal cursor advances past
    /// spawned trips.
    pub fn generate_bus_spawns(&mut self, sim_time_s: f64) -> Vec<BusSpawnRequest> {
        let sim_time_sod = (sim_time_s % 86400.0) as u32;
        let mut requests = Vec::new();

        while self.next_trip_index < self.schedules.len() {
            let schedule = &self.schedules[self.next_trip_index];
            let first_departure = schedule
                .stop_times
                .first()
                .map(|st| st.departure_s)
                .unwrap_or(u32::MAX);

            if first_departure > sim_time_sod {
                break;
            }

            let stop_indices = self
                .route_stops
                .get(&schedule.route_id)
                .cloned()
                .unwrap_or_default();

            requests.push(BusSpawnRequest {
                route_id: schedule.route_id.clone(),
                trip_id: schedule.trip_id.clone(),
                stop_indices,
            });

            self.next_trip_index += 1;
        }

        requests
    }

    /// Number of remaining unspawned trips.
    pub fn remaining_trips(&self) -> usize {
        self.schedules.len() - self.next_trip_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtfs::{BusSchedule, StopTime};

    fn make_schedule(trip_id: &str, route_id: &str, departure_s: u32) -> BusSchedule {
        BusSchedule {
            trip_id: trip_id.to_string(),
            route_id: route_id.to_string(),
            stop_times: vec![
                StopTime {
                    stop_id: "S1".to_string(),
                    arrival_s: departure_s,
                    departure_s,
                    stop_sequence: 1,
                },
                StopTime {
                    stop_id: "S2".to_string(),
                    arrival_s: departure_s + 300,
                    departure_s: departure_s + 300,
                    stop_sequence: 2,
                },
                StopTime {
                    stop_id: "S3".to_string(),
                    arrival_s: departure_s + 600,
                    departure_s: departure_s + 600,
                    stop_sequence: 3,
                },
            ],
        }
    }

    fn make_route_stop_ids() -> HashMap<String, Vec<String>> {
        let mut m = HashMap::new();
        m.insert(
            "R1".to_string(),
            vec!["S1".to_string(), "S2".to_string(), "S3".to_string()],
        );
        m
    }

    fn make_stop_id_to_index() -> HashMap<String, usize> {
        let mut m = HashMap::new();
        m.insert("S1".to_string(), 0);
        m.insert("S2".to_string(), 1);
        m.insert("S3".to_string(), 2);
        m
    }

    #[test]
    fn new_builds_route_stops_mapping() {
        let route_stop_ids = make_route_stop_ids();
        let stop_id_to_index = make_stop_id_to_index();
        let schedules = vec![make_schedule("T1", "R1", 21600)];

        let spawner = BusSpawner::new(&route_stop_ids, &stop_id_to_index, schedules);
        assert_eq!(
            spawner.route_stops.get("R1").unwrap(),
            &vec![0, 1, 2],
            "route R1 should map to stop indices [0, 1, 2]"
        );
    }

    #[test]
    fn generate_before_any_departure_returns_empty() {
        let route_stop_ids = make_route_stop_ids();
        let stop_id_to_index = make_stop_id_to_index();
        // Trip departs at 06:00 (21600s)
        let schedules = vec![make_schedule("T1", "R1", 21600)];

        let mut spawner = BusSpawner::new(&route_stop_ids, &stop_id_to_index, schedules);

        // sim_time_s = 05:00 (18000s) -- before departure
        let requests = spawner.generate_bus_spawns(18000.0);
        assert!(requests.is_empty(), "no trips should spawn before departure");
    }

    #[test]
    fn generate_at_departure_returns_one_request() {
        let route_stop_ids = make_route_stop_ids();
        let stop_id_to_index = make_stop_id_to_index();
        let schedules = vec![make_schedule("T1", "R1", 21600)];

        let mut spawner = BusSpawner::new(&route_stop_ids, &stop_id_to_index, schedules);

        // At exactly 06:00
        let requests = spawner.generate_bus_spawns(21600.0);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].route_id, "R1");
        assert_eq!(requests[0].trip_id, "T1");
        assert_eq!(requests[0].stop_indices, vec![0, 1, 2]);
    }

    #[test]
    fn generate_advances_cursor_no_double_spawn() {
        let route_stop_ids = make_route_stop_ids();
        let stop_id_to_index = make_stop_id_to_index();
        let schedules = vec![make_schedule("T1", "R1", 21600)];

        let mut spawner = BusSpawner::new(&route_stop_ids, &stop_id_to_index, schedules);

        // First call at 06:00 -- spawns T1
        let r1 = spawner.generate_bus_spawns(21600.0);
        assert_eq!(r1.len(), 1);

        // Second call at same time -- should be empty (already spawned)
        let r2 = spawner.generate_bus_spawns(21600.0);
        assert!(r2.is_empty(), "same trip must not spawn twice");
    }

    #[test]
    fn generate_multiple_trips_at_once() {
        let route_stop_ids = make_route_stop_ids();
        let stop_id_to_index = make_stop_id_to_index();
        let schedules = vec![
            make_schedule("T1", "R1", 21600), // 06:00
            make_schedule("T2", "R1", 22200), // 06:10
            make_schedule("T3", "R1", 25200), // 07:00
        ];

        let mut spawner = BusSpawner::new(&route_stop_ids, &stop_id_to_index, schedules);

        // At 06:15 (22500s) -- T1 and T2 should be ready, T3 not yet
        let requests = spawner.generate_bus_spawns(22500.0);
        assert_eq!(requests.len(), 2, "T1 and T2 should spawn");

        let trip_ids: Vec<&str> = requests.iter().map(|r| r.trip_id.as_str()).collect();
        assert!(trip_ids.contains(&"T1"));
        assert!(trip_ids.contains(&"T2"));

        // At 07:01 -- T3 should now spawn
        let requests2 = spawner.generate_bus_spawns(25260.0);
        assert_eq!(requests2.len(), 1);
        assert_eq!(requests2[0].trip_id, "T3");
    }

    #[test]
    fn spawn_request_has_correct_stop_indices() {
        let mut route_stop_ids = HashMap::new();
        route_stop_ids.insert(
            "R2".to_string(),
            vec!["SA".to_string(), "SB".to_string()],
        );

        let mut stop_id_to_index = HashMap::new();
        stop_id_to_index.insert("SA".to_string(), 5);
        stop_id_to_index.insert("SB".to_string(), 12);

        let schedules = vec![BusSchedule {
            trip_id: "TX".to_string(),
            route_id: "R2".to_string(),
            stop_times: vec![StopTime {
                stop_id: "SA".to_string(),
                arrival_s: 3600,
                departure_s: 3600,
                stop_sequence: 1,
            }],
        }];

        let mut spawner = BusSpawner::new(&route_stop_ids, &stop_id_to_index, schedules);
        let requests = spawner.generate_bus_spawns(3600.0);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].stop_indices, vec![5, 12]);
    }
}
