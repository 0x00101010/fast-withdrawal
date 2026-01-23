//! Proof generation for L2→L1 withdrawals.
//!
//! This module generates the cryptographic proofs required to prove a withdrawal
//! on L1 using the OP Stack's fault proof system.

use crate::types::WithdrawalHash;
use alloy_contract::private::Provider;
use alloy_primitives::{keccak256, Address, BlockNumber, Bytes, B256, U256};
use alloy_rpc_types_eth::BlockNumberOrTag;
use binding::opstack::{
    IDisputeGameFactory, IFaultDisputeGame, IOptimismPortal2, OutputRootProof,
    WithdrawalTransaction, MESSAGE_PASSER_ADDRESS, OUTPUT_VERSION_V0,
};
use eyre::{eyre, Result};
use tracing::debug;

/// Parameters required to prove a withdrawal on L1.
#[derive(Debug, Clone)]
pub struct ProveWithdrawalParams {
    pub withdrawal: WithdrawalTransaction,
    pub dispute_game_index: U256,
    pub output_root_proof: OutputRootProof,
    pub withdrawal_proof: Vec<Bytes>,
}

/// Generate proof for a withdrawal that was initiated on L2.
///
/// This function:
/// 1. Fetches the withdrawal transaction receipt from L2
/// 2. Gets the L2 block header containing the withdrawal
/// 3. Finds a finalized dispute game covering this block
/// 4. Generates a Merkle proof that the withdrawal exists in L2 state
/// 5. Builds the output root proof structure
///
/// # Arguments
/// * `l1_provider` - Provider for L1 queries (dispute game, portal)
/// * `l2_provider` - Provider for L2 queries (receipt, block, proof)
/// * `withdrawal_tx_hash` - Transaction hash of the initiateWithdrawal call on L2
/// * `portal_address` - Address of OptimismPortal2 on L1
/// * `factory_address` - Address of DisputeGameFactory on L1
pub async fn generate_proof<P1, P2>(
    l1_provider: &P1,
    l2_provider: &P2,
    portal_address: Address,
    factory_address: Address,
    withdrawal_hash: WithdrawalHash,
    withdrawal: WithdrawalTransaction,
    block_number: BlockNumber,
) -> Result<ProveWithdrawalParams>
where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    // 1. Find a dispute game covering the withdrawal block
    debug!(
        withdrawal_block = block_number,
        "Finding dispute game covering withdrawal block"
    );
    let (dispute_game_index, game_l2_block) =
        find_game_for_withdrawal(l1_provider, portal_address, factory_address, block_number)
            .await?;

    debug!(
        game_index = %dispute_game_index,
        game_l2_block = game_l2_block,
        withdrawal_block = block_number,
        "Found suitable dispute game"
    );

    // 2. Get L2 block header for the GAME's block (not the withdrawal block!)
    // The output root proof must match the dispute game's committed state
    debug!(
        block = game_l2_block,
        "Fetching L2 block header for game's L2 block"
    );
    let block = l2_provider
        .get_block_by_number(BlockNumberOrTag::Number(game_l2_block))
        .await?
        .ok_or_else(|| eyre!("Block not found: {}", game_l2_block))?;

    let state_root = block.header.state_root;
    let block_hash = block.header.hash;

    // 3. Get storage proof using eth_getProof at the GAME's block
    // The withdrawal must exist at this block (which is >= withdrawal block)
    debug!(
        block = game_l2_block,
        "Generating storage proof at game's L2 block"
    );
    let storage_slot = compute_storage_slot(withdrawal_hash);
    let proof_result = l2_provider
        .get_proof(MESSAGE_PASSER_ADDRESS, vec![storage_slot])
        .block_id(BlockNumberOrTag::Number(game_l2_block).into())
        .await?;

    let message_passer_storage_root = proof_result.storage_hash;
    let withdrawal_proof = proof_result
        .storage_proof
        .first()
        .ok_or_else(|| eyre!("No storage proof returned"))?
        .proof
        .clone();

    debug!(
        proof_nodes = withdrawal_proof.len(),
        "Generated storage proof"
    );

    // 4. Build output root proof
    let output_root_proof = OutputRootProof {
        version: OUTPUT_VERSION_V0,
        stateRoot: state_root,
        messagePasserStorageRoot: message_passer_storage_root,
        latestBlockhash: block_hash,
    };

    Ok(ProveWithdrawalParams {
        withdrawal,
        dispute_game_index,
        output_root_proof,
        withdrawal_proof,
    })
}

/// Find a dispute game that covers the withdrawal's L2 block.
///
/// This function searches through recent dispute games to find one where:
/// - The game's L2 block number >= withdrawal's L2 block number
///
/// Note: For proving, we don't need the game to be finalized - we can prove
/// against an in-flight dispute game. Finalization is only required for the
/// finalize step after the challenge period.
///
/// Games are created roughly every hour, so we typically only need to check
/// a few dozen games even for withdrawals from weeks ago.
/// Returns (dispute_game_index, game_l2_block_number)
async fn find_game_for_withdrawal<P>(
    l1_provider: &P,
    portal_address: Address,
    factory_address: Address,
    withdrawal_l2_block: u64,
) -> Result<(U256, u64)>
where
    P: Provider + Clone,
{
    // Get the respected game type from portal
    let portal = IOptimismPortal2::new(portal_address, l1_provider);
    let game_type = portal.respectedGameType().call().await?;

    debug!(game_type, "Got respected game type from portal");

    let factory = IDisputeGameFactory::new(factory_address, l1_provider);

    // Get total game count to start from the latest
    let game_count = factory.gameCount().call().await?;
    if game_count == U256::ZERO {
        return Err(eyre!("No dispute games exist"));
    }
    debug!(total_games = %game_count, "Starting search from latest game");

    const MAX_GAMES_TO_CHECK: u64 = 1000; // ~40 days at 1 game/hour
    let start = game_count.saturating_sub(U256::from(1));

    debug!(
        start_index = %start,
        lookback = %MAX_GAMES_TO_CHECK,
        "Fetching batch of games"
    );

    let games = factory
        .findLatestGames(game_type, start, U256::from(MAX_GAMES_TO_CHECK))
        .call()
        .await?;

    if games.is_empty() {
        eyre::bail!("No games of type {} found", game_type);
    }

    debug!(
        found_games = games.len(),
        first_game_index = %games.first().map(|g| g.index).unwrap_or(U256::ZERO),
        last_game_index = %games.last().map(|g| g.index).unwrap_or(U256::ZERO),
        game_count = %game_count,
        "Found games for binary search"
    );

    // Log the newest game's L2 block to verify we can cover the withdrawal
    if let Some(newest_game) = games.first() {
        let newest_address = Address::from_slice(&newest_game.metadata.as_slice()[12..32]);
        let newest_contract = IFaultDisputeGame::new(newest_address, l1_provider);
        if let Ok(newest_l2_block) = newest_contract.l2BlockNumber().call().await {
            debug!(
                newest_game_index = %newest_game.index,
                newest_game_l2_block = newest_l2_block.to::<u64>(),
                withdrawal_l2_block,
                "Newest game L2 block check"
            );
        }
    }

    // Validate that all game indices are within bounds
    for game in &games {
        if game.index >= game_count {
            return Err(eyre!(
                "Invalid game index {} >= game count {}",
                game.index,
                game_count
            ));
        }
    }

    // Binary search to find the oldest game that covers the withdrawal.
    // Games array is sorted in DESCENDING order by L2 block:
    //   games[0] = newest (highest L2 block)
    //   games[len-1] = oldest (lowest L2 block)
    //
    // We want to find the rightmost (oldest) game where game_l2_block >= withdrawal_l2_block.
    // This is equivalent to finding the first game where game_l2_block < withdrawal_l2_block,
    // then returning the game just before it.
    let mut lo = 0;
    let mut hi = games.len();

    while lo < hi {
        let mi = lo + (hi - lo) / 2;
        let game = &games[mi];

        // Extract game proxy address from metadata (GameId)
        // GameId format: type (32 bits) | timestamp (64 bits) | proxy address (160 bits)
        // The address is in the lower 160 bits (20 bytes)
        let game_address = Address::from_slice(&game.metadata.as_slice()[12..32]);

        debug!(
            game_index = %game.index,
            game_address = %game_address,
            array_index = mi,
            "Processing game from search results"
        );

        let game_contract = IFaultDisputeGame::new(game_address, l1_provider);
        let game_l2_block = game_contract.l2BlockNumber().call().await.map_err(|e| {
            eyre!(
                "Failed to call l2BlockNumber on game {} at address {}: {}",
                game.index,
                game_address,
                e
            )
        })?;

        let game_l2_block_num = game_l2_block.to::<u64>();
        debug!(
            game_index = %game.index,
            game_l2_block = game_l2_block_num,
            withdrawal_l2_block,
            covers = game_l2_block_num >= withdrawal_l2_block,
            "Game L2 block comparison"
        );

        // In descending order: if this game covers, search right (older) for more candidates
        // If this game doesn't cover, search left (newer) for a game that does
        if game_l2_block_num >= withdrawal_l2_block {
            lo = mi + 1; // This game covers, but older games might too - search right
        } else {
            hi = mi; // This game is too old, search left for newer games
        }
    }

    // lo is now pointing to the first game that DOESN'T cover (or past the end).
    // The game we want is at lo - 1 (the last game that covers).
    if lo == 0 {
        // Even the newest game doesn't cover the withdrawal
        eyre::bail!(
            "No games of type {} found covering L2 block {} (newest game L2 block is older)",
            game_type,
            withdrawal_l2_block
        );
    }

    let selected_game = &games[lo - 1];

    // We need to get the L2 block for the selected game.
    // If we happened to check it during binary search, we might have it cached,
    // but the binary search may not have checked this exact game.
    // Re-fetch to be safe.
    let game_address = Address::from_slice(&selected_game.metadata.as_slice()[12..32]);
    let game_contract = IFaultDisputeGame::new(game_address, l1_provider);
    let game_l2_block = game_contract.l2BlockNumber().call().await?.to::<u64>();

    Ok((selected_game.index, game_l2_block))
}

/// Compute the storage slot for a withdrawal hash in the L2ToL1MessagePasser contract.
///
/// The storage layout is: `mapping(bytes32 => bool) public sentMessages`
/// Solidity storage slot = keccak256(key || slot_index)
/// For our mapping at slot 0: keccak256(withdrawalHash || 0)
pub fn compute_storage_slot(withdrawal_hash: B256) -> B256 {
    let mut data = [0u8; 64];
    data[0..32].copy_from_slice(withdrawal_hash.as_slice());
    // data[32..64] is already zeros (mapping is at slot 0)
    keccak256(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_storage_slot() {
        let withdrawal_hash = B256::from([1u8; 32]);
        let slot = compute_storage_slot(withdrawal_hash);

        // Verify it's deterministic
        let slot2 = compute_storage_slot(withdrawal_hash);
        assert_eq!(slot, slot2);

        // Verify different hashes produce different slots
        let other_hash = B256::from([2u8; 32]);
        let other_slot = compute_storage_slot(other_hash);
        assert_ne!(slot, other_slot);
    }

    #[test]
    fn test_storage_slot_format() {
        // Storage slot should be keccak256(withdrawalHash || 0x00...00)
        let withdrawal_hash = B256::ZERO;
        let slot = compute_storage_slot(withdrawal_hash);

        // Manually compute expected value
        let data = [0u8; 64];
        let expected = keccak256(data);

        assert_eq!(slot, expected);
    }

    #[test]
    fn test_prove_params_structure() {
        let params = ProveWithdrawalParams {
            withdrawal: WithdrawalTransaction {
                nonce: U256::from(1),
                sender: Address::ZERO,
                target: Address::ZERO,
                value: U256::from(1000),
                gasLimit: U256::from(100000),
                data: Bytes::new(),
            },
            dispute_game_index: U256::from(42),
            output_root_proof: OutputRootProof {
                version: OUTPUT_VERSION_V0,
                stateRoot: B256::ZERO,
                messagePasserStorageRoot: B256::ZERO,
                latestBlockhash: B256::ZERO,
            },
            withdrawal_proof: vec![Bytes::from(vec![1, 2, 3])],
        };

        assert_eq!(params.dispute_game_index, U256::from(42));
        assert_eq!(params.withdrawal_proof.len(), 1);
    }

    #[test]
    fn test_compute_storage_slot_real_example() {
        // Test with a real withdrawal hash pattern
        let withdrawal_hash = B256::from_slice(&[
            0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab,
            0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78,
            0x90, 0xab, 0xcd, 0xef,
        ]);

        let slot = compute_storage_slot(withdrawal_hash);

        // Verify the slot is 32 bytes
        assert_eq!(slot.len(), 32);

        // Verify it's not zero (would indicate a bug)
        assert_ne!(slot, B256::ZERO);
    }
}
