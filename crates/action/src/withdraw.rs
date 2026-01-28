use crate::{Action, SignerFn};
use alloy_primitives::{utils::format_ether, Address, Bytes, B256, U256};
use alloy_provider::Provider;
use alloy_sol_types::SolEvent;
use binding::opstack::{IL2ToL1MessagePasser, WithdrawalTransaction};
use tracing::info;
use withdrawal::types::WithdrawalHash;

/// Withdraw input data.
#[derive(Clone)]
pub struct Withdraw {
    /// withdrawal contract address
    /// should be the address of L2ToL1MessagePasser
    pub contract: Address,
    pub source: Address,
    pub target: Address,
    pub value: U256,
    pub gas_limit: U256,
    pub data: Bytes,
    /// Optional: only exists on initiated withdrawal
    /// transaction hash from execution
    pub tx_hash: Option<B256>,
}

pub struct WithdrawAction<P> {
    provider: P,
    signer: SignerFn,
    action: Withdraw,
}

impl<P: Provider + Clone> WithdrawAction<P> {
    pub fn new(provider: P, signer: SignerFn, action: Withdraw) -> Self {
        Self {
            provider,
            signer,
            action,
        }
    }
}

impl<P> Action for WithdrawAction<P>
where
    P: Provider + Clone,
{
    async fn is_ready(&self) -> eyre::Result<bool> {
        if self.action.value == U256::ZERO {
            return Ok(false);
        }

        if self.action.target == Address::ZERO {
            return Ok(false);
        }

        let balance = self.provider.get_balance(self.action.source).await?;
        Ok(balance >= self.action.value)
    }

    async fn is_completed(&self) -> eyre::Result<bool> {
        let Some(tx_hash) = self.action.tx_hash else {
            return Ok(false);
        };

        // Transaction must exist and be mined
        let Some(receipt) = self.provider.get_transaction_receipt(tx_hash).await? else {
            return Ok(false);
        };

        // Parse the MessagePassed event to verify it's our withdrawal
        let Ok((withdrawal_tx, _)) = parse_message_passed_event(&receipt) else {
            return Ok(false);
        };

        // Double-check this is our withdrawal by comparing parameters
        if withdrawal_tx.sender != self.action.source
            || withdrawal_tx.target != self.action.target
            || withdrawal_tx.value != self.action.value
            || withdrawal_tx.gasLimit != self.action.gas_limit
            || withdrawal_tx.data != self.action.data
        {
            return Ok(false);
        }

        // This is definitely our withdrawal, and it's completed
        Ok(true)
    }

    async fn execute(&mut self) -> eyre::Result<crate::Result> {
        if self.is_completed().await? {
            eyre::bail!("Withdrawal already initiated")
        }

        let contract = IL2ToL1MessagePasser::new(self.action.contract, &self.provider);

        // Build the transaction request
        let call = contract
            .initiateWithdrawal(
                self.action.target,
                self.action.gas_limit,
                self.action.data.clone(),
            )
            .value(self.action.value);
        let tx_request = call.into_transaction_request();

        // Sign externally
        let signed_tx = (self.signer)(tx_request).await?;

        // Broadcast the signed transaction
        let pending = self.provider.send_raw_transaction(&signed_tx).await?;
        let receipt = pending.get_receipt().await?;

        let (withdrawal_tx, withdrawal_hash) = parse_message_passed_event(&receipt)?;
        info!(
            tx_hash = %receipt.transaction_hash,
            block_number = receipt.block_number,
            gas_used = receipt.gas_used,
            withdrawal_hash = %withdrawal_hash,
            withdrawal_tx = ?withdrawal_tx,
            "Withdrawal initiated."
        );

        self.action.tx_hash = Some(receipt.transaction_hash);

        Ok(crate::Result {
            tx_hash: receipt.transaction_hash,
            block_number: receipt.block_number,
            gas_used: Some(U256::from(receipt.gas_used)),
        })
    }

    fn description(&self) -> String {
        let eth_amount = format_ether(self.action.value);
        format!("Withdrawing {} ETH to Ethereum Mainnet", eth_amount)
    }
}

fn parse_message_passed_event(
    receipt: &alloy_rpc_types_eth::transaction::TransactionReceipt,
) -> eyre::Result<(WithdrawalTransaction, WithdrawalHash)> {
    for log in receipt.logs() {
        if let Ok(event) = IL2ToL1MessagePasser::MessagePassed::decode_log(&log.inner) {
            let tx = WithdrawalTransaction {
                nonce: event.nonce,
                sender: event.sender,
                target: event.target,
                value: event.value,
                gasLimit: event.gasLimit,
                data: event.data.data.clone(),
            };

            let hash = event.withdrawalHash;

            return Ok((tx, hash));
        }
    }

    eyre::bail!("Message passed event not found in receipt")
}
