mod remote_signer;

use alloy_consensus::TxEnvelope;
use alloy_network::{eip2718::Encodable2718, EthereumWallet, TransactionBuilder};
use alloy_primitives::Bytes;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::TransactionRequest;
use alloy_signer_local::PrivateKeySigner;
pub use remote_signer::RemoteSigner;
use std::{future::Future, pin::Pin, sync::Arc};
use thiserror::Error;

/// A function that signs a transaction request and returns signed bytes.
///
/// This type alias matches the one in the `action` crate and allows
/// for both local and remote signing implementations.
pub type SignerFn = Arc<
    dyn Fn(TransactionRequest) -> Pin<Box<dyn Future<Output = eyre::Result<Bytes>> + Send>>
        + Send
        + Sync,
>;

#[derive(Error, Debug)]
pub enum ClientError {
    /// Error parsing or validating URLs
    #[error("Invalid RPC URL: {0}")]
    InvalidUrl(String),

    /// Error connecting to the RPC endpoint
    #[error("Connection error: {0}")]
    Connection(String),

    /// Error with private key
    #[error("Invalid private key: {0}")]
    InvalidPrivateKey(String),

    /// General error with context
    #[error("Client error: {0}")]
    Other(String),
}

/// Convenience function to create an ethereum rpc provider from url.
pub async fn create_provider(rpc_url: &str) -> Result<impl Provider + Clone, ClientError> {
    let url = rpc_url
        .parse()
        .map_err(|e| ClientError::InvalidUrl(format!("{}", e)))?;
    let provider = ProviderBuilder::new().connect_http(url);

    Ok(provider)
}

/// Create a provider with wallet signing capability from a private key.
pub fn create_wallet_provider(
    rpc_url: &str,
    private_key: &str,
) -> Result<impl Provider + Clone, ClientError> {
    let url = rpc_url
        .parse()
        .map_err(|e| ClientError::InvalidUrl(format!("{}", e)))?;

    let signer: PrivateKeySigner = private_key
        .parse()
        .map_err(|e| ClientError::InvalidPrivateKey(format!("{}", e)))?;

    let wallet = EthereumWallet::from(signer);

    let provider = ProviderBuilder::new().wallet(wallet).connect_http(url);

    Ok(provider)
}

/// Create a SignerFn from a RemoteSigner.
///
/// The transaction must be fully filled (nonce, gas, fees, chain_id, from) before
/// being passed to this signer. Use `fill_transaction` at the call site.
pub fn remote_signer_fn(remote: RemoteSigner) -> SignerFn {
    Arc::new(move |tx| {
        let remote = remote.clone();
        Box::pin(async move { remote.sign_transaction(tx).await })
    })
}

/// Create a SignerFn from a local private key.
///
/// The transaction must be fully filled (nonce, gas, fees, chain_id, from) before
/// being passed to this signer. Use `fill_transaction` at the call site.
pub fn local_signer_fn(private_key: &str) -> Result<SignerFn, ClientError> {
    let signer: PrivateKeySigner = private_key
        .parse()
        .map_err(|e| ClientError::InvalidPrivateKey(format!("{}", e)))?;
    let wallet = EthereumWallet::from(signer);

    Ok(Arc::new(move |tx: TransactionRequest| {
        let wallet = wallet.clone();
        Box::pin(async move {
            // Build and sign the typed transaction
            let tx_envelope: TxEnvelope =
                tx.build(&wallet).await.map_err(|e| eyre::eyre!("{}", e))?;

            // Encode to EIP-2718 bytes
            let mut encoded = Vec::new();
            tx_envelope.encode_2718(&mut encoded);
            Ok(Bytes::from(encoded))
        })
    }))
}

/// Fill missing transaction fields using the provider.
///
/// The `from` address must be set on the transaction request before calling this function.
/// This function will fill in chain_id, nonce, gas, and fee parameters if not already set.
pub async fn fill_transaction<P>(
    mut tx: TransactionRequest,
    provider: &P,
) -> eyre::Result<TransactionRequest>
where
    P: Provider,
{
    let from = tx
        .from
        .ok_or_else(|| eyre::eyre!("Transaction must have 'from' address set"))?;

    // Get chain_id from provider if not set
    if tx.chain_id.is_none() {
        tx.chain_id = Some(provider.get_chain_id().await?);
    }

    // Get nonce if not set
    if tx.nonce.is_none() {
        let nonce = provider.get_transaction_count(from).await?;
        tx.nonce = Some(nonce);
    }

    // Get fee parameters if not set (EIP-1559) - do this before gas estimation
    // since gas estimation may need fee info
    if tx.max_fee_per_gas.is_none() || tx.max_priority_fee_per_gas.is_none() {
        let fee_estimate = provider.estimate_eip1559_fees().await?;
        if tx.max_fee_per_gas.is_none() {
            tx.max_fee_per_gas = Some(fee_estimate.max_fee_per_gas);
        }
        if tx.max_priority_fee_per_gas.is_none() {
            tx.max_priority_fee_per_gas = Some(fee_estimate.max_priority_fee_per_gas);
        }
    }

    // Estimate gas if not set
    if tx.gas.is_none() {
        let gas_estimate = provider.estimate_gas(tx.clone()).await?;
        // Add 20% buffer for safety
        tx.gas = Some(gas_estimate + gas_estimate / 5);
    }

    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_invalid_url() {
        let result = create_provider("not a url").await;
        assert!(result.is_err());
    }
}
