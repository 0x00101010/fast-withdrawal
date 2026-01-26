//! Integration tests for in-flight deposit tracking.
//!
//! Tests the DepositStateProvider's ability to:
//! - Scan L1 for FundsDeposited events
//! - Scan L2 for FilledRelay events
//! - Correlate deposits with fills to determine in-flight status
//!
//! Run with:
//! ```bash
//! cargo test --package orchestrator --test inflight
//! ```

#[path = "setup.rs"]
mod setup;

use alloy_provider::Provider;
use deposit::{get_inflight_deposit_total, get_inflight_deposits, DepositStateProvider};
use setup::{load_test_config, setup_provider};

#[tokio::test]
async fn test_deposit_state_provider_creation() {
    let config = load_test_config();
    let network = config.network_config();

    println!("Creating deposit state provider");
    println!("L1 RPC: {}", config.l1_rpc_url);
    println!("L2 RPC: {}", config.l2_rpc_url);
    println!("L1 SpokePool: {}", network.ethereum.spoke_pool);
    println!("L2 SpokePool: {}", network.unichain.spoke_pool);

    let l1_provider = setup_provider(&config.l1_rpc_url).await;
    let l2_provider = setup_provider(&config.l2_rpc_url).await;

    let _state_provider = DepositStateProvider::new(
        l1_provider,
        l2_provider,
        network.ethereum.spoke_pool,
        network.unichain.spoke_pool,
    );

    println!("✓ Deposit state provider created successfully");
}

#[tokio::test]
async fn test_get_inflight_deposits_no_deposits() {
    let config = load_test_config();
    let network = config.network_config();

    println!("Testing in-flight deposit scan with no expected deposits");

    let l1_provider = setup_provider(&config.l1_rpc_url).await;
    let l2_provider = setup_provider(&config.l2_rpc_url).await;

    // Use a random address that likely has no deposits
    let random_depositor = alloy_primitives::address!("0000000000000000000000000000000000000001");

    let inflight = get_inflight_deposits(
        l1_provider,
        l2_provider,
        network.ethereum.spoke_pool,
        network.unichain.spoke_pool,
        random_depositor,
        network.unichain.chain_id,
        network.ethereum.chain_id,
        3600, // 1 hour lookback
        network.ethereum.block_time_secs,
        network.unichain.block_time_secs,
    )
    .await
    .expect("Failed to get in-flight deposits");

    println!("Found {} in-flight deposits (expected 0)", inflight.len());
    assert!(
        inflight.is_empty(),
        "Random address should have no deposits"
    );

    println!("✓ No deposits found for random address (as expected)");
}

#[tokio::test]
async fn test_get_inflight_deposits_scan() {
    let config = load_test_config();
    let network = config.network_config();

    println!("Testing in-flight deposit scan for configured EOA");
    println!("EOA: {}", config.eoa_address);
    println!("Destination chain: {}", network.unichain.chain_id);

    let l1_provider = setup_provider(&config.l1_rpc_url).await;
    let l2_provider = setup_provider(&config.l2_rpc_url).await;

    // Get current block numbers for reference
    let l1_block = l1_provider.get_block_number().await.unwrap();
    let l2_block = l2_provider.get_block_number().await.unwrap();
    println!("L1 current block: {}", l1_block);
    println!("L2 current block: {}", l2_block);

    // Use 12 hour lookback (matches default config)
    let lookback_secs = 43200;
    let l1_lookback_blocks = lookback_secs / network.ethereum.block_time_secs;
    let l2_lookback_blocks = lookback_secs / network.unichain.block_time_secs;

    println!(
        "Lookback: {} seconds ({} L1 blocks, {} L2 blocks)",
        lookback_secs, l1_lookback_blocks, l2_lookback_blocks
    );

    let inflight = get_inflight_deposits(
        l1_provider,
        l2_provider,
        network.ethereum.spoke_pool,
        network.unichain.spoke_pool,
        config.eoa_address,
        network.unichain.chain_id,
        network.ethereum.chain_id,
        lookback_secs,
        network.ethereum.block_time_secs,
        network.unichain.block_time_secs,
    )
    .await
    .expect("Failed to get in-flight deposits");

    println!("Found {} in-flight deposits", inflight.len());

    for deposit in &inflight {
        println!("  Deposit ID: {}", deposit.deposit_id);
        println!("    Amount: {} wei", deposit.input_amount);
        println!("    L1 Block: {}", deposit.block_number);
        println!(
            "    Origin -> Dest: {} -> {}",
            deposit.origin_chain_id, deposit.destination_chain_id
        );
    }

    println!("✓ In-flight deposit scan completed");
}

#[tokio::test]
async fn test_get_inflight_deposit_total() {
    let config = load_test_config();
    let network = config.network_config();

    println!("Testing in-flight deposit total calculation");

    let l1_provider = setup_provider(&config.l1_rpc_url).await;
    let l2_provider = setup_provider(&config.l2_rpc_url).await;

    let total = get_inflight_deposit_total(
        l1_provider,
        l2_provider,
        network.ethereum.spoke_pool,
        network.unichain.spoke_pool,
        config.eoa_address,
        network.unichain.chain_id,
        network.ethereum.chain_id,
        43200, // 12 hours
        network.ethereum.block_time_secs,
        network.unichain.block_time_secs,
    )
    .await
    .expect("Failed to get in-flight deposit total");

    println!("Total in-flight: {} wei", total);

    // Convert to ETH for readability
    let total_eth = alloy_primitives::utils::format_ether(total);
    println!("Total in-flight: {} ETH", total_eth);

    println!("✓ In-flight deposit total calculated");
}

#[tokio::test]
async fn test_lookback_calculation() {
    let config = load_test_config();
    let network = config.network_config();

    println!("Testing lookback calculation");

    // 12 hours = 43200 seconds
    let lookback_secs: u64 = 43200;

    // L1 (Ethereum): 12 second blocks
    let l1_blocks = lookback_secs / network.ethereum.block_time_secs;
    println!(
        "L1 lookback: {} seconds / {} block_time = {} blocks",
        lookback_secs, network.ethereum.block_time_secs, l1_blocks
    );
    assert_eq!(l1_blocks, 3600); // 43200 / 12 = 3600 blocks

    // L2 (Unichain): 1 second blocks
    let l2_blocks = lookback_secs / network.unichain.block_time_secs;
    println!(
        "L2 lookback: {} seconds / {} block_time = {} blocks",
        lookback_secs, network.unichain.block_time_secs, l2_blocks
    );
    assert_eq!(l2_blocks, 43200); // 43200 / 1 = 43200 blocks

    println!("✓ Lookback calculations correct");
}

#[tokio::test]
#[ignore = "slow test - scans large block range"]
async fn test_long_lookback_scan_slow() {
    let config = load_test_config();
    let network = config.network_config();

    println!("Testing long lookback scan (24 hours)");

    let l1_provider = setup_provider(&config.l1_rpc_url).await;
    let l2_provider = setup_provider(&config.l2_rpc_url).await;

    // 24 hour lookback
    let lookback_secs = 86400 * 7;
    let l1_blocks = lookback_secs / network.ethereum.block_time_secs;
    let l2_blocks = lookback_secs / network.unichain.block_time_secs;

    println!("Scanning {} L1 blocks, {} L2 blocks", l1_blocks, l2_blocks);

    let inflight = get_inflight_deposits(
        l1_provider,
        l2_provider,
        network.ethereum.spoke_pool,
        network.unichain.spoke_pool,
        config.eoa_address,
        network.unichain.chain_id,
        network.ethereum.chain_id,
        lookback_secs,
        network.ethereum.block_time_secs,
        network.unichain.block_time_secs,
    )
    .await
    .expect("Failed to get in-flight deposits");

    println!(
        "Found {} in-flight deposits in last {} seconds",
        inflight.len(),
        lookback_secs,
    );

    for deposit in &inflight {
        println!("  Deposit ID: {}", deposit.deposit_id);
        println!("    Amount: {} wei", deposit.input_amount);
        println!("    L1 Block: {}", deposit.block_number);
        println!(
            "    Origin -> Dest: {} -> {}",
            deposit.origin_chain_id, deposit.destination_chain_id
        );
    }

    println!("✓ Long lookback scan completed");
}
