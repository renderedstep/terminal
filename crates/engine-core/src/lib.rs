//! The text-adventure engine's pure rules: the dice and the geometry.
//!
//! Every function here takes values and returns values. Nothing reads a
//! database, the network or the clock, so each rule can be checked against
//! the golden vectors the Ruby engine writes (see `vectors/README.md`).
//!
//! Draws come from [`random::Random`], a reproduction of Ruby's `Random`, and
//! never from any other generator: the same seed has to roll the same dice in
//! both engines.

pub mod boxes;
pub mod cast;
pub mod danger;
pub mod deadline;
pub mod interior;
pub mod parameters;
pub mod population;
pub mod random;
pub mod roll;
pub mod shuffle_connections;
pub mod spot;
pub mod stat_block;
pub mod text;
pub mod world_mechanic;
