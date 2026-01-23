use alloy_primitives::Address;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Top-level orchestrator configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// L1 RPC endpoint url
    pub l1_rpc_url: String,

    /// L2 RPC endpoint url
    pub l2_rpc_url: String,

    /// L2 spoke pool address
    pub l2_spoke_pool_address: Address,

    /// L1 OptimismPortal2 address (for withdrawal proving/finalization)
    pub l1_portal_address: Address,

    /// EOA address
    pub eoa_address: Address,
}

impl Config {
    pub fn from_file(path: impl AsRef<Path>) -> eyre::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;

        Ok(config)
    }
}
