//! Integration tests for withdrawal state querying.
//!
//! Tests the WithdrawalStateProvider's ability to:
//! - Scan L2 for MessagePassed events
//! - Query L1 for withdrawal status (proven/finalized)
//! - Reconstruct state from blockchain

use crate::setup::{load_test_config, setup_provider};
use alloy_primitives::{address, Address};
use alloy_provider::Provider;
use withdrawal::state::WithdrawalStateProvider;
use withdrawal::types::WithdrawalStatus;

#[path = "setup.rs"]
mod setup;

const MESSAGE_PASSER_ADDRESS: Address =
    address!("4200000000000000000000000000000000000016");

#[tokio::test]
async fn test_state_provider_creation() {
    let config = load_test_config();

    println!("Creating withdrawal state provider");
    println!("L1 RPC: {}", config.l1_rpc_url);
    println!("L2 RPC: {}", config.l2_rpc_url);
    println!("L1 Portal: {}", config.l1_portal_address);

    let l1_provider = setup_provider(&config.l1_rpc_url).await;
    let l2_provider = setup_provider(&config.l2_rpc_url).await;

    let _state_provider = WithdrawalStateProvider::new(
        l1_provider,
        l2_provider,
        config.l1_portal_address,
        MESSAGE_PASSER_ADDRESS,
    );

    println!("✓ State provider created successfully");
}

#[tokio::test]
async fn test_scan_pending_withdrawals_larger_range() {
    let config = load_test_config();

    println!("Testing scan of larger block range");

    let l1_provider = setup_provider(&config.l1_rpc_url).await;
    let l2_provider = setup_provider(&config.l2_rpc_url).await;

    let state_provider = WithdrawalStateProvider::new(
        l1_provider,
        l2_provider.clone(),
        config.l1_portal_address,
        MESSAGE_PASSER_ADDRESS,
    );

    // Scan last 10,000 blocks (should find some withdrawals on testnet)
    let current_block = l2_provider.get_block_number().await.unwrap();
    let from_block = current_block.saturating_sub(10_000);

    println!("Scanning blocks {} to {}", from_block, current_block);

    let withdrawals = state_provider
        .get_pending_withdrawals(from_block, current_block, config.eoa_address)
        .await
        .expect("Failed to scan withdrawals");

    println!("Found {} pending withdrawals in last 10k blocks", withdrawals.len());

    // Print summary of statuses
    let mut initiated_count = 0;
    let mut proven_count = 0;

    for withdrawal in &withdrawals {
        match withdrawal.status {
            WithdrawalStatus::Initiated => initiated_count += 1,
            WithdrawalStatus::Proven { .. } => proven_count += 1,
            WithdrawalStatus::Finalized => {
                panic!("Found finalized withdrawal - should have been filtered out")
            }
        }
    }

    println!("  Initiated: {}", initiated_count);
    println!("  Proven: {}", proven_count);
    println!("  Finalized: 0 (filtered out)");

    println!("✓ Larger scan completed successfully");
}

#[tokio::test]
async fn test_query_withdrawal_status() {
    let config = load_test_config();

    println!("Testing withdrawal status querying");

    let l1_provider = setup_provider(&config.l1_rpc_url).await;
    let l2_provider = setup_provider(&config.l2_rpc_url).await;

    let state_provider = WithdrawalStateProvider::new(
        l1_provider,
        l2_provider.clone(),
        config.l1_portal_address,
        MESSAGE_PASSER_ADDRESS,
    );

    // First, find some withdrawals
    let current_block = l2_provider.get_block_number().await.unwrap();
    let from_block = current_block.saturating_sub(10_000);

    let withdrawals = state_provider
        .get_pending_withdrawals(from_block, current_block, config.eoa_address)
        .await
        .expect("Failed to scan withdrawals");

    if withdrawals.is_empty() {
        println!("⚠ No withdrawals found in range - skipping status query test");
        return;
    }

    println!("Found {} withdrawals to test", withdrawals.len());

    // Test querying status for each withdrawal
    for withdrawal in withdrawals.iter().take(5) {
        // Test first 5
        let status = state_provider
            .query_withdrawal_status(withdrawal.hash, config.eoa_address)
            .await
            .expect("Failed to query status");

        println!("Withdrawal {}: {:?}", withdrawal.hash, status);

        // Status should match what we got from get_pending_withdrawals
        match (&withdrawal.status, &status) {
            (WithdrawalStatus::Initiated, WithdrawalStatus::Initiated) => {}
            (
                WithdrawalStatus::Proven { timestamp: t1 },
                WithdrawalStatus::Proven { timestamp: t2 },
            ) => {
                assert_eq!(t1, t2, "Timestamps should match");
            }
            _ => panic!(
                "Status mismatch: expected {:?}, got {:?}",
                withdrawal.status, status
            ),
        }
    }

    println!("✓ Status queries successful");
}

#[tokio::test]
async fn test_is_finalized_check() {
    let config = load_test_config();

    println!("Testing is_finalized check");

    let l1_provider = setup_provider(&config.l1_rpc_url).await;
    let l2_provider = setup_provider(&config.l2_rpc_url).await;

    let state_provider = WithdrawalStateProvider::new(
        l1_provider,
        l2_provider.clone(),
        config.l1_portal_address,
        MESSAGE_PASSER_ADDRESS,
    );

    // Get some withdrawals
    let current_block = l2_provider.get_block_number().await.unwrap();
    let from_block = current_block.saturating_sub(10_000);

    let withdrawals = state_provider
        .get_pending_withdrawals(from_block, current_block, config.eoa_address)
        .await
        .expect("Failed to scan withdrawals");

    if withdrawals.is_empty() {
        println!("⚠ No withdrawals found - checking random hash");

        // Check a random hash (should not be finalized)
        let random_hash =
            alloy_primitives::b256!("0000000000000000000000000000000000000000000000000000000000000001");
        let is_finalized = state_provider
            .is_finalized(random_hash)
            .await
            .expect("Failed to check finalized status");

        assert!(!is_finalized, "Random hash should not be finalized");
        println!("✓ Random hash is not finalized (as expected)");
        return;
    }

    // Check finalized status for found withdrawals
    for withdrawal in withdrawals.iter().take(3) {
        let is_finalized = state_provider
            .is_finalized(withdrawal.hash)
            .await
            .expect("Failed to check finalized status");

        // These should NOT be finalized (since get_pending_withdrawals filters them out)
        assert!(
            !is_finalized,
            "Pending withdrawal should not be finalized: {}",
            withdrawal.hash
        );

        println!("✓ Withdrawal {} is not finalized", withdrawal.hash);
    }

    println!("✓ Finalized checks successful");
}

#[tokio::test]
async fn test_is_proven_check() {
    let config = load_test_config();

    println!("Testing is_proven check");

    let l1_provider = setup_provider(&config.l1_rpc_url).await;
    let l2_provider = setup_provider(&config.l2_rpc_url).await;

    let state_provider = WithdrawalStateProvider::new(
        l1_provider,
        l2_provider.clone(),
        config.l1_portal_address,
        MESSAGE_PASSER_ADDRESS,
    );

    // Get some withdrawals
    let current_block = l2_provider.get_block_number().await.unwrap();
    let from_block = current_block.saturating_sub(10_000);

    let withdrawals = state_provider
        .get_pending_withdrawals(from_block, current_block, config.eoa_address)
        .await
        .expect("Failed to scan withdrawals");

    if withdrawals.is_empty() {
        println!("⚠ No withdrawals found - checking random hash");

        // Check a random hash (should not be proven)
        let random_hash =
            alloy_primitives::b256!("0000000000000000000000000000000000000000000000000000000000000001");
        let is_proven = state_provider
            .is_proven(random_hash, config.eoa_address)
            .await
            .expect("Failed to check proven status");

        assert!(is_proven.is_none(), "Random hash should not be proven");
        println!("✓ Random hash is not proven (as expected)");
        return;
    }

    // Check proven status for found withdrawals
    for withdrawal in &withdrawals {
        let proven_result = state_provider
            .is_proven(withdrawal.hash, config.eoa_address)
            .await
            .expect("Failed to check proven status");

        match (&withdrawal.status, &proven_result) {
            (WithdrawalStatus::Initiated, None) => {
                println!("✓ Initiated withdrawal {} is not proven", withdrawal.hash);
            }
            (WithdrawalStatus::Proven { timestamp }, Some(proven)) => {
                assert_eq!(
                    timestamp, &proven.timestamp,
                    "Timestamp mismatch for withdrawal {}",
                    withdrawal.hash
                );
                println!(
                    "✓ Proven withdrawal {} has timestamp {}",
                    withdrawal.hash, proven.timestamp
                );
            }
            _ => panic!(
                "Status/proven mismatch for {}: status={:?}, proven={:?}",
                withdrawal.hash, withdrawal.status, proven_result
            ),
        }
    }

    println!("✓ Proven checks successful");
}
