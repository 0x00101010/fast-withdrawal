use crate::setup::{load_test_config, mock_signer, setup_provider, setup_signer};
use action::{
    withdraw::{Withdraw, WithdrawAction},
    Action,
};
use alloy_primitives::{Address, Bytes, U256};
use alloy_provider::Provider;
use binding::opstack::MESSAGE_PASSER_ADDRESS;

#[path = "setup.rs"]
mod setup;

fn create_test_withdrawal(source: Address, target: Address) -> Withdraw {
    let value = U256::from(1_000_000);
    let gas_limit = U256::from(200_000); // Seems to be common with good buffer

    Withdraw {
        contract: MESSAGE_PASSER_ADDRESS,
        source,
        target,
        value,
        gas_limit,
        data: Bytes::new(),
        tx_hash: None,
    }
}

#[tokio::test]
async fn test_withdraw_action_creation() {
    let config = load_test_config();

    println!("Testing withdrawal action creation");
    println!("Network: Unichain Sepolia → Ethereum Sepolia");
    println!("L2ToL1MessagePasser: {}", MESSAGE_PASSER_ADDRESS);
    println!("Test Source: {}", config.eoa_address);
    println!("Test Target: {}", config.eoa_address);

    let provider = setup_provider(&config.l2_rpc_url).await;
    let withdraw = create_test_withdrawal(config.eoa_address, config.eoa_address);
    let action = WithdrawAction::new(provider, mock_signer(), withdraw);

    let is_ready = action.is_ready().await.expect("Failed to check is_ready");
    assert!(is_ready);
}

#[tokio::test]
async fn test_withdraw_action_validation() {
    let config = load_test_config();
    let provider = setup_provider(&config.l2_rpc_url).await;

    println!("Testing withdrawal action validation");

    // Test valid config
    let valid_config = create_test_withdrawal(config.eoa_address, config.eoa_address);
    let action = WithdrawAction::new(provider.clone(), mock_signer(), valid_config);

    // Should not panic when checking is_ready
    let ready = action
        .is_ready()
        .await
        .expect("Failed to check is_ready for valid config");
    assert!(ready);

    // Test invalid config: zero target
    let mut invalid_config = create_test_withdrawal(config.eoa_address, Address::ZERO);
    let action = WithdrawAction::new(provider.clone(), mock_signer(), invalid_config.clone());

    // Should still work - validation happens in is_ready, not construction
    let ready = action.is_ready().await.expect("Failed to check is_ready");
    assert!(!ready);

    // Test invalid config: zero value
    invalid_config.value = U256::ZERO;
    let action = WithdrawAction::new(provider.clone(), mock_signer(), invalid_config);
    let ready = action.is_ready().await.expect("Failed to check is_ready");
    assert!(!ready);
}

#[tokio::test]
async fn test_withdraw_action_is_completed() {
    let config = load_test_config();
    let provider = setup_provider(&config.l2_rpc_url).await;

    println!("Testing withdrawal action is_completed check");

    // Create withdrawal config
    let withdraw_config = create_test_withdrawal(config.eoa_address, config.eoa_address);

    // Create withdrawal action
    let action = WithdrawAction::new(provider, mock_signer(), withdraw_config);

    // Check if completed (should be false for random parameters we just created)
    let is_completed = action
        .is_completed()
        .await
        .expect("Failed to check is_completed");

    // Should not be completed as it's not executed
    assert!(!is_completed);
}

#[tokio::test]
async fn test_withdraw_action_is_ready_checks_balance() {
    let config = load_test_config();
    let provider = setup_provider(&config.l2_rpc_url).await;

    println!("Testing withdrawal action is_ready balance check");

    // Get actual balance of test address
    let balance = provider
        .get_balance(config.eoa_address)
        .await
        .expect("Failed to get balance");
    println!("Test address balance: {} wei", balance);

    // Create withdrawal with amount LESS than balance
    let mut withdraw_config = create_test_withdrawal(config.eoa_address, config.eoa_address);

    if balance > U256::ZERO {
        // Set amount to half of balance
        withdraw_config.value = balance / U256::from(2);
        let action = WithdrawAction::new(provider.clone(), mock_signer(), withdraw_config.clone());

        let is_ready = action.is_ready().await.expect("Failed to check is_ready");
        println!(
            "✓ With sufficient balance ({}): is_ready = {}",
            withdraw_config.value, is_ready
        );
        assert!(
            is_ready,
            "Should be ready when source has sufficient balance"
        );
    } else {
        println!("⚠ Test address has zero balance, skipping positive balance test");
    }

    // Create withdrawal with amount MORE than balance
    withdraw_config.value = balance + U256::from(1_000_000);
    let action = WithdrawAction::new(provider, mock_signer(), withdraw_config.clone());

    let is_ready = action.is_ready().await.expect("Failed to check is_ready");
    println!(
        "✓ With insufficient balance ({}): is_ready = {}",
        withdraw_config.value, is_ready
    );
    assert!(
        !is_ready,
        "Should not be ready when source has insufficient balance"
    );
}

#[tokio::test]
#[ignore = "requires real funds and submits actual transaction - run with: just run-withdraw"]
async fn test_withdraw_action_execute() {
    let config = load_test_config();
    let network_config = config.network_config();

    println!("⚠️  WARNING: This test will execute a REAL withdrawal transaction!");
    println!("This will initiate an L2→L1 withdrawal that takes 7 days to finalize.");
    println!("Setting up provider and signer for transaction signing...");

    // Use provider and signer
    let provider = setup_provider(&config.l2_rpc_url).await;
    let signer = setup_signer(network_config.unichain.chain_id, provider.clone());

    println!("\nTest Source (L2): {}", config.eoa_address);
    println!("Test Target (L1): {}", config.eoa_address);
    println!("Make sure the source has sufficient ETH on L2 for the withdrawal + gas");

    // Create withdrawal config (withdraw to self)
    let withdraw = create_test_withdrawal(config.eoa_address, config.eoa_address);

    println!("\nWithdrawal Details:");
    println!("  L2ToL1MessagePasser: {}", withdraw.contract);
    println!("  Source (L2): {}", withdraw.source);
    println!("  Target (L1): {}", withdraw.target);
    println!("  Value: {} wei", withdraw.value);
    println!("  Gas Limit (L1): {}", withdraw.gas_limit);
    println!("  Data: {} bytes", withdraw.data.len());

    // Create withdrawal action
    let mut action = WithdrawAction::new(provider, signer, withdraw);

    // Verify action is ready
    let is_ready = action.is_ready().await.expect("Failed to check is_ready");
    assert!(
        is_ready,
        "Withdrawal action should be ready. Make sure source has sufficient balance."
    );
    println!("✓ Action is ready");

    // Check if already completed
    let was_completed = action
        .is_completed()
        .await
        .expect("Failed to check is_completed");
    assert!(!was_completed);

    // Execute the withdrawal
    println!("\nExecuting withdrawal transaction...");
    let result = match action.execute().await {
        Ok(result) => result,
        Err(e) => {
            eprintln!("\n❌ Failed to execute withdrawal:");
            eprintln!("Error: {}", e);
            eprintln!("\nError chain:");
            for (i, cause) in e.chain().enumerate() {
                eprintln!("  {}: {}", i, cause);
            }
            eprintln!("\nDebug representation:");
            eprintln!("{:?}", e);
            panic!("Withdrawal execution failed - see error details above");
        }
    };

    println!("\n✓ Withdrawal initiated successfully!");
    println!("  Transaction Hash: {:?}", result.tx_hash);
    println!("  Block Number: {:?}", result.block_number);
    println!("  Gas Used: {:?}", result.gas_used);

    // Verify transaction was successful
    assert!(
        result.block_number.is_some(),
        "Transaction should be included in a block"
    );

    // Verify withdrawal is now marked as completed
    let is_completed = action
        .is_completed()
        .await
        .expect("Failed to check is_completed after execution");
    assert!(
        is_completed,
        "Withdrawal should be marked as completed after execution"
    );
    println!("✓ Withdrawal marked as completed");
}
