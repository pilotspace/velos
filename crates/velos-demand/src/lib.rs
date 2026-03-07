//! velos-demand: OD matrices, time-of-day profiles, and agent spawning.
//!
//! Demand generation drives agent creation for the VELOS traffic microsimulation.
//! Combines Origin-Destination matrices with time-of-day scaling profiles to
//! produce realistic traffic patterns for HCMC District 1 POC.

pub mod error;
pub mod od_matrix;
pub mod spawner;
pub mod tod_profile;

pub use error::DemandError;
pub use od_matrix::{NamedZone, OdMatrix, Zone};
pub use spawner::{SpawnRequest, SpawnVehicleType, Spawner};
pub use tod_profile::TodProfile;
