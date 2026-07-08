//! Route-data tooling for the maritime simulation.
//!
//! Provides the pieces the `gen_routes` binary composes into a defensible,
//! depth-aware procedural route generator:
//! - [`bathymetry`] — real EMODnet sea-depth grid (draft / under-keel clearance).
//! - [`ports`] — built-in major-port gazetteer for the sim bbox.
//! - [`router`] — cost-field weighted A* that keeps large ships in deep water,
//!   off the coast, and along plausible lanes, with seeded route variety.
#![allow(clippy::doc_markdown)]

pub mod bathymetry;
pub mod lanes;
pub mod ports;
pub mod router;
