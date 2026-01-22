use crate::Action;
use alloy_primitives::{utils::format_ether, Address, Bytes, U256};
use alloy_provider::Provider;
use alloy_sol_types::{sol, SolEvent};
use tracing::info;
use withdrawal::{contract::WithdrawalTransaction, types::WithdrawalHash};

sol! {
    #[sol(rpc)]
    interface L2ToL1MessagePasser {
        event MessagePassed(
           uint256 indexed nonce,
           address indexed sender,
           address indexed target,
           uint256 value,
           uint256 gasLimit,
           bytes data,
           bytes32 withdrawalHash
       );

       function initiateWithdrawal(
           address _target,
           uint256 _gasLimit,
           bytes calldata _data
       ) external payable;

       function sentMessages(bytes32) external view returns (bool);
       function messageNonce() external view returns (uint256);
    }
}

/// Withdraw input data.
#[allow(dead_code)]
pub struct Withdraw {
    /// withdrawal contract address
    /// should be the address of L2ToL1MessagePasser
    pub contract: Address,
    pub source: Address,
    pub target: Address,
    pub value: U256,
    pub gas_limit: U256,
    pub data: Bytes,
}

#[allow(dead_code)]
pub struct WithdrawAction<P> {
    provider: P,
    action: Withdraw,
}

impl<P> Action for WithdrawAction<P>
where
    P: Provider + Clone,
{
    async fn is_ready(&self) -> eyre::Result<bool> {
        let balance = self.provider.get_balance(self.action.source).await?;
        Ok(balance >= self.action.value)
    }

    async fn is_completed(&self) -> eyre::Result<bool> {
        // TODO: This needs to be tightened up. especially around idempotency
        //       How do we make sure that without nonce it works?
        //
        // To check if withdrawal is completed, we need to:
        // 1. Compute the expected withdrawal hash
        // 2. Query sentMessages(hash) to see if it's already initiated
        let contract = L2ToL1MessagePasser::new(self.action.contract, &self.provider);

        // Scan recent MessagePassed events to find if our withdrawal exists
        // Filter by sender (indexed), target (indexed)
        let filter = contract
            .MessagePassed_filter()
            .address(self.action.contract)
            .topic1(self.action.source) // sender is indexed
            .topic2(self.action.target); // target is indexed

        let events = filter.query().await?;

        // Check if any event matches our exact parameters
        for (message, _) in events {
            if message.value == self.action.value
                && message.gasLimit == self.action.gas_limit
                && message.data == self.action.data
            {
                // Found our withdrawal - it's completed
                return Ok(true);
            }
        }

        Ok(false)
    }

    async fn execute(&self) -> eyre::Result<crate::Result> {
        if self.is_completed().await? {
            eyre::bail!("Withdrawal already initiated")
        }

        let contract = L2ToL1MessagePasser::new(self.action.contract, &self.provider);

        let tx = contract
            .initiateWithdrawal(
                self.action.target,
                self.action.gas_limit,
                self.action.data.clone(),
            )
            .value(self.action.value)
            .send()
            .await?;

        let receipt = tx.get_receipt().await?;

        let (withdrawal_tx, withdrawal_hash) = parse_message_passed_event(&receipt)?;
        info!(
            tx_hash = %receipt.transaction_hash,
            block_number = receipt.block_number,
            gas_used = receipt.gas_used,
            withdrawal_hash = %withdrawal_hash,
            withdrawal_tx = ?withdrawal_tx,
            "Withdrawal initiated."
        );

        Ok(crate::Result {
            tx_hash: withdrawal_hash,
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
        if let Ok(event) = L2ToL1MessagePasser::MessagePassed::decode_log(&log.inner) {
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
