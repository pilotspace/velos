//! Vehicle behaviour models for VELOS traffic microsimulation.
//!
//! This crate provides:
//! - **IDM** (Intelligent Driver Model) car-following acceleration
//! - **MOBIL** lane-change decision model
//! - **VehicleType** enum with default parameter sets
//! - **Gridlock** cycle detection on agent waiting graphs

pub mod bus;
pub mod config;
pub mod emergency;
pub mod error;
pub mod gridlock;
pub mod idm;
pub mod krauss;
pub mod mobil;
pub mod social_force;
pub mod sublane;
pub mod types;
