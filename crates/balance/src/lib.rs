//! Balance monitoring for blockchain accounts and contracts.
//!
//! This crate provides high-level interfaces for querying balances from
//! blockchain providers, with specific support for SpokePool relayer refund queries
//! and EOA token balances.

pub mod monitor;

use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use std::future::Future;

/// Represents a blockchain balance at a specific point in time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Balance {
    /// The address holding the balance
    pub holder: Address,
    /// The asset address (zero address for native token)
    pub asset: Address,
    /// The balance amount
    pub amount: U256,
}

/// Type of balance query to perform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BalanceQuery {
    /// Query ERC20 token balance for an EOA or contract
    ERC20Balance {
        /// Token contract address
        token: Address,
        /// Holder address
        holder: Address,
    },
    /// Query native ETH balance
    NativeBalance {
        /// Account address
        address: Address,
    },
    /// Query SpokePool relayer refund balance
    ///
    /// Calls `SpokePool.getRelayerRefund(token, relayer)` to get claimable balance
    SpokePoolBalance {
        /// SpokePool contract address
        spoke_pool: Address,
        /// Token address to query
        token: Address,
        /// Relayer address to query
        relayer: Address,
    },
}

/// Trait for monitoring balances on a blockchain.
pub trait Monitor: Send + Sync {
    /// Query a single balance.
    fn query_balance(
        &self,
        query: BalanceQuery,
    ) -> impl Future<Output = eyre::Result<Balance>> + Send;
}
