pub mod claim;
pub mod deposit;
pub mod finalize;
pub mod prove;
pub mod withdraw;

use alloy_primitives::{Bytes, TxHash, U256};
use alloy_rpc_types::TransactionRequest;
pub use client::fill_transaction;
use std::{future::Future, pin::Pin, sync::Arc};

/// A function that signs a transaction request and returns signed bytes.
///
/// This abstraction allows actions to work with both local wallet signing
/// and remote signing via a signer-proxy service.
pub type SignerFn = Arc<
    dyn Fn(TransactionRequest) -> Pin<Box<dyn Future<Output = eyre::Result<Bytes>> + Send>>
        + Send
        + Sync,
>;

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
    fn execute(&mut self) -> impl Future<Output = eyre::Result<Result>> + Send;

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
    use super::SignerFn;
    use alloy_provider::{network::Ethereum, Provider, RootProvider};
    use std::sync::Arc;

    /// Mock provider for unit tests.
    #[derive(Clone)]
    pub struct MockProvider;

    impl Provider for MockProvider {
        fn root(&self) -> &RootProvider<Ethereum> {
            todo!()
        }
    }

    /// Create a mock signer for testing that panics if called.
    /// Used for tests that don't actually execute transactions.
    pub fn mock_signer() -> SignerFn {
        Arc::new(|_tx| Box::pin(async { panic!("mock signer should not be called") }))
    }
}
