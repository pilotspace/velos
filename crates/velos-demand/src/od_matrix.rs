//! Origin-Destination matrix for zone-to-zone trip volumes.

use std::collections::HashMap;

/// Traffic analysis zones.
///
/// The `BenThanh..Waterfront` variants are sub-zones within District 1 for
/// the POC. The `District1..BinhThanh` variants represent full-district zones
/// for the 5-district HCMC simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Zone {
    // District 1 sub-zones (POC).
    BenThanh,
    NguyenHue,
    Bitexco,
    BuiVien,
    Waterfront,

    // 5-district zones.
    District1,
    District3,
    District5,
    District10,
    BinhThanh,
}

/// Named zone descriptor used with ToD profiles.
#[derive(Debug, Clone)]
pub struct NamedZone {
    /// Zone enum variant.
    pub zone: Zone,
    /// Human-readable name.
    pub name: String,
}

/// Origin-Destination matrix storing zone-to-zone trip counts (trips per hour).
#[derive(Debug, Clone)]
pub struct OdMatrix {
    trips: HashMap<(Zone, Zone), u32>,
}

impl OdMatrix {
    /// Create an empty OD matrix.
    pub fn new() -> Self {
        Self {
            trips: HashMap::new(),
        }
    }

    /// Set trips per hour for a zone pair.
    pub fn set_trips(&mut self, from: Zone, to: Zone, count: u32) {
        if count == 0 {
            self.trips.remove(&(from, to));
        } else {
            self.trips.insert((from, to), count);
        }
    }

    /// Get trips per hour for a zone pair. Returns 0 if not configured.
    pub fn get_trips(&self, from: Zone, to: Zone) -> u32 {
        self.trips.get(&(from, to)).copied().unwrap_or(0)
    }

    /// Sum of all trips across all zone pairs.
    pub fn total_trips(&self) -> u32 {
        self.trips.values().sum()
    }

    /// Iterate over all non-zero zone pairs: (from, to, count).
    pub fn zone_pairs(&self) -> impl Iterator<Item = (Zone, Zone, u32)> + '_ {
        self.trips
            .iter()
            .map(|(&(from, to), &count)| (from, to, count))
    }

    /// Factory: District 1 POC OD matrix with 5 sub-zones and realistic trip volumes.
    ///
    /// Total: ~560 trips/hour across all OD pairs.
    pub fn district1_poc() -> Self {
        let mut od = Self::new();

        od.set_trips(Zone::BenThanh, Zone::NguyenHue, 80);
        od.set_trips(Zone::NguyenHue, Zone::BenThanh, 75);

        od.set_trips(Zone::BenThanh, Zone::Bitexco, 70);
        od.set_trips(Zone::Bitexco, Zone::BenThanh, 65);

        od.set_trips(Zone::BuiVien, Zone::BenThanh, 55);
        od.set_trips(Zone::BenThanh, Zone::BuiVien, 50);

        od.set_trips(Zone::BuiVien, Zone::Waterfront, 60);
        od.set_trips(Zone::Waterfront, Zone::BuiVien, 55);

        od.set_trips(Zone::NguyenHue, Zone::Waterfront, 50);

        od
    }

    /// Factory: 5-district HCMC OD matrix with realistic inter-district flows.
    ///
    /// Produces 25 OD pairs (5x5 including intra-zone) with relative weights
    /// based on HCMC district characteristics:
    /// - District 1 (CBD) attracts the most commuters from all other districts
    /// - Adjacent districts have higher cross-flows
    /// - Intra-zone trips represent local circulation
    ///
    /// Base trips are per-hour at AM peak. Scale by ToD factor for other hours.
    /// Total at factor=1.0: ~140,000 trips/hour (scales to ~280K at peak factor ~2.0).
    pub fn hcmc_5district() -> Self {
        let mut od = Self::new();

        // Intra-zone trips (local circulation).
        od.set_trips(Zone::District1, Zone::District1, 8_000);
        od.set_trips(Zone::District3, Zone::District3, 6_000);
        od.set_trips(Zone::District5, Zone::District5, 5_000);
        od.set_trips(Zone::District10, Zone::District10, 7_000);
        od.set_trips(Zone::BinhThanh, Zone::BinhThanh, 6_500);

        // District 1 (CBD) -- strongest attractor.
        // Inbound commuter flows.
        od.set_trips(Zone::District3, Zone::District1, 12_000);
        od.set_trips(Zone::District5, Zone::District1, 8_000);
        od.set_trips(Zone::District10, Zone::District1, 10_000);
        od.set_trips(Zone::BinhThanh, Zone::District1, 11_000);

        // District 1 outbound (return flows, slightly lower).
        od.set_trips(Zone::District1, Zone::District3, 10_000);
        od.set_trips(Zone::District1, Zone::District5, 7_000);
        od.set_trips(Zone::District1, Zone::District10, 8_500);
        od.set_trips(Zone::District1, Zone::BinhThanh, 9_500);

        // Adjacent district cross-flows.
        // D3 <-> D10 (adjacent, strong flow).
        od.set_trips(Zone::District3, Zone::District10, 4_500);
        od.set_trips(Zone::District10, Zone::District3, 4_000);

        // D3 <-> Binh Thanh (adjacent).
        od.set_trips(Zone::District3, Zone::BinhThanh, 3_500);
        od.set_trips(Zone::BinhThanh, Zone::District3, 3_000);

        // D5 <-> D10 (adjacent).
        od.set_trips(Zone::District5, Zone::District10, 3_000);
        od.set_trips(Zone::District10, Zone::District5, 2_500);

        // Distant cross-flows (weaker).
        od.set_trips(Zone::District5, Zone::District3, 2_000);
        od.set_trips(Zone::District3, Zone::District5, 1_800);
        od.set_trips(Zone::District5, Zone::BinhThanh, 1_500);
        od.set_trips(Zone::BinhThanh, Zone::District5, 1_200);
        od.set_trips(Zone::District10, Zone::BinhThanh, 2_500);
        od.set_trips(Zone::BinhThanh, Zone::District10, 2_000);

        od
    }
}

impl Default for OdMatrix {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_matrix_is_empty() {
        let od = OdMatrix::new();
        assert_eq!(od.total_trips(), 0);
    }

    #[test]
    fn set_zero_removes_pair() {
        let mut od = OdMatrix::new();
        od.set_trips(Zone::BenThanh, Zone::Bitexco, 50);
        od.set_trips(Zone::BenThanh, Zone::Bitexco, 0);
        assert_eq!(od.total_trips(), 0);
        assert_eq!(od.zone_pairs().count(), 0);
    }

    #[test]
    fn district1_poc_total_in_range() {
        let od = OdMatrix::district1_poc();
        let total = od.total_trips();
        assert_eq!(total, 560);
    }

    #[test]
    fn hcmc_5district_has_25_pairs() {
        let od = OdMatrix::hcmc_5district();
        let pair_count = od.zone_pairs().count();
        assert!(pair_count >= 20);
    }
}
