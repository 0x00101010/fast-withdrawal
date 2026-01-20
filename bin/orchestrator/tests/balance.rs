//! Integration tests for balance monitoring.
//!
//! These tests require a test configuration file at `test-config.toml` in the repository root.
//! See `test-config.toml` for an example.
//!
//! Run with:
//! ```bash
//! cargo test --package orchestrator --test integration_test
//! ```

#[path = "setup.rs"]
mod setup;

use alloy_primitives::Address;
use balance::monitor::BalanceMonitor;
use orchestrator::{check_l1_native_balance, check_l2_spoke_pool_balance};
use setup::load_test_config;

#[tokio::test]
async fn test_l1_native_balance_query() {
    let config = load_test_config();

    println!("Testing L1 native balance query");
    println!("L1 RPC: {}", config.l1_rpc_url);
    println!("L1 EOA: {}", config.eoa_address);

    // Create provider and monitor
    let provider = client::create_provider(&config.l1_rpc_url)
        .await
        .expect("Failed to create L1 provider");

    let monitor = BalanceMonitor::new(provider);

    // Query balance
    let result = check_l1_native_balance(&monitor, config.eoa_address)
        .await
        .expect("Failed to query L1 native balance");

    println!("✓ L1 Native Balance:");
    println!("  Address: {}", result.holder);
    println!("  Balance: {} wei", result.amount);

    // Assertions
    assert_eq!(result.holder, config.eoa_address);
    // Balance could be zero, but the query should succeed
}

#[tokio::test]
async fn test_l2_spokepool_balance_query() {
    let config = load_test_config();

    // Using zero address as token for now - update config to add token field if needed
    let token = Address::ZERO;

    println!("Testing L2 SpokePool balance query");
    println!("L2 RPC: {}", config.l2_rpc_url);
    println!("SpokePool: {}", config.l2_spoke_pool_address);
    println!("Token: {}", token);
    println!("Relayer: {}", config.eoa_address);

    // Create provider and monitor
    let provider = client::create_provider(&config.l2_rpc_url)
        .await
        .expect("Failed to create L2 provider");

    let monitor = BalanceMonitor::new(provider);

    // Query balance
    let result = check_l2_spoke_pool_balance(
        &monitor,
        config.l2_spoke_pool_address,
        token,
        config.eoa_address,
    )
    .await
    .expect("Failed to query L2 SpokePool balance");

    println!("✓ L2 SpokePool Balance:");
    println!("  SpokePool: {}", config.l2_spoke_pool_address);
    println!("  Token: {}", result.asset);
    println!("  Relayer: {}", result.holder);
    println!("  Balance: {}", result.amount);

    // Assertions
    assert_eq!(result.holder, config.eoa_address);
    assert_eq!(result.asset, token);
    // Balance could be zero, but the query should succeed
}

#[tokio::test]
async fn test_both_chains_integration() {
    let config = load_test_config();

    println!("Testing full integration with both L1 and L2");

    let token = Address::ZERO;

    // Create L1 provider and monitor
    let l1_provider = client::create_provider(&config.l1_rpc_url)
        .await
        .expect("Failed to create L1 provider");
    let l1_monitor = BalanceMonitor::new(l1_provider);

    // Create L2 provider and monitor
    let l2_provider = client::create_provider(&config.l2_rpc_url)
        .await
        .expect("Failed to create L2 provider");
    let l2_monitor = BalanceMonitor::new(l2_provider);

    // Query both balances
    let l1_result = check_l1_native_balance(&l1_monitor, config.eoa_address)
        .await
        .expect("Failed to query L1 balance");

    let l2_result = check_l2_spoke_pool_balance(
        &l2_monitor,
        config.l2_spoke_pool_address,
        token,
        config.eoa_address,
    )
    .await
    .expect("Failed to query L2 balance");

    println!("✓ Integration test complete");
    println!("  L1 Balance: {} wei", l1_result.amount);
    println!("  L2 SpokePool Balance: {}", l2_result.amount);

    // Both queries should succeed
    assert_eq!(l1_result.holder, config.eoa_address);
    assert_eq!(l2_result.holder, config.eoa_address);
}
