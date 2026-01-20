//! Integration tests for deposit actions.
//!
//! Tests deposit functionality using Sepolia testnet configuration.
//!
//! Run with:
//! ```bash
//! cargo test --package orchestrator --test deposit
//! ```
#[path = "setup.rs"]
mod setup;

use action::{
    deposit::{DepositAction, DepositConfig},
    Action,
};
use alloy_primitives::{Address, Bytes, U256};
use alloy_provider::Provider;
use config::NetworkConfig;
use setup::{load_test_config, setup_wallet_provider};

/// Helper to create a test deposit config for Ethereum Sepolia -> Unichain Sepolia
fn create_test_deposit_config(depositor: Address) -> DepositConfig {
    let network_config = NetworkConfig::sepolia();

    // Use small amounts for testing
    let input_amount = U256::from(1_000_000); // 1M wei = 0.000001 ETH (very small amount)
    let output_amount = U256::from(1_000_000); // 99% of input (1% fee estimate)

    DepositConfig {
        spoke_pool: network_config.ethereum.spoke_pool,
        depositor,
        recipient: depositor,                       // Send to self for testing
        input_token: network_config.ethereum.weth,  // WETH on Ethereum
        output_token: network_config.unichain.weth, // WETH on Unichain
        input_amount,
        output_amount,
        destination_chain_id: network_config.unichain.chain_id,
        exclusive_relayer: Address::ZERO, // No exclusive relayer
        fill_deadline: 0, // explicitly request slow fill
        exclusivity_parameter: 0, // No exclusivity period
        message: Bytes::new(),
    }
}

/// Common test setup: load config and create provider
async fn setup_provider() -> impl Provider + Clone {
    let config = load_test_config();

    client::create_provider(&config.l1_rpc_url)
        .await
        .expect("Failed to create L1 provider")
}

#[tokio::test]
async fn test_deposit_action_creation() {
    let config = load_test_config();
    let network_config = NetworkConfig::sepolia();

    println!("Testing deposit action creation");
    println!("Network: Sepolia");
    println!("Ethereum SpokePool: {}", network_config.ethereum.spoke_pool);
    println!("Destination Chain ID: {}", network_config.unichain.chain_id);
    println!("Test Depositor: {}", config.eoa_address);

    let provider = setup_provider().await;

    // Create deposit config
    let deposit_config = create_test_deposit_config(config.eoa_address);

    // Create deposit action
    let action = DepositAction::new(provider, deposit_config);

    // Test is_ready
    let is_ready = action.is_ready();
    println!("✓ Deposit action created");
    println!("  Is ready: {}", is_ready);

    assert!(is_ready, "Deposit action should be ready with valid config");
}

#[tokio::test]
async fn test_deposit_action_validation() {
    let config = load_test_config();
    let provider = setup_provider().await;

    println!("Testing deposit action validation");

    // Test invalid config: zero spoke pool
    let mut invalid_config = create_test_deposit_config(config.eoa_address);
    invalid_config.spoke_pool = Address::ZERO;

    let action = DepositAction::new(provider.clone(), invalid_config);
    assert!(
        !action.is_ready(),
        "Should not be ready with zero spoke pool"
    );

    // Test invalid config: zero recipient
    let mut invalid_config = create_test_deposit_config(config.eoa_address);
    invalid_config.recipient = Address::ZERO;

    let action = DepositAction::new(provider.clone(), invalid_config);
    assert!(
        !action.is_ready(),
        "Should not be ready with zero recipient"
    );

    // Test invalid config: zero amount
    let mut invalid_config = create_test_deposit_config(config.eoa_address);
    invalid_config.input_amount = U256::ZERO;

    let action = DepositAction::new(provider.clone(), invalid_config);
    assert!(!action.is_ready(), "Should not be ready with zero amount");

    // Test invalid config: output > input
    let mut invalid_config = create_test_deposit_config(config.eoa_address);
    invalid_config.input_amount = U256::from(100);
    invalid_config.output_amount = U256::from(200);

    let action = DepositAction::new(provider, invalid_config);
    assert!(
        !action.is_ready(),
        "Should not be ready when output exceeds input"
    );

    println!("✓ All validation checks passed");
}

#[tokio::test]
async fn test_deposit_action_description() {
    let config = load_test_config();
    let provider = setup_provider().await;

    println!("Testing deposit action description");

    // Create deposit config
    let deposit_config = create_test_deposit_config(config.eoa_address);
    let dest_chain = deposit_config.destination_chain_id;

    // Create deposit action
    let action = DepositAction::new(provider, deposit_config);

    // Get description
    let description = action.description();
    println!("✓ Description: {}", description);

    assert!(description.contains("Deposit"));
    assert!(description.contains("ETH"));
    assert!(description.contains(&dest_chain.to_string()));
}

#[tokio::test]
async fn test_deposit_action_is_completed() {
    let config = load_test_config();
    let provider = setup_provider().await;

    println!("Testing deposit action is_completed check");

    // Create deposit config
    let deposit_config = create_test_deposit_config(config.eoa_address);

    // Create deposit action
    let action = DepositAction::new(provider, deposit_config);

    // Check if completed (should be false since we haven't executed)
    let is_completed = action
        .is_completed()
        .await
        .expect("Failed to check is_completed");

    println!("✓ Is completed: {}", is_completed);

    // For now, is_completed always returns false (stub implementation)
    assert!(!is_completed);
}

#[tokio::test]
#[ignore = "requires real funds and submits actual transaction - run with: just test-ignored"]
async fn test_deposit_action_execute() {
    let config = load_test_config();

    println!("⚠️  WARNING: This test will execute a REAL deposit transaction!");
    println!("Setting up wallet provider for transaction signing...");

    // Use wallet provider instead of read-only provider
    let provider = setup_wallet_provider().await;

    println!("\nTest Depositor: {}", config.eoa_address);
    println!("Make sure the depositor has sufficient ETH for the deposit + gas");

    // Create deposit config
    let deposit_config = create_test_deposit_config(config.eoa_address);

    println!("\nDeposit Details:");
    println!("  SpokePool: {}", deposit_config.spoke_pool);
    println!("  Depositor: {}", deposit_config.depositor);
    println!("  Recipient: {}", deposit_config.recipient);
    println!("  Input Token: {}", deposit_config.input_token);
    println!("  Output Token: {}", deposit_config.output_token);
    println!("  Input Amount: {} wei", deposit_config.input_amount);
    println!("  Output Amount: {} wei", deposit_config.output_amount);
    println!(
        "  Destination Chain: {}",
        deposit_config.destination_chain_id
    );
    println!(
        "  Fill Deadline: {} (unix timestamp)",
        deposit_config.fill_deadline
    );
    println!(
        "  Exclusivity Parameter: {}",
        deposit_config.exclusivity_parameter
    );

    // Get current timestamp for comparison
    let current_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    println!(
        "\nCurrent Timestamp: {} (unix timestamp)",
        current_timestamp
    );
    println!(
        "Time until deadline: {} seconds",
        deposit_config
            .fill_deadline
            .saturating_sub(current_timestamp)
    );

    // Create deposit action
    let action = DepositAction::new(provider, deposit_config);

    // Verify action is ready
    assert!(action.is_ready(), "Deposit action should be ready");

    // Execute the deposit
    println!("\nExecuting deposit transaction...");
    let result = match action.execute().await {
        Ok(result) => result,
        Err(e) => {
            eprintln!("\n❌ Failed to execute deposit:");
            eprintln!("Error: {}", e);
            eprintln!("\nError chain:");
            for (i, cause) in e.chain().enumerate() {
                eprintln!("  {}: {}", i, cause);
            }
            eprintln!("\nDebug representation:");
            eprintln!("{:?}", e);
            panic!("Deposit execution failed - see error details above");
        }
    };

    println!("\n✓ Deposit executed successfully!");
    println!("  Transaction Hash: {:?}", result.tx_hash);
    println!("  Block Number: {:?}", result.block_number);
    println!("  Gas Used: {:?}", result.gas_used);

    // Verify transaction was successful
    assert!(
        result.block_number.is_some(),
        "Transaction should be included in a block"
    );
}
