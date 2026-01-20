//! Configuration types for the fast-withdrawal system.
//!
//! This crate provides:
//! - Network configurations (mainnet, testnet)
//! - Contract addresses for different chains
//! - Configuration loading and validation

pub mod network;

pub use network::{
    EthereumConfig, NetworkConfig, NetworkConfigBuilder, NetworkType, UnichainConfig,
};
