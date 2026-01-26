use alloy_network::EthereumWallet;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer_local::PrivateKeySigner;
use thiserror::Error;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_invalid_url() {
        let result = create_provider("not a url").await;
        assert!(result.is_err());
    }
}
