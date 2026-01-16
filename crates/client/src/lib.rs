use alloy_provider::{Provider, ProviderBuilder};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientError {
    /// Error parsing or validating URLs
    #[error("Invalid RPC URL: {0}")]
    InvalidUrl(String),

    /// Error connecting to the RPC endpoint
    #[error("Connection error: {0}")]
    Connection(String),

    /// General error with context
    #[error("Client error: {0}")]
    Other(String),
}

/// Convenience function to create a ethereum rpc provider from url.
pub async fn create_provider(rpc_url: &str) -> Result<impl Provider, ClientError> {
    let url = rpc_url
        .parse()
        .map_err(|e| ClientError::InvalidUrl(format!("{}", e)))?;
    let provider = ProviderBuilder::new().connect_http(url);

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
