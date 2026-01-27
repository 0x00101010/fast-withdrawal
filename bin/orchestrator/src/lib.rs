pub mod config;
pub mod metrics;

use crate::metrics::Metrics;
use action::{
    deposit::{DepositAction, DepositConfig},
    finalize::{Finalize, FinalizeAction},
    prove::{Prove, ProveAction},
    withdraw::{Withdraw, WithdrawAction},
    Action,
};
use alloy_primitives::{utils::format_ether, Address, Bytes, U256};
use alloy_provider::Provider;
use alloy_rpc_types_eth::BlockNumberOrTag;
use balance::{monitor::BalanceMonitor, Balance, BalanceQuery, Monitor};
use deposit::get_inflight_deposits;
use tracing::{error, info, warn};
use withdrawal::{
    state::{PendingWithdrawal, WithdrawalStateProvider},
    types::WithdrawalStatus,
};

/// Convert ETH string from format_ether to f64 for metrics.
fn eth_to_f64(eth_str: String) -> f64 {
    eth_str.parse::<f64>().unwrap_or(0.0)
}

/// Update all metrics gauges with current state.
///
/// Queries balances, in-flight deposits, and pending withdrawals, then updates
/// the metrics accordingly. Errors are logged but don't fail the function.
pub async fn update_metrics<P1, P2>(
    l1_provider: P1,
    l2_provider: P2,
    config: &config::Config,
    metrics: &Metrics,
) where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    let network = config.network_config();

    // 1. L1 EOA balance
    match l1_provider.get_balance(config.eoa_address).await {
        Ok(balance) => metrics.set_l1_eoa_balance_eth(eth_to_f64(format_ether(balance))),
        Err(e) => warn!(error = %e, "Failed to get L1 EOA balance for metrics"),
    }

    // 2. L2 EOA balance
    match l2_provider.get_balance(config.eoa_address).await {
        Ok(balance) => metrics.set_l2_eoa_balance_eth(eth_to_f64(format_ether(balance))),
        Err(e) => warn!(error = %e, "Failed to get L2 EOA balance for metrics"),
    }

    // 3. SpokePool WETH balance
    let l2_monitor = BalanceMonitor::new(l2_provider.clone());
    match check_l2_spoke_pool_balance(
        &l2_monitor,
        network.unichain.spoke_pool,
        network.unichain.weth,
    )
    .await
    {
        Ok(balance) => metrics.set_spoke_pool_balance_eth(eth_to_f64(format_ether(balance.amount))),
        Err(e) => warn!(error = %e, "Failed to get SpokePool balance for metrics"),
    }

    // 4. In-flight deposits
    match get_inflight_deposits(
        l1_provider.clone(),
        l2_provider.clone(),
        network.ethereum.spoke_pool,
        network.unichain.spoke_pool,
        config.eoa_address,
        network.unichain.chain_id,
        network.ethereum.chain_id,
        config.deposit_lookback_secs,
        network.ethereum.block_time_secs,
        network.unichain.block_time_secs,
    )
    .await
    {
        Ok(deposits) => {
            let total: U256 = deposits.iter().map(|d| d.input_amount).sum();
            metrics.set_inflight_deposits(deposits.len(), eth_to_f64(format_ether(total)));
        }
        Err(e) => warn!(error = %e, "Failed to get in-flight deposits for metrics"),
    }

    // 5. In-flight withdrawals (by status)
    let l2_current_block = match l2_provider.get_block_number().await {
        Ok(b) => b,
        Err(e) => {
            warn!(error = %e, "Failed to get L2 block number for withdrawal metrics");
            return;
        }
    };
    let lookback_blocks = config.withdrawal_lookback_secs / network.unichain.block_time_secs;
    let from_block = l2_current_block.saturating_sub(lookback_blocks);

    let state_provider = WithdrawalStateProvider::new(
        l1_provider,
        l2_provider,
        network.unichain.l1_portal,
        network.unichain.l2_to_l1_message_passer,
    );

    match state_provider
        .get_pending_withdrawals(
            BlockNumberOrTag::Number(from_block),
            BlockNumberOrTag::Latest,
            config.eoa_address,
        )
        .await
    {
        Ok(pending) => {
            let mut initiated_count = 0usize;
            let mut initiated_amount = U256::ZERO;
            let mut proven_count = 0usize;
            let mut proven_amount = U256::ZERO;

            for w in &pending {
                match w.status {
                    WithdrawalStatus::Initiated => {
                        initiated_count += 1;
                        initiated_amount += w.transaction.value;
                    }
                    WithdrawalStatus::Proven { .. } => {
                        proven_count += 1;
                        proven_amount += w.transaction.value;
                    }
                    WithdrawalStatus::Finalized => {}
                }
            }

            metrics.set_inflight_withdrawals(
                initiated_count,
                eth_to_f64(format_ether(initiated_amount)),
                proven_count,
                eth_to_f64(format_ether(proven_amount)),
            );
        }
        Err(e) => warn!(error = %e, "Failed to get pending withdrawals for metrics"),
    }
}

pub async fn check_l2_spoke_pool_balance<P>(
    monitor: &BalanceMonitor<P>,
    spoke_pool: Address,
    token: Address,
) -> eyre::Result<Balance>
where
    P: Provider + Clone,
{
    let query = BalanceQuery::ERC20Balance {
        token,
        holder: spoke_pool,
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

/// Process all pending withdrawals - finalize mature ones, prove initiated ones.
///
/// Scans for withdrawals based on lookback time and processes them based on their status:
/// - Proven + mature: Execute finalize
/// - Initiated: Execute prove
///
/// Errors are logged but don't halt processing of other withdrawals.
pub async fn process_pending_withdrawals<P1, P2>(
    l1_provider: P1,
    l2_provider: P2,
    config: &config::Config,
) -> eyre::Result<()>
where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    let network = config.network_config();

    // Calculate from_block based on lookback time
    let l2_current_block = l2_provider.get_block_number().await?;
    let lookback_blocks = config.withdrawal_lookback_secs / network.unichain.block_time_secs;
    let from_block = l2_current_block.saturating_sub(lookback_blocks);

    let state_provider = WithdrawalStateProvider::new(
        l1_provider.clone(),
        l2_provider.clone(),
        network.unichain.l1_portal,
        network.unichain.l2_to_l1_message_passer,
    );

    let pending = state_provider
        .get_pending_withdrawals(
            BlockNumberOrTag::Number(from_block),
            BlockNumberOrTag::Latest,
            config.eoa_address,
        )
        .await?;

    if pending.is_empty() {
        info!("No pending withdrawals found");
        return Ok(());
    }

    info!(count = pending.len(), "Found pending withdrawals");

    for withdrawal in &pending {
        match &withdrawal.status {
            WithdrawalStatus::Proven { .. } => {
                if let Err(e) = finalize_withdrawal(
                    l1_provider.clone(),
                    l2_provider.clone(),
                    network.unichain.l1_portal,
                    config.eoa_address,
                    withdrawal,
                    config.dry_run,
                )
                .await
                {
                    warn!(
                        withdrawal_hash = %withdrawal.hash,
                        error = %e,
                        "Failed to finalize withdrawal"
                    );
                }
            }
            WithdrawalStatus::Initiated => {
                if let Err(e) = prove_withdrawal(
                    l1_provider.clone(),
                    l2_provider.clone(),
                    network.unichain.l1_portal,
                    network.unichain.l1_dispute_game_factory,
                    withdrawal,
                    config.dry_run,
                )
                .await
                {
                    warn!(
                        withdrawal_hash = %withdrawal.hash,
                        error = %e,
                        "Failed to prove withdrawal"
                    );
                }
            }
            WithdrawalStatus::Finalized => {
                // Should not appear in pending list, but handle gracefully
            }
        }
    }

    Ok(())
}

/// Finalize a single proven withdrawal.
async fn finalize_withdrawal<P1, P2>(
    l1_provider: P1,
    l2_provider: P2,
    portal_address: Address,
    proof_submitter: Address,
    withdrawal: &PendingWithdrawal,
    dry_run: bool,
) -> eyre::Result<()>
where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    let finalize = Finalize {
        portal_address,
        withdrawal: withdrawal.transaction.clone(),
        withdrawal_hash: withdrawal.hash,
        proof_submitter,
    };

    let mut action = FinalizeAction::new(l1_provider, l2_provider, finalize);

    if !action.is_ready().await? {
        info!(
            withdrawal_hash = %withdrawal.hash,
            "Withdrawal not ready to finalize (proof not mature)"
        );
        return Ok(());
    }

    if dry_run {
        info!(
            withdrawal_hash = %withdrawal.hash,
            "[DRY-RUN] Would finalize withdrawal"
        );
        return Ok(());
    }

    info!(withdrawal_hash = %withdrawal.hash, "Finalizing withdrawal");

    match action.execute().await {
        Ok(result) => {
            info!(
                withdrawal_hash = %withdrawal.hash,
                tx_hash = %result.tx_hash,
                "Withdrawal finalized"
            );
        }
        Err(e) => {
            error!(
                withdrawal_hash = %withdrawal.hash,
                error = %e,
                "Failed to execute finalize"
            );
            return Err(e);
        }
    }

    Ok(())
}

/// Prove a single initiated withdrawal.
async fn prove_withdrawal<P1, P2>(
    l1_provider: P1,
    l2_provider: P2,
    portal_address: Address,
    factory_address: Address,
    withdrawal: &PendingWithdrawal,
    dry_run: bool,
) -> eyre::Result<()>
where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    let prove = Prove {
        portal_address,
        factory_address,
        withdrawal: withdrawal.transaction.clone(),
        withdrawal_hash: withdrawal.hash,
        l2_block: withdrawal.l2_block,
    };

    let mut action = ProveAction::new(l1_provider, l2_provider, prove);

    if !action.is_ready().await? {
        info!(
            withdrawal_hash = %withdrawal.hash,
            "Withdrawal already proven"
        );
        return Ok(());
    }

    if dry_run {
        info!(
            withdrawal_hash = %withdrawal.hash,
            "[DRY-RUN] Would prove withdrawal"
        );
        return Ok(());
    }

    info!(withdrawal_hash = %withdrawal.hash, "Proving withdrawal");

    match action.execute().await {
        Ok(result) => {
            info!(
                withdrawal_hash = %withdrawal.hash,
                tx_hash = %result.tx_hash,
                "Withdrawal proven"
            );
        }
        Err(e) => {
            error!(
                withdrawal_hash = %withdrawal.hash,
                error = %e,
                "Failed to execute prove"
            );
            return Err(e);
        }
    }

    Ok(())
}

/// Check L2 EOA balance and initiate withdrawal if threshold met.
///
/// Returns the withdrawal amount if a withdrawal was initiated, None otherwise.
pub async fn maybe_initiate_withdrawal<P>(
    l2_provider: P,
    config: &config::Config,
) -> eyre::Result<Option<U256>>
where
    P: Provider + Clone,
{
    let network = config.network_config();
    let balance = l2_provider.get_balance(config.eoa_address).await?;

    if balance <= config.withdrawal_threshold_wei {
        info!(
            balance = %format_ether(balance),
            threshold = %format_ether(config.withdrawal_threshold_wei),
            "L2 EOA balance below threshold, skipping withdrawal"
        );
        return Ok(None);
    }

    // Withdraw everything except gas buffer
    let withdrawal_amount = balance.saturating_sub(config.gas_buffer_wei);

    if withdrawal_amount == U256::ZERO {
        info!("Nothing to withdraw after gas buffer");
        return Ok(None);
    }

    if config.dry_run {
        info!(
            balance = %format_ether(balance),
            withdrawal_amount = %format_ether(withdrawal_amount),
            "[DRY-RUN] Would initiate L2→L1 withdrawal"
        );
        return Ok(Some(withdrawal_amount));
    }

    info!(
        balance = %format_ether(balance),
        withdrawal_amount = %format_ether(withdrawal_amount),
        "Initiating L2→L1 withdrawal"
    );

    let withdraw = Withdraw {
        contract: network.unichain.l2_to_l1_message_passer,
        source: config.eoa_address,
        target: config.eoa_address, // Send to same address on L1
        value: withdrawal_amount,
        gas_limit: U256::from(300_000),
        data: Bytes::new(),
        tx_hash: None,
    };

    let mut action = WithdrawAction::new(l2_provider, withdraw);

    match action.execute().await {
        Ok(result) => {
            info!(
                tx_hash = %result.tx_hash,
                amount = %format_ether(withdrawal_amount),
                "Withdrawal initiated"
            );
            Ok(Some(withdrawal_amount))
        }
        Err(e) => {
            error!(error = %e, "Failed to initiate withdrawal");
            Err(e)
        }
    }
}

/// Check SpokePool balance (with in-flight adjustment) and deposit if needed.
///
/// Logic:
/// 1. Get actual L2 SpokePool balance
/// 2. Get in-flight deposit total (initiated but not yet filled)
/// 3. Calculate projected_balance = actual - inflight
/// 4. If projected_balance > target: deposit (projected - floor)
///
/// Returns the deposit amount if a deposit was executed, None otherwise.
pub async fn maybe_deposit<P1, P2>(
    l1_provider: P1,
    l2_provider: P2,
    config: &config::Config,
) -> eyre::Result<Option<U256>>
where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    let network = config.network_config();

    // Get actual L2 SpokePool balance
    let l2_monitor = BalanceMonitor::new(l2_provider.clone());
    let actual_balance = check_l2_spoke_pool_balance(
        &l2_monitor,
        network.unichain.spoke_pool,
        network.unichain.weth,
    )
    .await?;

    // Get in-flight deposit total
    let inflight_deposits = get_inflight_deposits(
        l1_provider.clone(),
        l2_provider,
        network.ethereum.spoke_pool,
        network.unichain.spoke_pool,
        config.eoa_address,
        network.unichain.chain_id,
        network.ethereum.chain_id,
        config.deposit_lookback_secs,
        network.ethereum.block_time_secs,
        network.unichain.block_time_secs,
    )
    .await?;
    let inflight_total: U256 = inflight_deposits.iter().map(|d| d.input_amount).sum();

    // Calculate projected balance
    let projected_balance = actual_balance.amount.saturating_sub(inflight_total);

    info!(
        actual_balance = %format_ether(actual_balance.amount),
        inflight_total = %format_ether(inflight_total),
        projected_balance = %format_ether(projected_balance),
        target = %format_ether(config.spoke_pool_target_wei),
        "Checking deposit conditions"
    );

    if projected_balance <= config.spoke_pool_target_wei {
        info!("Projected balance below target, skipping deposit");
        return Ok(None);
    }

    // Calculate deposit amount: projected - floor
    let deposit_amount = projected_balance.saturating_sub(config.spoke_pool_floor_wei);

    if deposit_amount == U256::ZERO {
        info!("Nothing to deposit after floor");
        return Ok(None);
    }

    // Check L1 EOA balance
    let l1_balance = l1_provider.get_balance(config.eoa_address).await?;
    if l1_balance < deposit_amount {
        warn!(
            l1_balance = %format_ether(l1_balance),
            deposit_amount = %format_ether(deposit_amount),
            "Insufficient L1 balance for deposit"
        );
        return Ok(None);
    }

    if config.dry_run {
        info!(
            deposit_amount = %format_ether(deposit_amount),
            "[DRY-RUN] Would execute deposit"
        );
        return Ok(Some(deposit_amount));
    }

    info!(
        deposit_amount = %format_ether(deposit_amount),
        "Executing deposit"
    );

    // Calculate fill deadline (current time + 1 hour)
    let fill_deadline = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() as u32
        + 3600;

    let deposit_config = DepositConfig {
        spoke_pool: network.ethereum.spoke_pool,
        depositor: config.eoa_address,
        recipient: config.eoa_address,
        input_token: network.ethereum.weth,
        output_token: network.unichain.weth,
        input_amount: deposit_amount,
        output_amount: deposit_amount * U256::from(2), // This is to enforce slow fill as no relayer would want to fill that
        destination_chain_id: network.unichain.chain_id,
        exclusive_relayer: Address::ZERO,
        fill_deadline,
        exclusivity_parameter: 0,
        message: Bytes::new(),
    };

    let mut action = DepositAction::new(l1_provider, deposit_config);

    match action.execute().await {
        Ok(result) => {
            info!(
                tx_hash = %result.tx_hash,
                amount = %format_ether(deposit_amount),
                "Deposit executed"
            );
            Ok(Some(deposit_amount))
        }
        Err(e) => {
            error!(error = %e, "Failed to execute deposit");
            Err(e)
        }
    }
}
