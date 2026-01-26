use alloy_primitives::{Address, U256};
pub use config::{NetworkConfig, NetworkType};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Top-level orchestrator configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// L1 RPC endpoint url
    pub l1_rpc_url: String,

    /// L2 RPC endpoint url
    pub l2_rpc_url: String,

    /// Network type (mainnet or testnet)
    pub network: NetworkType,

    /// EOA address
    pub eoa_address: Address,

    /// How far back to scan for in-flight deposits (in seconds).
    #[serde(default = "default_deposit_lookback_secs")]
    pub deposit_lookback_secs: u64,

    /// Trigger deposit when L2 SpokePool balance exceeds this value.
    #[serde(default = "default_spoke_pool_target_wei")]
    pub spoke_pool_target_wei: U256,

    /// Minimum to leave in L2 SpokePool after deposit.
    #[serde(default = "default_spoke_pool_floor_wei")]
    pub spoke_pool_floor_wei: U256,

    /// Trigger L2→L1 withdrawal when L2 EOA balance exceeds this value.
    #[serde(default = "default_withdrawal_threshold_wei")]
    pub withdrawal_threshold_wei: U256,

    /// Leave this much ETH on L2 EOA for gas.
    #[serde(default = "default_gas_buffer_wei")]
    pub gas_buffer_wei: U256,

    /// How far back to scan for pending withdrawals (in seconds).
    #[serde(default = "default_withdrawal_lookback_secs")]
    pub withdrawal_lookback_secs: u64,

    /// How often to run the main loop (in seconds).
    #[serde(default = "default_cycle_interval_secs")]
    pub cycle_interval_secs: u64,

    /// Dry-run mode: log actions without executing transactions.
    #[serde(default)]
    pub dry_run: bool,
}

const fn default_deposit_lookback_secs() -> u64 {
    43200
}

fn default_spoke_pool_target_wei() -> U256 {
    U256::from(75_000_000_000_000_000_000_u128)
}

fn default_spoke_pool_floor_wei() -> U256 {
    U256::from(20_000_000_000_000_000_000_u128)
}

fn default_withdrawal_threshold_wei() -> U256 {
    U256::from(75_000_000_000_000_000_000_u128)
}

fn default_gas_buffer_wei() -> U256 {
    U256::from(10_000_000_000_000_000_u128)
}

const fn default_withdrawal_lookback_secs() -> u64 {
    1_209_600 // 2 weeks in seconds
}

const fn default_cycle_interval_secs() -> u64 {
    30
}

impl Config {
    pub fn from_file(path: impl AsRef<Path>) -> eyre::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;

        Ok(config)
    }

    /// Get the network configuration based on the configured network type.
    pub const fn network_config(&self) -> NetworkConfig {
        NetworkConfig::from_network_type(self.network)
    }
}
