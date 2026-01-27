use alloy_primitives::{Address, U256};
pub use config::{NetworkConfig, NetworkType};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Top-level orchestrator configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    pub deposit_lookback_secs: u64,

    /// Trigger deposit when L2 SpokePool balance exceeds this value.
    pub spoke_pool_target_wei: U256,

    /// Minimum to leave in L2 SpokePool after deposit.
    pub spoke_pool_floor_wei: U256,

    /// Trigger L2→L1 withdrawal when L2 EOA balance exceeds this value.
    pub withdrawal_threshold_wei: U256,

    /// Leave this much ETH on L2 EOA for gas.
    pub gas_buffer_wei: U256,

    /// How far back to scan for pending withdrawals (in seconds).
    pub withdrawal_lookback_secs: u64,

    /// How often to run the main loop (in seconds).
    pub cycle_interval_secs: u64,

    /// Dry-run mode: log actions without executing transactions.
    pub dry_run: bool,

    /// Port for Prometheus metrics HTTP server.
    pub metrics_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            l1_rpc_url: String::new(),
            l2_rpc_url: String::new(),
            network: NetworkType::Testnet,
            eoa_address: Address::ZERO,
            deposit_lookback_secs: 43200, // 12 hours
            spoke_pool_target_wei: U256::from(75_000_000_000_000_000_000_u128), // 75 ETH
            spoke_pool_floor_wei: U256::from(20_000_000_000_000_000_000_u128),  // 20 ETH
            withdrawal_threshold_wei: U256::from(75_000_000_000_000_000_000_u128), // 75 ETH
            gas_buffer_wei: U256::from(10_000_000_000_000_000_u128), // 0.01 ETH
            withdrawal_lookback_secs: 1_209_600, // 2 weeks
            cycle_interval_secs: 30,
            dry_run: false,
            metrics_port: 9090,
        }
    }
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
