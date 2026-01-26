//! In-flight deposit tracking for Across Protocol.
//!
//! Tracks deposits initiated on L1 that haven't been filled on L2 yet.
//! Uses `(originChainId, depositId)` as the correlation key.

use alloy_contract::private::Provider;
use alloy_primitives::{Address, FixedBytes, U256};
use binding::across::ISpokePool;
use std::collections::HashSet;
use tokio_retry::{strategy::ExponentialBackoff, Retry};
use tracing::{debug, warn};

/// An in-flight deposit that has been initiated on L1 but not yet filled on L2.
#[derive(Debug, Clone)]
pub struct InFlightDeposit {
    /// Unique deposit ID on the origin chain
    pub deposit_id: U256,
    /// Chain ID where the deposit was initiated
    pub origin_chain_id: u64,
    /// Chain ID where the deposit should be filled
    pub destination_chain_id: u64,
    /// Amount deposited (input amount)
    pub input_amount: U256,
    /// Depositor address
    pub depositor: Address,
    /// Block number on L1 where the deposit was initiated
    pub block_number: u64,
}

/// Provider for querying in-flight deposits across L1 and L2.
pub struct DepositStateProvider<P1, P2> {
    l1_provider: P1,
    l2_provider: P2,
    l1_spoke_pool: Address,
    l2_spoke_pool: Address,
}

impl<P1, P2> DepositStateProvider<P1, P2>
where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    pub const fn new(
        l1_provider: P1,
        l2_provider: P2,
        l1_spoke_pool: Address,
        l2_spoke_pool: Address,
    ) -> Self {
        Self {
            l1_provider,
            l2_provider,
            l1_spoke_pool,
            l2_spoke_pool,
        }
    }

    /// Get all in-flight deposits (initiated on L1 but not filled on L2).
    ///
    /// # Arguments
    /// * `depositor` - Filter deposits by this depositor address
    /// * `destination_chain_id` - Filter deposits destined for this chain
    /// * `origin_chain_id` - The chain ID of L1 (typically 1 for Ethereum mainnet)
    /// * `lookback_secs` - How far back to scan (in seconds)
    /// * `l1_block_time_secs` - L1 block time (12 for Ethereum)
    /// * `l2_block_time_secs` - L2 block time (1 for Unichain)
    ///
    /// # Returns
    /// A list of deposits that have been initiated but not yet filled.
    pub async fn get_inflight_deposits(
        &self,
        depositor: Address,
        destination_chain_id: u64,
        origin_chain_id: u64,
        lookback_secs: u64,
        l1_block_time_secs: u64,
        l2_block_time_secs: u64,
    ) -> eyre::Result<Vec<InFlightDeposit>> {
        // Calculate lookback blocks for each chain
        let l1_lookback_blocks = lookback_secs / l1_block_time_secs;
        let l2_lookback_blocks = lookback_secs / l2_block_time_secs;

        // Get current block numbers
        let l1_current_block = self.l1_provider.get_block_number().await?;
        let l2_current_block = self.l2_provider.get_block_number().await?;

        let l1_from_block = l1_current_block.saturating_sub(l1_lookback_blocks);
        let l2_from_block = l2_current_block.saturating_sub(l2_lookback_blocks);

        debug!(
            l1_from = l1_from_block,
            l1_to = l1_current_block,
            l2_from = l2_from_block,
            l2_to = l2_current_block,
            lookback_secs,
            depositor = %depositor,
            destination_chain_id,
            "Scanning for in-flight deposits"
        );

        // Query L1 for FundsDeposited events
        let l1_deposits = self
            .scan_l1_deposits(
                depositor,
                destination_chain_id,
                l1_from_block,
                l1_current_block,
            )
            .await?;

        if l1_deposits.is_empty() {
            debug!("No L1 deposits found in range");
            return Ok(vec![]);
        }

        // Collect deposit IDs to check on L2
        let deposit_ids: Vec<U256> = l1_deposits.iter().map(|d| d.deposit_id).collect();

        debug!(
            count = l1_deposits.len(),
            "Found L1 deposits, checking L2 for fills"
        );

        // Query L2 for FilledRelay events matching these deposit IDs
        let filled_ids = self
            .get_filled_deposit_ids(
                origin_chain_id,
                &deposit_ids,
                l2_from_block,
                l2_current_block,
            )
            .await?;

        debug!(
            filled_count = filled_ids.len(),
            "Found filled deposits on L2"
        );

        // Filter out filled deposits
        let inflight: Vec<InFlightDeposit> = l1_deposits
            .into_iter()
            .filter(|d| !filled_ids.contains(&d.deposit_id))
            .collect();

        debug!(
            inflight_count = inflight.len(),
            "In-flight deposits after filtering"
        );

        Ok(inflight)
    }

    /// Scan L1 for FundsDeposited events in chunks.
    async fn scan_l1_deposits(
        &self,
        depositor: Address,
        destination_chain_id: u64,
        from_block: u64,
        to_block: u64,
    ) -> eyre::Result<Vec<InFlightDeposit>> {
        const CHUNK_SIZE: u64 = 9_500;

        let mut all_deposits = Vec::new();
        let mut current = from_block;

        while current <= to_block {
            let chunk_end = (current + CHUNK_SIZE - 1).min(to_block);

            let chunk_deposits = self
                .scan_l1_chunk_with_retry(depositor, destination_chain_id, current, chunk_end)
                .await?;

            all_deposits.extend(chunk_deposits);
            current = chunk_end + 1;
        }

        Ok(all_deposits)
    }

    /// Scan a single L1 chunk with retry logic.
    async fn scan_l1_chunk_with_retry(
        &self,
        depositor: Address,
        destination_chain_id: u64,
        from_block: u64,
        to_block: u64,
    ) -> eyre::Result<Vec<InFlightDeposit>> {
        let retry_strategy = ExponentialBackoff::from_millis(100).take(5);

        Retry::spawn(retry_strategy, || async {
            self.scan_l1_chunk(depositor, destination_chain_id, from_block, to_block)
                .await
                .map_err(|e| {
                    warn!(
                        from = from_block,
                        to = to_block,
                        error = %e,
                        "L1 chunk scan failed, will retry"
                    );
                    e
                })
        })
        .await
    }

    /// Scan a single chunk of L1 blocks for FundsDeposited events.
    async fn scan_l1_chunk(
        &self,
        depositor: Address,
        destination_chain_id: u64,
        from_block: u64,
        to_block: u64,
    ) -> eyre::Result<Vec<InFlightDeposit>> {
        let contract = ISpokePool::new(self.l1_spoke_pool, &self.l1_provider);

        // Convert depositor address to bytes32 for filtering
        let depositor_bytes32 = address_to_bytes32(depositor);

        let filter = contract
            .FundsDeposited_filter()
            .topic1(U256::from(destination_chain_id)) // destinationChainId (indexed)
            .topic3(depositor_bytes32) // depositor (indexed)
            .from_block(from_block)
            .to_block(to_block);

        let events = filter.query().await?;

        let origin_chain_id = self.l1_provider.get_chain_id().await?;

        let deposits: Vec<InFlightDeposit> = events
            .into_iter()
            .map(|(event, log)| InFlightDeposit {
                deposit_id: event.depositId,
                origin_chain_id,
                destination_chain_id,
                input_amount: event.inputAmount,
                depositor,
                block_number: log.block_number.unwrap_or_default(),
            })
            .collect();

        Ok(deposits)
    }

    /// Query L2 for FilledRelay events and return the set of filled deposit IDs.
    async fn get_filled_deposit_ids(
        &self,
        origin_chain_id: u64,
        deposit_ids: &[U256],
        from_block: u64,
        to_block: u64,
    ) -> eyre::Result<HashSet<U256>> {
        if deposit_ids.is_empty() {
            return Ok(HashSet::new());
        }

        let mut filled_ids = HashSet::new();

        // Scan in chunks
        const CHUNK_SIZE: u64 = 9_500;
        let mut current = from_block;

        while current <= to_block {
            let chunk_end = (current + CHUNK_SIZE - 1).min(to_block);

            let chunk_filled = self
                .scan_l2_fills_chunk_with_retry(origin_chain_id, current, chunk_end)
                .await?;

            // Only keep fills for deposit IDs we care about
            for id in chunk_filled {
                if deposit_ids.contains(&id) {
                    filled_ids.insert(id);
                }
            }

            current = chunk_end + 1;
        }

        Ok(filled_ids)
    }

    /// Scan a single L2 chunk with retry logic.
    async fn scan_l2_fills_chunk_with_retry(
        &self,
        origin_chain_id: u64,
        from_block: u64,
        to_block: u64,
    ) -> eyre::Result<Vec<U256>> {
        let retry_strategy = ExponentialBackoff::from_millis(100).take(5);

        Retry::spawn(retry_strategy, || async {
            self.scan_l2_fills_chunk(origin_chain_id, from_block, to_block)
                .await
                .map_err(|e| {
                    warn!(
                        from = from_block,
                        to = to_block,
                        error = %e,
                        "L2 chunk scan failed, will retry"
                    );
                    e
                })
        })
        .await
    }

    /// Scan a single chunk of L2 blocks for FilledRelay events.
    async fn scan_l2_fills_chunk(
        &self,
        origin_chain_id: u64,
        from_block: u64,
        to_block: u64,
    ) -> eyre::Result<Vec<U256>> {
        let contract = ISpokePool::new(self.l2_spoke_pool, &self.l2_provider);

        let filter = contract
            .FilledRelay_filter()
            .topic1(U256::from(origin_chain_id)) // originChainId (indexed)
            .from_block(from_block)
            .to_block(to_block);

        let events = filter.query().await?;

        let deposit_ids: Vec<U256> = events
            .into_iter()
            .map(|(event, _)| event.depositId)
            .collect();

        Ok(deposit_ids)
    }
}

/// Convert an Address to bytes32 (left-padded with zeros).
fn address_to_bytes32(addr: Address) -> FixedBytes<32> {
    let mut bytes = [0u8; 32];
    bytes[12..32].copy_from_slice(addr.as_slice());
    FixedBytes::from(bytes)
}

/// Convenience function to get in-flight deposits without creating a provider struct.
#[allow(clippy::too_many_arguments)]
pub async fn get_inflight_deposits<P1, P2>(
    l1_provider: P1,
    l2_provider: P2,
    l1_spoke_pool: Address,
    l2_spoke_pool: Address,
    depositor: Address,
    destination_chain_id: u64,
    origin_chain_id: u64,
    lookback_secs: u64,
    l1_block_time_secs: u64,
    l2_block_time_secs: u64,
) -> eyre::Result<Vec<InFlightDeposit>>
where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    let provider =
        DepositStateProvider::new(l1_provider, l2_provider, l1_spoke_pool, l2_spoke_pool);

    provider
        .get_inflight_deposits(
            depositor,
            destination_chain_id,
            origin_chain_id,
            lookback_secs,
            l1_block_time_secs,
            l2_block_time_secs,
        )
        .await
}

/// Get the total amount of in-flight deposits (initiated on L1 but not yet filled on L2).
///
/// This is used to calculate the projected SpokePool balance after pending deposits settle.
#[allow(clippy::too_many_arguments)]
pub async fn get_inflight_deposit_total<P1, P2>(
    l1_provider: P1,
    l2_provider: P2,
    l1_spoke_pool: Address,
    l2_spoke_pool: Address,
    depositor: Address,
    destination_chain_id: u64,
    origin_chain_id: u64,
    lookback_secs: u64,
    l1_block_time_secs: u64,
    l2_block_time_secs: u64,
) -> eyre::Result<U256>
where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    let inflight = get_inflight_deposits(
        l1_provider,
        l2_provider,
        l1_spoke_pool,
        l2_spoke_pool,
        depositor,
        destination_chain_id,
        origin_chain_id,
        lookback_secs,
        l1_block_time_secs,
        l2_block_time_secs,
    )
    .await?;

    let total: U256 = inflight.iter().map(|d| d.input_amount).sum();
    Ok(total)
}
