use crate::{
    hash::compute_withdrawal_hash,
    types::{WithdrawalHash, WithdrawalStatus},
};
use alloy_contract::private::Provider;
use alloy_primitives::Address;
use alloy_rpc_types_eth::BlockNumberOrTag;
use binding::opstack::{
    IL2ToL1MessagePasser, IOptimismPortal2, IOptimismPortal2::ProvenWithdrawal,
    WithdrawalTransaction,
};
use tokio_retry::{strategy::ExponentialBackoff, Retry};
use tracing::{debug, error, warn};

#[allow(dead_code)]
pub struct WithdrawalStateProvider<P1, P2> {
    l1_provider: P1,
    l2_provider: P2,
    portal_address: Address,
    message_passer_address: Address,
}

#[allow(dead_code)]
pub struct PendingWithdrawal {
    pub transaction: WithdrawalTransaction,
    pub hash: WithdrawalHash,
    pub l2_block: u64,
    pub status: WithdrawalStatus,
}

#[allow(dead_code)]
impl<P1, P2> WithdrawalStateProvider<P1, P2>
where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    pub const fn new(
        l1_provider: P1,
        l2_provider: P2,
        portal_address: Address,
        message_passer_address: Address,
    ) -> Self {
        Self {
            l1_provider,
            l2_provider,
            portal_address,
            message_passer_address,
        }
    }

    pub async fn query_withdrawal_status(
        &self,
        hash: WithdrawalHash,
        proof_submitter: Address,
    ) -> eyre::Result<WithdrawalStatus> {
        if self.is_finalized(hash).await? {
            return Ok(WithdrawalStatus::Finalized);
        }

        if let Some(proven) = self.is_proven(hash, proof_submitter).await? {
            return Ok(WithdrawalStatus::Proven {
                timestamp: proven.timestamp,
            });
        }

        Ok(WithdrawalStatus::Initiated)
    }

    /// Get all pending withdrawals from L2 events in the given block range.
    ///
    /// Scans MessagePassed events and returns withdrawals that haven't been finalized,
    /// with their current status (Initiated or Proven).
    ///
    /// This method:
    /// 1. Resolves `Latest` to concrete block numbers immediately (handles load balancer inconsistency)
    /// 2. Chunks requests into 9,500 block ranges (with 500 block safety margin)
    /// 3. Retries failed chunks with exponential backoff
    ///
    /// The safety margin and chunking handle RPC providers that may be slightly out of sync
    /// when behind a load balancer.
    pub async fn get_pending_withdrawals(
        &self,
        from_block: BlockNumberOrTag,
        to_block: BlockNumberOrTag,
        proof_submitter: Address,
    ) -> eyre::Result<Vec<PendingWithdrawal>> {
        // CRITICAL: Resolve both endpoints to concrete block numbers FIRST
        // This creates a consistent snapshot and prevents load balancer issues
        let from_block_num = self.resolve_block_number(from_block).await?;
        let to_block_num = self.resolve_block_number(to_block).await?;

        if from_block_num > to_block_num {
            return Err(eyre::eyre!(
                "from_block ({}) must be <= to_block ({})",
                from_block_num,
                to_block_num
            ));
        }

        debug!(
            from = from_block_num,
            to = to_block_num,
            "Scanning for withdrawals (snapshot taken)"
        );

        self.scan_chunks(from_block_num, to_block_num, proof_submitter)
            .await
    }

    /// Resolve BlockNumberOrTag to a concrete block number.
    async fn resolve_block_number(&self, block: BlockNumberOrTag) -> eyre::Result<u64> {
        match block {
            BlockNumberOrTag::Number(n) => Ok(n),
            BlockNumberOrTag::Latest => {
                let block_num = self.l2_provider.get_block_number().await?;
                Ok(block_num)
            }
            _ => Err(eyre::eyre!("Unsupported block tag: {:?}", block)),
        }
    }

    /// Scan blocks in chunks with retry logic.
    async fn scan_chunks(
        &self,
        from_block: u64,
        to_block: u64,
        proof_submitter: Address,
    ) -> eyre::Result<Vec<PendingWithdrawal>> {
        // Use 9,500 block chunks (500 block safety margin for RPC limits)
        const CHUNK_SIZE: u64 = 9_500;

        let mut all_withdrawals = Vec::new();
        let mut current = from_block;

        while current <= to_block {
            let chunk_end = (current + CHUNK_SIZE - 1).min(to_block);

            debug!(
                from = current,
                to = chunk_end,
                "Scanning chunk for withdrawals"
            );

            // Retry chunk with exponential backoff on failure
            let chunk_withdrawals = self
                .scan_chunk_with_retry(current, chunk_end, proof_submitter)
                .await?;

            all_withdrawals.extend(chunk_withdrawals);
            current = chunk_end + 1;
        }

        Ok(all_withdrawals)
    }

    /// Scan a single chunk with retry and exponential backoff.
    async fn scan_chunk_with_retry(
        &self,
        from_block: u64,
        to_block: u64,
        proof_submitter: Address,
    ) -> eyre::Result<Vec<PendingWithdrawal>> {
        // Exponential backoff: 100ms, 200ms, 400ms, 800ms, 1.6s (max 5 attempts)
        let retry_strategy = ExponentialBackoff::from_millis(100).take(5);

        Retry::spawn(retry_strategy, || async {
            self.scan_chunk(from_block, to_block, proof_submitter)
                .await
                .map_err(|e| {
                    warn!(
                        from = from_block,
                        to = to_block,
                        error = %e,
                        "Chunk scan failed, will retry"
                    );
                    e
                })
        })
        .await
    }

    /// Scan a single chunk of blocks (no retry logic).
    async fn scan_chunk(
        &self,
        from_block: u64,
        to_block: u64,
        proof_submitter: Address,
    ) -> eyre::Result<Vec<PendingWithdrawal>> {
        let contract = IL2ToL1MessagePasser::new(self.message_passer_address, &self.l2_provider);

        let filter = contract
            .MessagePassed_filter()
            .from_block(from_block)
            .to_block(to_block);
        let events = filter.query().await?;

        let mut withdrawals = vec![];
        for (event, log) in events {
            let tx = WithdrawalTransaction {
                nonce: event.nonce,
                sender: event.sender,
                target: event.target,
                value: event.value,
                gasLimit: event.gasLimit,
                data: event.data,
            };

            let computed_hash = compute_withdrawal_hash(&tx);
            if computed_hash != event.withdrawalHash {
                error!(
                    block = ?log.block_number,
                    computed_hash = %computed_hash,
                    withdrawal_hash = %event.withdrawalHash,
                    "Error!: withdrawal hash mismatch for withdrawal"
                );
                // allow to continue, don't fail the entire scan.
                continue;
            }

            // Query the current status of this withdrawal
            let status = self
                .query_withdrawal_status(event.withdrawalHash, proof_submitter)
                .await?;

            // Skip finalized withdrawals - nothing to do
            if matches!(status, WithdrawalStatus::Finalized) {
                continue;
            }

            withdrawals.push(PendingWithdrawal {
                transaction: tx,
                hash: event.withdrawalHash,
                l2_block: log.block_number.unwrap_or_default(),
                status,
            })
        }

        Ok(withdrawals)
    }

    pub async fn is_finalized(&self, hash: WithdrawalHash) -> eyre::Result<bool> {
        let portal = IOptimismPortal2::new(self.portal_address, &self.l1_provider);
        let finalized = portal.finalizedWithdrawals(hash).call().await?;
        Ok(finalized)
    }

    pub async fn is_proven(
        &self,
        hash: WithdrawalHash,
        proof_submitter: Address,
    ) -> eyre::Result<Option<ProvenWithdrawal>> {
        let portal = IOptimismPortal2::new(self.portal_address, &self.l1_provider);
        let proven = portal
            .provenWithdrawals(hash, proof_submitter)
            .call()
            .await?;

        if proven.timestamp == 0 {
            Ok(None)
        } else {
            Ok(Some(ProvenWithdrawal {
                disputeGameProxy: proven.disputeGameProxy,
                timestamp: proven.timestamp,
            }))
        }
    }
}
