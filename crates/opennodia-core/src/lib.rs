//! OpenNodia core types and primitives.
//!
//! This crate provides the foundational types shared across all OpenNodia
//! modules: addresses, asset IDs, amounts, rounds, and error types.

pub mod address;
pub mod amount;
pub mod asset;
pub mod error;
pub mod network;
pub mod round;

pub use address::Address;
pub use amount::MicroAlgo;
pub use asset::AssetId;
pub use error::{Error, Result};
pub use network::Network;
pub use round::Round;
