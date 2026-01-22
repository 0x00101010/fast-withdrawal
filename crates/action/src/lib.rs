pub mod claim;
pub mod deposit;
pub mod withdraw;

use alloy_primitives::{TxHash, U256};
use std::future::Future;

/// Trait for executable onchain actions.
pub trait Action: Send + Sync {
    /// Check to see if the action is ready to be executed.
    ///
    /// Returns true if all predictions are met.
    fn is_ready(&self) -> impl Future<Output = eyre::Result<bool>> + Send;

    /// Check if the action has already been completed.
    ///
    /// Returns true if the action was already executed successfully.
    fn is_completed(&self) -> impl Future<Output = eyre::Result<bool>> + Send;

    /// Execute the action.
    ///
    /// Returns the transaction hash of the executed action.
    fn execute(&self) -> impl Future<Output = eyre::Result<Result>> + Send;

    /// Get a human-readable description of this action.
    fn description(&self) -> String;
}

/// Result of an action.
pub struct Result {
    /// Transaction hash
    pub tx_hash: TxHash,
    /// Block number where transaction was included
    pub block_number: Option<u64>,
    /// Gas used
    pub gas_used: Option<U256>,
}

#[cfg(test)]
pub(crate) mod test_utils {
    use alloy_provider::{network::Ethereum, Provider, RootProvider};

    /// Mock provider for unit tests.
    #[derive(Clone)]
    pub struct MockProvider;

    impl Provider for MockProvider {
        fn root(&self) -> &RootProvider<Ethereum> {
            todo!()
        }
    }
}
