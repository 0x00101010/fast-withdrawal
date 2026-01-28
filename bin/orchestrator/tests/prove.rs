//! Integration tests for prove action.
//!
//! Tests the ProveAction's ability to:
//! - Find a withdrawal transaction on L2
//! - Generate storage proof and submit to L1
//! - Execute real prove transaction

use crate::setup::{load_test_config, setup_provider, setup_signer};
use action::{
    prove::{Prove, ProveAction},
    Action,
};
use alloy_provider::Provider;
use alloy_rpc_types_eth::BlockNumberOrTag;
use binding::opstack::MESSAGE_PASSER_ADDRESS;
use withdrawal::{state::WithdrawalStateProvider, types::WithdrawalStatus};

#[path = "setup.rs"]
mod setup;

/// Test executing prove action for a real pending withdrawal
///
/// This test:
/// 1. Scans L2 for pending withdrawals for the configured EOA
/// 2. Picks the most recent initiated withdrawal
/// 3. Creates a ProveAction and executes it
/// 4. Submits the prove transaction to L1
#[tokio::test]
#[ignore = "requires real pending withdrawals onchain and submits actual transaction - run with: just run-prove"]
async fn test_prove_action_execute() {
    // Initialize tracing for debug logs
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let config = load_test_config();

    println!("Testing prove action execution");
    println!("L1 RPC: {}", config.l1_rpc_url);
    println!("L2 RPC: {}", config.l2_rpc_url);
    println!("Portal: {}", config.network_config().unichain.l1_portal);
    println!(
        "Factory: {}",
        config.network_config().unichain.l1_dispute_game_factory
    );
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
    let from_block = current_block.saturating_sub(600_000); // almost a week

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

    // Find the most recent initiated withdrawal
    let initiated_withdrawal = withdrawals
        .iter()
        .rev()
        .find(|w| matches!(w.status, WithdrawalStatus::Initiated));

    if initiated_withdrawal.is_none() {
        println!("⚠ No initiated withdrawals found - cannot test prove action");
        println!("  Create a withdrawal on L2 and wait a few minutes, then run this test");
        return;
    }

    let withdrawal = initiated_withdrawal.unwrap();
    println!("\nProving withdrawal:");
    println!("  Hash: {}", withdrawal.hash);
    println!("  L2 Block: {}", withdrawal.l2_block);
    println!("  Sender: {}", withdrawal.transaction.sender);
    println!("  Target: {}", withdrawal.transaction.target);
    println!("  Value: {}", withdrawal.transaction.value);

    // Create prove action
    let prove = Prove {
        portal_address: config.network_config().unichain.l1_portal,
        factory_address: config.network_config().unichain.l1_dispute_game_factory,
        withdrawal: withdrawal.transaction.clone(),
        withdrawal_hash: withdrawal.hash,
        l2_block: withdrawal.l2_block,
        from: config.eoa_address,
    };

    let mut action = ProveAction::new(l1_provider, l2_provider, l1_signer, prove);

    // Check if ready
    println!("\nChecking if action is ready...");
    match action.is_ready().await {
        Ok(true) => println!("✓ Action is ready"),
        Ok(false) => {
            println!("✗ Action is not ready (withdrawal may already be proven)");
            return;
        }
        Err(e) => {
            println!("✗ Failed to check readiness: {}", e);
            return;
        }
    }

    // Execute the prove action
    println!("\nExecuting prove action...");
    let result = action
        .execute()
        .await
        .expect("Failed to execute prove action");

    println!("✓ Successfully proved withdrawal on L1!");
    println!("  Transaction hash: {}", result.tx_hash);
    println!("  Block number: {:?}", result.block_number);
    println!("  Gas used: {:?}", result.gas_used);

    // Verify it's now completed
    let completed = action
        .is_completed()
        .await
        .expect("Failed to check completion");
    assert!(completed, "Action should be completed after execution");
    println!("✓ Withdrawal is now proven on L1");
}

/// Debug test to validate output root proof construction
///
/// This test helps diagnose InvalidOutputRootProof errors by:
/// 1. Generating the proof
/// 2. Fetching the dispute game's root claim
/// 3. Computing the hash of our output root proof
/// 4. Comparing them to see if they match
#[tokio::test]
#[ignore = "requires inflight withdrawal and dispute game created"]
async fn test_debug_output_root_proof() {
    use alloy_primitives::keccak256;
    use binding::opstack::{IDisputeGameFactory, IFaultDisputeGame};
    use withdrawal::proof::generate_proof;

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

    let withdrawal = withdrawals
        .iter()
        .rev()
        .find(|w| matches!(w.status, WithdrawalStatus::Initiated))
        .expect("No initiated withdrawal found");

    println!("Withdrawal hash: {}", withdrawal.hash);
    println!("Withdrawal L2 block: {}", withdrawal.l2_block);

    // Generate proof
    let proof_params = generate_proof(
        &l1_provider,
        &l2_provider,
        config.network_config().unichain.l1_portal,
        config.network_config().unichain.l1_dispute_game_factory,
        withdrawal.hash,
        withdrawal.transaction.clone(),
        withdrawal.l2_block,
    )
    .await
    .expect("Failed to generate proof");

    println!("\n=== Output Root Proof ===");
    println!("Version: {}", proof_params.output_root_proof.version);
    println!("State Root: {}", proof_params.output_root_proof.stateRoot);
    println!(
        "Message Passer Storage Root: {}",
        proof_params.output_root_proof.messagePasserStorageRoot
    );
    println!(
        "Latest Block Hash: {}",
        proof_params.output_root_proof.latestBlockhash
    );

    // Compute hash of output root proof (same as Hashing.hashOutputRootProof in Solidity)
    // outputRoot = keccak256(abi.encode(version, stateRoot, messagePasserStorageRoot, latestBlockhash))
    let mut encoded = Vec::with_capacity(128);
    encoded.extend_from_slice(proof_params.output_root_proof.version.as_slice());
    encoded.extend_from_slice(proof_params.output_root_proof.stateRoot.as_slice());
    encoded.extend_from_slice(
        proof_params
            .output_root_proof
            .messagePasserStorageRoot
            .as_slice(),
    );
    encoded.extend_from_slice(proof_params.output_root_proof.latestBlockhash.as_slice());
    let computed_output_root = keccak256(&encoded);

    println!("\n=== Computed Output Root ===");
    println!("Computed: {}", computed_output_root);

    // Get the dispute game's root claim
    println!("\n=== Dispute Game Info ===");
    println!("Dispute game index: {}", proof_params.dispute_game_index);

    // Get game address from factory
    let factory = IDisputeGameFactory::new(
        config.network_config().unichain.l1_dispute_game_factory,
        &l1_provider,
    );
    let game_info = factory
        .gameAtIndex(proof_params.dispute_game_index)
        .call()
        .await
        .expect("Failed to get game at index");
    println!("Game type: {}", game_info.gameType_);
    println!("Game timestamp: {}", game_info.timestamp_);
    println!("Game address: {}", game_info.proxy_);

    let game = IFaultDisputeGame::new(game_info.proxy_, &l1_provider);
    let root_claim = game
        .rootClaim()
        .call()
        .await
        .expect("Failed to get root claim");
    let game_l2_block = game
        .l2BlockNumber()
        .call()
        .await
        .expect("Failed to get L2 block");

    println!("Game L2 block: {}", game_l2_block);
    println!("Root claim: {}", root_claim);

    println!("\n=== Comparison ===");
    println!("Computed output root: {}", computed_output_root);
    println!("Game root claim:      {}", root_claim);

    if computed_output_root == root_claim {
        println!("✓ MATCH - Output root proof is valid");
    } else {
        println!("✗ MISMATCH - Output root proof is invalid!");
        println!("\nThe output root proof components don't hash to the game's root claim.");
        println!("This could mean:");
        println!(
            "  1. We're using the wrong L2 block's state (should use game's L2 block, not withdrawal's)"
        );
        println!("  2. The state root or block hash is incorrect");
        println!("  3. The message passer storage root is incorrect");
    }

    assert_eq!(
        computed_output_root, root_claim,
        "Output root proof must match game's root claim"
    );
}

/// Test that compute_storage_slot produces valid storage slots
#[tokio::test]
async fn test_compute_storage_slot() {
    use alloy_primitives::B256;
    use withdrawal::proof::compute_storage_slot;

    println!("Testing compute_storage_slot");

    let withdrawal_hash = B256::from([1u8; 32]);
    let slot = compute_storage_slot(withdrawal_hash);

    // Verify it's deterministic
    let slot2 = compute_storage_slot(withdrawal_hash);
    assert_eq!(slot, slot2, "Storage slot should be deterministic");

    // Verify different hashes produce different slots
    let other_hash = B256::from([2u8; 32]);
    let other_slot = compute_storage_slot(other_hash);
    assert_ne!(
        slot, other_slot,
        "Different hashes should produce different slots"
    );

    // Verify it's not zero
    assert_ne!(
        slot,
        B256::ZERO,
        "Storage slot should not be zero for non-zero hash"
    );

    println!("✓ Storage slot computation works correctly");
}
