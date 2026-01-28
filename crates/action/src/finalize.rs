//! Finalize withdrawal action.
//!
//! Finalizes a proven withdrawal on L1, executing the withdrawal transaction
//! and sending ETH/tokens to the recipient.

use crate::{Action, SignerFn};
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use binding::opstack::{IOptimismPortal2, WithdrawalTransaction};
use tracing::info;
use withdrawal::{state::WithdrawalStateProvider, types::WithdrawalHash};

/// Input data for finalizing a withdrawal on L1.
#[derive(Clone, Debug)]
pub struct Finalize {
    /// OptimismPortal2 contract address on L1
    pub portal_address: Address,
    /// The withdrawal transaction details
    pub withdrawal: WithdrawalTransaction,
    /// Hash of the withdrawal
    pub withdrawal_hash: WithdrawalHash,
    /// Address that submitted the proof (usually the same as withdrawal sender)
    pub proof_submitter: Address,
    /// Address that will submit the finalize transaction
    pub from: Address,
}

/// Action to finalize a proven withdrawal on L1.
pub struct FinalizeAction<P1, P2> {
    l1_provider: P1,
    l2_provider: P2,
    signer: SignerFn,
    action: Finalize,
}

impl<P1, P2> FinalizeAction<P1, P2>
where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    pub fn new(l1_provider: P1, l2_provider: P2, signer: SignerFn, action: Finalize) -> Self {
        Self {
            l1_provider,
            l2_provider,
            signer,
            action,
        }
    }

    /// Get the withdrawal hash for this action.
    pub const fn withdrawal_hash(&self) -> WithdrawalHash {
        self.action.withdrawal_hash
    }

    /// Check if the withdrawal has been finalized using WithdrawalStateProvider.
    async fn check_is_finalized(&self) -> eyre::Result<bool> {
        let state = WithdrawalStateProvider::new(
            self.l1_provider.clone(),
            self.l2_provider.clone(),
            self.action.portal_address,
            Address::ZERO, // message passer not needed for finalized check
        );

        state.is_finalized(self.action.withdrawal_hash).await
    }

    /// Check if the withdrawal has been proven and get the proof timestamp.
    async fn check_is_proven(&self) -> eyre::Result<Option<u64>> {
        let state = WithdrawalStateProvider::new(
            self.l1_provider.clone(),
            self.l2_provider.clone(),
            self.action.portal_address,
            Address::ZERO, // message passer not needed for proven check
        );

        let proven = state
            .is_proven(self.action.withdrawal_hash, self.action.proof_submitter)
            .await?;

        Ok(proven.map(|p| p.timestamp))
    }

    /// Get the proof maturity delay from the portal contract.
    async fn get_proof_maturity_delay(&self) -> eyre::Result<u64> {
        let portal = IOptimismPortal2::new(self.action.portal_address, &self.l1_provider);
        let delay: U256 = portal.proofMaturityDelaySeconds().call().await?;
        Ok(delay.try_into().unwrap_or(u64::MAX))
    }

    /// Get the current L1 block timestamp.
    async fn get_current_timestamp(&self) -> eyre::Result<u64> {
        let block = self
            .l1_provider
            .get_block_by_number(alloy_rpc_types_eth::BlockNumberOrTag::Latest)
            .await?
            .ok_or_else(|| eyre::eyre!("Failed to get latest block"))?;
        Ok(block.header.timestamp)
    }
}

impl<P1, P2> Action for FinalizeAction<P1, P2>
where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    async fn is_ready(&self) -> eyre::Result<bool> {
        // Not ready if already finalized
        if self.check_is_finalized().await? {
            return Ok(false);
        }

        // Check if proven and maturity delay has passed
        let Some(proven_timestamp) = self.check_is_proven().await? else {
            // Not proven yet
            return Ok(false);
        };

        let maturity_delay = self.get_proof_maturity_delay().await?;
        let current_timestamp = self.get_current_timestamp().await?;

        // Ready if current time >= proven timestamp + maturity delay
        Ok(current_timestamp >= proven_timestamp + maturity_delay)
    }

    async fn is_completed(&self) -> eyre::Result<bool> {
        self.check_is_finalized().await
    }

    async fn execute(&mut self) -> eyre::Result<crate::Result> {
        if self.is_completed().await? {
            eyre::bail!("Withdrawal already finalized")
        }

        // Verify the withdrawal is proven
        let Some(proven_timestamp) = self.check_is_proven().await? else {
            eyre::bail!("Withdrawal not proven yet")
        };

        // Verify maturity delay has passed
        let maturity_delay = self.get_proof_maturity_delay().await?;
        let current_timestamp = self.get_current_timestamp().await?;

        if current_timestamp < proven_timestamp + maturity_delay {
            let remaining = (proven_timestamp + maturity_delay) - current_timestamp;
            eyre::bail!(
                "Proof maturity delay not elapsed. {} seconds remaining",
                remaining
            )
        }

        info!(
            withdrawal_hash = %self.action.withdrawal_hash,
            proof_submitter = %self.action.proof_submitter,
            "Finalizing withdrawal"
        );

        // Build the transaction request
        let portal = IOptimismPortal2::new(self.action.portal_address, &self.l1_provider);
        let call = portal.finalizeWithdrawalTransactionExternalProof(
            self.action.withdrawal.clone(),
            self.action.proof_submitter,
        );
        let tx_request = call.into_transaction_request().from(self.action.from);

        // Fill transaction fields (nonce, gas, fees) using our provider
        let filled_tx = client::fill_transaction(tx_request, &self.l1_provider).await?;

        // Sign externally
        let signed_tx = (self.signer)(filled_tx).await?;

        // Broadcast the signed transaction
        let pending = self.l1_provider.send_raw_transaction(&signed_tx).await?;
        let receipt = pending.get_receipt().await?;

        info!(
            tx_hash = %receipt.transaction_hash,
            block_number = receipt.block_number,
            gas_used = receipt.gas_used,
            withdrawal_hash = %self.action.withdrawal_hash,
            "Withdrawal finalized on L1"
        );

        Ok(crate::Result {
            tx_hash: receipt.transaction_hash,
            block_number: receipt.block_number,
            gas_used: Some(U256::from(receipt.gas_used)),
        })
    }

    fn description(&self) -> String {
        format!(
            "Finalizing withdrawal {} on L1",
            self.action.withdrawal_hash
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{mock_signer, MockProvider};
    use alloy_primitives::{address, b256, Bytes};

    fn create_test_finalize_action() -> FinalizeAction<MockProvider, MockProvider> {
        let finalize = Finalize {
            portal_address: address!("0d83dab629f0e0F9d36c0Cbc89B69a489f0751bD"),
            withdrawal: WithdrawalTransaction {
                nonce: U256::from(1),
                sender: address!("5CFFA347b0aE99cc01E5c01714cA5658e54a23D1"),
                target: address!("5CFFA347b0aE99cc01E5c01714cA5658e54a23D1"),
                value: U256::from(1000000000000000u64), // 0.001 ETH
                gasLimit: U256::from(100000),
                data: Bytes::new(),
            },
            withdrawal_hash: b256!(
                "1111111111111111111111111111111111111111111111111111111111111111"
            ),
            proof_submitter: address!("5CFFA347b0aE99cc01E5c01714cA5658e54a23D1"),
            from: address!("5CFFA347b0aE99cc01E5c01714cA5658e54a23D1"),
        };

        FinalizeAction::new(MockProvider, MockProvider, mock_signer(), finalize)
    }

    #[test]
    fn test_finalize_action_description() {
        let action = create_test_finalize_action();
        let desc = action.description();
        assert!(desc.contains("Finalizing withdrawal"));
        assert!(desc.contains("1111111111111111111111111111111111111111111111111111111111111111"));
    }

    #[test]
    fn test_finalize_action_withdrawal_hash() {
        let action = create_test_finalize_action();
        assert_eq!(
            action.withdrawal_hash(),
            b256!("1111111111111111111111111111111111111111111111111111111111111111")
        );
    }
}
