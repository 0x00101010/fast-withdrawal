//! Prove withdrawal action.
//!
//! Submits a proof to L1 that a withdrawal was initiated on L2.

use crate::{Action, SignerFn};
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use binding::opstack::{IOptimismPortal2, WithdrawalTransaction};
use tracing::info;
use withdrawal::{proof::generate_proof, state::WithdrawalStateProvider, types::WithdrawalHash};

/// Input data for proving a withdrawal on L1.
#[derive(Clone, Debug)]
pub struct Prove {
    /// OptimismPortal2 contract address on L1
    pub portal_address: Address,
    /// DisputeGameFactory contract address on L1
    pub factory_address: Address,
    /// The withdrawal transaction details
    pub withdrawal: WithdrawalTransaction,
    /// Hash of the withdrawal
    pub withdrawal_hash: WithdrawalHash,
    /// L2 block number where the withdrawal was initiated
    pub l2_block: u64,
    /// Address that will submit the proof transaction
    pub from: Address,
}

/// Action to prove a withdrawal on L1.
pub struct ProveAction<P1, P2> {
    l1_provider: P1,
    l2_provider: P2,
    signer: SignerFn,
    action: Prove,
}

impl<P1, P2> ProveAction<P1, P2>
where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    pub fn new(l1_provider: P1, l2_provider: P2, signer: SignerFn, action: Prove) -> Self {
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

    /// Check if the withdrawal has been proven using WithdrawalStateProvider.
    async fn check_is_proven(&self) -> eyre::Result<bool> {
        let state = WithdrawalStateProvider::new(
            self.l1_provider.clone(),
            self.l2_provider.clone(),
            self.action.portal_address,
            Address::ZERO, // message passer not needed for is_proven check
        );

        let proven = state
            .is_proven(self.action.withdrawal_hash, self.action.withdrawal.sender)
            .await?;

        Ok(proven.is_some())
    }
}

impl<P1, P2> Action for ProveAction<P1, P2>
where
    P1: Provider + Clone,
    P2: Provider + Clone,
{
    async fn is_ready(&self) -> eyre::Result<bool> {
        // Ready if not already proven
        Ok(!self.check_is_proven().await?)
    }

    async fn is_completed(&self) -> eyre::Result<bool> {
        self.check_is_proven().await
    }

    async fn execute(&mut self) -> eyre::Result<crate::Result> {
        if self.is_completed().await? {
            eyre::bail!("Withdrawal already proven")
        }

        // Generate the proof
        info!(
            withdrawal_hash = %self.action.withdrawal_hash,
            l2_block = self.action.l2_block,
            "Generating withdrawal proof"
        );

        let proof_params = generate_proof(
            &self.l1_provider,
            &self.l2_provider,
            self.action.portal_address,
            self.action.factory_address,
            self.action.withdrawal_hash,
            self.action.withdrawal.clone(),
            self.action.l2_block,
        )
        .await?;

        info!(
            dispute_game_index = %proof_params.dispute_game_index,
            proof_nodes = proof_params.withdrawal_proof.len(),
            "Proof generated, submitting to L1"
        );

        // Build the transaction request
        let portal = IOptimismPortal2::new(self.action.portal_address, &self.l1_provider);
        let call = portal.proveWithdrawalTransaction(
            proof_params.withdrawal,
            proof_params.dispute_game_index,
            proof_params.output_root_proof,
            proof_params.withdrawal_proof,
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
            dispute_game_index = %proof_params.dispute_game_index,
            "Withdrawal proven on L1"
        );

        Ok(crate::Result {
            tx_hash: receipt.transaction_hash,
            block_number: receipt.block_number,
            gas_used: Some(U256::from(receipt.gas_used)),
        })
    }

    fn description(&self) -> String {
        format!("Proving withdrawal {} on L1", self.action.withdrawal_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{mock_signer, MockProvider};
    use alloy_primitives::{address, b256, Bytes};

    fn create_test_prove_action() -> ProveAction<MockProvider, MockProvider> {
        let prove = Prove {
            portal_address: address!("0d83dab629f0e0F9d36c0Cbc89B69a489f0751bD"),
            factory_address: address!("eff73e5aa3B9AEC32c659Aa3E00444d20a84394b"),
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
            l2_block: 42276959,
            from: address!("5CFFA347b0aE99cc01E5c01714cA5658e54a23D1"),
        };

        ProveAction::new(MockProvider, MockProvider, mock_signer(), prove)
    }

    #[test]
    fn test_prove_action_description() {
        let action = create_test_prove_action();
        let desc = action.description();
        assert!(desc.contains("Proving withdrawal"));
        assert!(desc.contains("1111111111111111111111111111111111111111111111111111111111111111"));
    }

    #[test]
    fn test_prove_action_withdrawal_hash() {
        let action = create_test_prove_action();
        assert_eq!(
            action.withdrawal_hash(),
            b256!("1111111111111111111111111111111111111111111111111111111111111111")
        );
    }
}
