use crate::Action;
use alloy_primitives::{Address, Bytes, U256};
use alloy_provider::Provider;
use alloy_sol_types::sol;

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
        todo!()
    }

    async fn execute(&self) -> eyre::Result<crate::Result> {
        todo!()
    }

    fn description(&self) -> String {
        todo!()
    }
}
