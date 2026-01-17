mod config;

use alloy_primitives::Address;
use alloy_provider::Provider;
use balance::{monitor::BalanceMonitor, Balance, BalanceQuery, Monitor};

pub async fn check_l2_spoke_pool_balance<P>(
    monitor: &BalanceMonitor<P>,
    spoke_pool: Address,
    token: Address,
    relayer: Address,
) -> eyre::Result<Balance>
where
    P: Provider + Clone,
{
    let query = BalanceQuery::SpokePoolBalance {
        spoke_pool,
        token,
        relayer,
    };
    let balance = monitor.query_balance(query).await?;

    Ok(balance)
}

pub async fn check_l1_native_balance<P>(
    monitor: &BalanceMonitor<P>,
    address: Address,
) -> eyre::Result<Balance>
where
    P: Provider + Clone,
{
    let query = BalanceQuery::NativeBalance { address };
    let balance = monitor.query_balance(query).await?;
    Ok(balance)
}
