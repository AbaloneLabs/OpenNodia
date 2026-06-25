//! Error types shared across OpenNodia crates.

use thiserror::Error;

/// Unified error type for OpenNodia.
#[derive(Debug, Error)]
pub enum Error {
    #[error("algod client error: {0}")]
    Algod(String),

    #[error("invalid address: {0}")]
    Address(String),

    #[error("invalid asset: {0}")]
    Asset(String),

    #[error("authentication error: {0}")]
    Auth(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

/// Convenience Result alias.
pub type Result<T> = std::result::Result<T, Error>;
