mod remote_signer;

use alloy_consensus::TxEnvelope;
use alloy_network::{eip2718::Encodable2718, EthereumWallet, TransactionBuilder};
use alloy_primitives::{Address, Bytes};
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

/// Create a SignerFn from a RemoteSigner and provider.
///
/// The provider is used to fill transaction fields (nonce, gas, fees) before
/// sending to the remote signer-proxy for signing.
pub fn remote_signer_fn<P>(remote: RemoteSigner, provider: P) -> SignerFn
where
    P: Provider + Clone + 'static,
{
    let from_address = remote.address();
    let chain_id = remote.chain_id();

    Arc::new(move |tx| {
        let remote = remote.clone();
        let provider = provider.clone();
        Box::pin(async move {
            let filled_tx = fill_transaction(tx, &provider, from_address, chain_id).await?;
            remote.sign_transaction(filled_tx).await
        })
    })
}

/// Create a SignerFn from a local private key and provider.
///
/// The provider is used to fill transaction fields (nonce, gas, fees) before
/// signing locally with the private key.
pub fn local_signer_fn<P>(
    private_key: &str,
    chain_id: u64,
    provider: P,
) -> Result<SignerFn, ClientError>
where
    P: Provider + Clone + 'static,
{
    let signer: PrivateKeySigner = private_key
        .parse()
        .map_err(|e| ClientError::InvalidPrivateKey(format!("{}", e)))?;
    let from_address = signer.address();
    let wallet = EthereumWallet::from(signer);

    Ok(Arc::new(move |tx: TransactionRequest| {
        let wallet = wallet.clone();
        let provider = provider.clone();
        Box::pin(async move {
            let filled_tx = fill_transaction(tx, &provider, from_address, chain_id).await?;

            // Build and sign the typed transaction
            let tx_envelope: TxEnvelope = filled_tx
                .build(&wallet)
                .await
                .map_err(|e| eyre::eyre!("{}", e))?;

            // Encode to EIP-2718 bytes
            let mut encoded = Vec::new();
            tx_envelope.encode_2718(&mut encoded);
            Ok(Bytes::from(encoded))
        })
    }))
}

/// Fill missing transaction fields using the provider.
async fn fill_transaction<P>(
    mut tx: TransactionRequest,
    provider: &P,
    from: Address,
    chain_id: u64,
) -> eyre::Result<TransactionRequest>
where
    P: Provider,
{
    // Set from address
    if tx.from.is_none() {
        tx.from = Some(from);
    }

    // Set chain_id
    if tx.chain_id.is_none() {
        tx.chain_id = Some(chain_id);
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
