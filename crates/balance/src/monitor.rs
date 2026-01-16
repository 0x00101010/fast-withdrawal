use crate::{Balance, BalanceQuery, Monitor, MonitorError};
use alloy_primitives::{address, Address};
use alloy_provider::Provider;
use alloy_sol_types::sol;
use tracing::debug;

// Define SpokePool contract interface using Alloy's sol! macro
sol! {
   #[sol(rpc)]
   interface ISpokePool {
       /// Get the claimable relayer refund balance
       /// Returns the amount of l2TokenAddress that refundAddress can claim
       function getRelayerRefund(address l2TokenAddress, address refundAddress)
external view returns (uint256);
   }
}

// Define ERC20 contract interface
sol! {
   #[sol(rpc)]
   interface IERC20 {
       /// Get token balance of an account
       function balanceOf(address account) external view returns (uint256);
   }
}

const ETH_TOKEN_ADDRESS: Address = address!("0000000000000000000000000000000000000000");

// Balance monitor implementation.
pub struct BalanceMonitor<P> {
    provider: P,
}

impl<P> BalanceMonitor<P>
where
    P: Provider + Clone,
{
    #[allow(dead_code)]
    const fn new(provider: P) -> Self {
        Self { provider }
    }

    /// Query Across SpokePool relayer refund balance.
    async fn query_spoke_pool(
        &self,
        spoke_pool: Address,
        token: Address,
        relayer: Address,
    ) -> Result<Balance, MonitorError> {
        debug!(
            "Querying SpokePool balance: spokepool={}, token={}, relayer={}",
            spoke_pool, token, relayer
        );

        let contract = ISpokePool::new(spoke_pool, &self.provider);
        let balance = contract
            .getRelayerRefund(token, relayer)
            .call()
            .await
            .map_err(|e| MonitorError::ContractCall(format!("getRelayerRefund failed: {}", e)))?;

        Ok(Balance {
            holder: relayer,
            asset: token,
            amount: balance,
        })
    }

    async fn query_native(&self, address: Address) -> Result<Balance, MonitorError> {
        debug!("Querying native balance: address={}", address);

        let balance = self
            .provider
            .get_balance(address)
            .await
            .map_err(|e| MonitorError::Provider(format!("query balance failed: {}", e)))?;

        Ok(Balance {
            holder: address,
            asset: ETH_TOKEN_ADDRESS,
            amount: balance,
        })
    }
}

impl<P> Monitor for BalanceMonitor<P>
where
    P: Provider + Clone,
{
    async fn query_balance(&self, query: BalanceQuery) -> Result<Balance, MonitorError> {
        match query {
            BalanceQuery::SpokePoolBalance {
                spoke_pool,
                token,
                relayer,
            } => self.query_spoke_pool(spoke_pool, token, relayer).await,
            BalanceQuery::ERC20Balance {
                token: _,
                holder: _,
            } => {
                todo!("Implement ERC20 balance query")
            }
            BalanceQuery::NativeBalance { address } => self.query_native(address).await,
        }
    }
}
