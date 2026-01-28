//! Integration tests for finalize action.
//!
//! Tests the FinalizeAction's ability to:
//! - Find a proven withdrawal on L1
//! - Check if proof maturity delay has passed
//! - Execute real finalize transaction

use crate::setup::{load_test_config, setup_provider, setup_signer};
use action::{
    finalize::{Finalize, FinalizeAction},
    Action,
};
use alloy_provider::Provider;
use alloy_rpc_types_eth::BlockNumberOrTag;
use binding::opstack::{MESSAGE_PASSER_ADDRESS, SECONDS_PER_DAY, SECONDS_PER_HOUR};
use withdrawal::{state::WithdrawalStateProvider, types::WithdrawalStatus};

#[path = "setup.rs"]
mod setup;

/// Test executing finalize action for a real proven withdrawal
///
/// This test:
/// 1. Scans L2 for pending withdrawals for the configured EOA
/// 2. Finds the most recent proven withdrawal that's past maturity delay
/// 3. Creates a FinalizeAction and executes it
/// 4. Submits the finalize transaction to L1
#[tokio::test]
#[ignore = "requires real proven withdrawals past maturity delay onchain and submits actual transaction - run with: just run-finalize"]
async fn test_finalize_action_execute() {
    // Initialize tracing for debug logs
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let config = load_test_config();

    println!("Testing finalize action execution");
    println!("L1 RPC: {}", config.l1_rpc_url);
    println!("L2 RPC: {}", config.l2_rpc_url);
    println!("Portal: {}", config.network_config().unichain.l1_portal);
    println!("EOA: {}", config.eoa_address);

    // Use provider and signer for L1 (needs to sign transactions)
    let l1_provider = setup_provider(&config.l1_rpc_url).await;
    let l2_provider = setup_provider(&config.l2_rpc_url).await;
    let l1_signer = setup_signer();

    // Find pending withdrawals
    let state_provider = WithdrawalStateProvider::new(
        l1_provider.clone(),
        l2_provider.clone(),
        config.network_config().unichain.l1_portal,
        MESSAGE_PASSER_ADDRESS,
    );

    let current_block = l2_provider.get_block_number().await.unwrap();
    let from_block = current_block.saturating_sub(1_200_000); // almost a week

    println!(
        "\nScanning blocks {} to {} for withdrawals",
        from_block, current_block
    );

    let withdrawals = state_provider
        .get_pending_withdrawals(
            BlockNumberOrTag::Number(from_block),
            BlockNumberOrTag::Latest,
            config.eoa_address,
        )
        .await
        .expect("Failed to scan withdrawals");

    println!("Found {} pending withdrawals", withdrawals.len());

    // Find the most recent proven withdrawal
    let proven_withdrawal = withdrawals
        .iter()
        .rev()
        .find(|w| matches!(w.status, WithdrawalStatus::Proven { .. }));

    if proven_withdrawal.is_none() {
        println!("⚠ No proven withdrawals found - cannot test finalize action");
        println!("  Prove a withdrawal first and wait for the maturity delay, then run this test");
        return;
    }

    let withdrawal = proven_withdrawal.unwrap();
    let proven_timestamp = match withdrawal.status {
        WithdrawalStatus::Proven { timestamp } => timestamp,
        _ => unreachable!(),
    };

    println!("\nFinalizing withdrawal:");
    println!("  Hash: {}", withdrawal.hash);
    println!("  L2 Block: {}", withdrawal.l2_block);
    println!("  Sender: {}", withdrawal.transaction.sender);
    println!("  Target: {}", withdrawal.transaction.target);
    println!("  Value: {}", withdrawal.transaction.value);
    println!("  Proven at timestamp: {}", proven_timestamp);

    // Create finalize action
    let finalize = Finalize {
        portal_address: config.network_config().unichain.l1_portal,
        withdrawal: withdrawal.transaction.clone(),
        withdrawal_hash: withdrawal.hash,
        proof_submitter: config.eoa_address, // Assuming we proved it ourselves
        from: config.eoa_address,
    };

    let mut action = FinalizeAction::new(l1_provider, l2_provider, l1_signer, finalize);

    // Check if ready
    println!("\nChecking if action is ready...");
    match action.is_ready().await {
        Ok(true) => println!("✓ Action is ready"),
        Ok(false) => {
            println!(
                "✗ Action is not ready (withdrawal may not be proven or maturity delay not passed)"
            );

            // Print more diagnostic info
            match action.is_completed().await {
                Ok(true) => println!("  Reason: Withdrawal is already finalized"),
                Ok(false) => println!("  Reason: Proof maturity delay has not elapsed yet"),
                Err(e) => println!("  Error checking status: {}", e),
            }
            return;
        }
        Err(e) => {
            println!("✗ Failed to check readiness: {}", e);
            return;
        }
    }

    // Execute the finalize action
    println!("\nExecuting finalize action...");
    let result = action
        .execute()
        .await
        .expect("Failed to execute finalize action");

    println!("✓ Successfully finalized withdrawal on L1!");
    println!("  Transaction hash: {}", result.tx_hash);
    println!("  Block number: {:?}", result.block_number);
    println!("  Gas used: {:?}", result.gas_used);

    // Verify it's now completed
    let completed = action
        .is_completed()
        .await
        .expect("Failed to check completion");
    assert!(completed, "Action should be completed after execution");
    println!("✓ Withdrawal is now finalized on L1");
}

/// Test that checks the status of proven withdrawals and their remaining time
///
/// This test doesn't execute any transactions, just checks the status.
#[tokio::test]
async fn test_check_proven_withdrawal_status() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let config = load_test_config();

    let l1_provider = setup_provider(&config.l1_rpc_url).await;
    let l2_provider = setup_provider(&config.l2_rpc_url).await;

    // Find pending withdrawals
    let state_provider = WithdrawalStateProvider::new(
        l1_provider.clone(),
        l2_provider.clone(),
        config.network_config().unichain.l1_portal,
        MESSAGE_PASSER_ADDRESS,
    );

    let current_block = l2_provider.get_block_number().await.unwrap();
    let from_block = current_block.saturating_sub(600_000);

    let withdrawals = state_provider
        .get_pending_withdrawals(
            BlockNumberOrTag::Number(from_block),
            BlockNumberOrTag::Latest,
            config.eoa_address,
        )
        .await
        .expect("Failed to scan withdrawals");

    println!("Found {} pending withdrawals", withdrawals.len());

    // Get proof maturity delay
    let portal = binding::opstack::IOptimismPortal2::new(
        config.network_config().unichain.l1_portal,
        &l1_provider,
    );
    let maturity_delay: alloy_primitives::U256 =
        portal.proofMaturityDelaySeconds().call().await.unwrap();
    let maturity_delay_secs: u64 = maturity_delay.try_into().unwrap_or(u64::MAX);

    println!(
        "Proof maturity delay: {} seconds ({} days)",
        maturity_delay_secs,
        maturity_delay_secs / SECONDS_PER_DAY
    );

    // Get current L1 timestamp
    let current_block_info = l1_provider
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await
        .unwrap()
        .unwrap();
    let current_timestamp = current_block_info.header.timestamp;

    for withdrawal in &withdrawals {
        match withdrawal.status {
            WithdrawalStatus::Proven { timestamp } => {
                let ready_at = timestamp + maturity_delay_secs;
                if current_timestamp >= ready_at {
                    println!(
                        "  {} - READY TO FINALIZE (proven at {}, ready since {})",
                        withdrawal.hash, timestamp, ready_at
                    );
                } else {
                    let remaining = ready_at - current_timestamp;
                    let remaining_hours = remaining / SECONDS_PER_HOUR;
                    let remaining_days = remaining / SECONDS_PER_DAY;
                    println!(
                        "  {} - NOT READY ({} hours / {} days remaining)",
                        withdrawal.hash, remaining_hours, remaining_days
                    );
                }
            }
            WithdrawalStatus::Initiated => {
                println!("  {} - INITIATED (not proven yet)", withdrawal.hash);
            }
            WithdrawalStatus::Finalized => {
                println!("  {} - FINALIZED", withdrawal.hash);
            }
        }
    }
}
