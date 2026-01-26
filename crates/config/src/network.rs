//! Network configuration for cross-chain actions.
//!
//! Provides chain-specific addresses and parameters for different networks
//! (mainnet, testnet, etc.).

use alloy_primitives::{address, Address};
use serde::{Deserialize, Serialize};

/// Network type (mainnet or testnet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkType {
    Mainnet,
    Testnet,
}

/// Ethereum network configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumConfig {
    /// Chain ID
    pub chain_id: u64,
    /// WETH contract address
    pub weth: Address,
    /// Across SpokePool contract address
    pub spoke_pool: Address,
    /// Block time in seconds (12 for Ethereum mainnet)
    pub block_time_secs: u64,
}

impl EthereumConfig {
    /// Ethereum mainnet configuration.
    pub const fn mainnet() -> Self {
        Self {
            chain_id: 1,
            weth: address!("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"),
            // https://etherscan.io/address/0x5c7BCd6E7De5423a257D81B442095A1a6ced35C5
            spoke_pool: address!("0x5c7BCd6E7De5423a257D81B442095A1a6ced35C5"),
            block_time_secs: 12,
        }
    }

    /// Ethereum Sepolia testnet configuration.
    pub const fn sepolia() -> Self {
        Self {
            chain_id: 11155111,
            weth: address!("0xfFf9976782d46CC05630D1f6eBAb18b2324d6B14"),
            // https://sepolia.etherscan.io/address/0x5ef6C01E11889d86803e0B23e3cB3F9E9d97B662
            spoke_pool: address!("0x5ef6C01E11889d86803e0B23e3cB3F9E9d97B662"),
            block_time_secs: 12,
        }
    }
}

/// Unichain network configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnichainConfig {
    /// Chain ID
    pub chain_id: u64,
    /// WETH contract address (OP Stack predeploy)
    pub weth: Address,
    /// Across SpokePool contract address
    pub spoke_pool: Address,
    /// Block time in seconds (1 for Unichain)
    pub block_time_secs: u64,
}

impl UnichainConfig {
    /// Unichain mainnet configuration.
    pub const fn mainnet() -> Self {
        Self {
            chain_id: 130,
            weth: address!("0x4200000000000000000000000000000000000006"),
            // https://uniscan.xyz/address/0x09aea4b2242abC8bb4BB78D537A67a245A7bEC64
            spoke_pool: address!("0x09aea4b2242abC8bb4BB78D537A67a245A7bEC64"),
            block_time_secs: 1,
        }
    }

    /// Unichain Sepolia testnet configuration.
    pub const fn sepolia() -> Self {
        Self {
            chain_id: 1301,
            weth: address!("4200000000000000000000000000000000000006"),
            // https://uniscan.xyz/address/0x6999526e507Cc3b03b180BbE05E1Ff938259A874
            spoke_pool: address!("0x6999526e507Cc3b03b180BbE05E1Ff938259A874"),
            block_time_secs: 1,
        }
    }
}

/// Complete network configuration for cross-chain actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Network type (mainnet or testnet)
    pub network_type: NetworkType,
    /// Ethereum/L1 configuration
    pub ethereum: EthereumConfig,
    /// Unichain/L2 configuration
    pub unichain: UnichainConfig,
}

impl NetworkConfig {
    /// Create mainnet configuration.
    pub const fn mainnet() -> Self {
        Self {
            network_type: NetworkType::Mainnet,
            ethereum: EthereumConfig::mainnet(),
            unichain: UnichainConfig::mainnet(),
        }
    }

    /// Create testnet (Sepolia) configuration.
    pub const fn sepolia() -> Self {
        Self {
            network_type: NetworkType::Testnet,
            ethereum: EthereumConfig::sepolia(),
            unichain: UnichainConfig::sepolia(),
        }
    }

    /// Create configuration from network type.
    pub const fn from_network_type(network_type: NetworkType) -> Self {
        match network_type {
            NetworkType::Mainnet => Self::mainnet(),
            NetworkType::Testnet => Self::sepolia(),
        }
    }
}

/// Builder for custom network configurations.
#[derive(Debug, Clone)]
pub struct NetworkConfigBuilder {
    network_type: NetworkType,
    ethereum: EthereumConfig,
    unichain: UnichainConfig,
}

impl NetworkConfigBuilder {
    /// Start with mainnet defaults.
    pub const fn mainnet() -> Self {
        Self {
            network_type: NetworkType::Mainnet,
            ethereum: EthereumConfig::mainnet(),
            unichain: UnichainConfig::mainnet(),
        }
    }

    /// Start with testnet defaults.
    pub const fn testnet() -> Self {
        Self {
            network_type: NetworkType::Testnet,
            ethereum: EthereumConfig::sepolia(),
            unichain: UnichainConfig::sepolia(),
        }
    }

    /// Override Ethereum SpokePool address.
    pub const fn ethereum_spoke_pool(mut self, address: Address) -> Self {
        self.ethereum.spoke_pool = address;
        self
    }

    /// Override Ethereum WETH address.
    pub const fn ethereum_weth(mut self, address: Address) -> Self {
        self.ethereum.weth = address;
        self
    }

    /// Override Unichain SpokePool address.
    pub const fn unichain_spoke_pool(mut self, address: Address) -> Self {
        self.unichain.spoke_pool = address;
        self
    }

    /// Override Unichain WETH address.
    pub const fn unichain_weth(mut self, address: Address) -> Self {
        self.unichain.weth = address;
        self
    }

    /// Build the network configuration.
    pub const fn build(self) -> NetworkConfig {
        NetworkConfig {
            network_type: self.network_type,
            ethereum: self.ethereum,
            unichain: self.unichain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mainnet_config() {
        let config = NetworkConfig::mainnet();
        assert_eq!(config.ethereum.chain_id, 1);
        assert_eq!(config.unichain.chain_id, 130);
        assert_eq!(config.network_type, NetworkType::Mainnet);
    }

    #[test]
    fn test_sepolia_config() {
        let config = NetworkConfig::sepolia();
        assert_eq!(config.ethereum.chain_id, 11155111);
        assert_eq!(config.network_type, NetworkType::Testnet);
    }

    #[test]
    fn test_custom_config_builder() {
        let custom_spoke_pool = address!("1111111111111111111111111111111111111111");

        let config = NetworkConfigBuilder::mainnet()
            .ethereum_spoke_pool(custom_spoke_pool)
            .build();

        assert_eq!(config.ethereum.spoke_pool, custom_spoke_pool);
        assert_eq!(config.network_type, NetworkType::Mainnet);
    }
}
