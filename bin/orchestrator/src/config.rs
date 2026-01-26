use alloy_primitives::Address;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Default lookback time for in-flight deposit scanning (12 hours in seconds).
const DEFAULT_DEPOSIT_LOOKBACK_SECS: u64 = 43200;

/// Top-level orchestrator configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// L1 RPC endpoint url
    pub l1_rpc_url: String,

    /// L2 RPC endpoint url
    pub l2_rpc_url: String,

    /// L1 OptimismPortal2 address (for withdrawal proving/finalization)
    pub l1_portal_address: Address,

    /// EOA address
    pub eoa_address: Address,

    /// L1 DisputeGameFactory address (for finding dispute games)
    pub l1_dispute_game_factory_address: Address,

    /// How far back to scan for in-flight deposits (in seconds).
    /// Default: 43200 (12 hours)
    #[serde(default = "default_deposit_lookback_secs")]
    pub deposit_lookback_secs: u64,
}

const fn default_deposit_lookback_secs() -> u64 {
    DEFAULT_DEPOSIT_LOOKBACK_SECS
}

impl Config {
    pub fn from_file(path: impl AsRef<Path>) -> eyre::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;

        Ok(config)
    }
}
