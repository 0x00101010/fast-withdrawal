use crate::{Balance, BalanceQuery, Monitor};
use alloy_primitives::Address;
use alloy_provider::Provider;
use binding::{across::ISpokePool, token::IERC20};
use eyre::Result;
use tracing::debug;

// Balance monitor implementation.
pub struct BalanceMonitor<P> {
    provider: P,
}

impl<P> BalanceMonitor<P>
where
    P: Provider + Clone,
{
    pub const fn new(provider: P) -> Self {
        Self { provider }
    }

    /// Query Across SpokePool relayer refund balance.
    async fn query_spoke_pool(
        &self,
        spoke_pool: Address,
        token: Address,
        relayer: Address,
    ) -> Result<Balance> {
        debug!(
            "Querying SpokePool balance: spokepool={}, token={}, relayer={}",
            spoke_pool, token, relayer
        );

        let contract = ISpokePool::new(spoke_pool, &self.provider);
        let amount = contract.getRelayerRefund(token, relayer).call().await?;

        Ok(Balance {
            holder: relayer,
            asset: token,
            amount,
        })
    }

    async fn query_native(&self, address: Address) -> Result<Balance> {
        debug!("Querying native balance: address={}", address);

        let balance = self.provider.get_balance(address).await?;

        Ok(Balance {
            holder: address,
            asset: Address::ZERO,
            amount: balance,
        })
    }

    async fn query_erc20(&self, token: Address, holder: Address) -> Result<Balance> {
        debug!("Querying erc20 {} balance: address={}", token, holder);

        let contract = IERC20::new(token, &self.provider);
        let amount = contract.balanceOf(holder).call().await?;

        Ok(Balance {
            holder,
            asset: token,
            amount,
        })
    }
}

impl<P> Monitor for BalanceMonitor<P>
where
    P: Provider + Clone,
{
    async fn query_balance(&self, query: BalanceQuery) -> Result<Balance> {
        match query {
            BalanceQuery::SpokePoolBalance {
                spoke_pool,
                token,
                relayer,
            } => self.query_spoke_pool(spoke_pool, token, relayer).await,
            BalanceQuery::ERC20Balance { token, holder } => self.query_erc20(token, holder).await,
            BalanceQuery::NativeBalance { address } => self.query_native(address).await,
        }
    }
}
