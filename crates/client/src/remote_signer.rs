//! Remote transaction signer that delegates signing to a signer-proxy service.
//!
//! The remote signer sends `eth_signTransaction` JSON-RPC requests to a proxy service,
//! which handles the actual signing (typically via an HSM or secure enclave).

use alloy_primitives::{Address, Bytes};
use alloy_rpc_types::eth::TransactionRequest;
use eyre::{bail, Result};
use serde::{Deserialize, Serialize};

/// A remote signer that delegates transaction signing to a signer-proxy service.
///
/// This signer sends `eth_signTransaction` requests over HTTP to a remote signing service
/// and returns the signed raw transaction bytes ready for broadcast.
///
/// # Example
///
/// ```ignore
/// let signer = RemoteSigner::new("http://localhost:9060", address, 1);
/// let signed_tx = signer.sign_transaction(tx_request).await?;
/// provider.send_raw_transaction(&signed_tx).await?;
/// ```
#[derive(Debug, Clone)]
pub struct RemoteSigner {
    client: reqwest::Client,
    proxy_url: String,
    address: Address,
    chain_id: u64,
}

impl RemoteSigner {
    /// Creates a new remote signer.
    ///
    /// # Arguments
    /// * `proxy_url` - The URL of the signer-proxy service (e.g., "http://localhost:9060")
    /// * `address` - The Ethereum address of the signer
    /// * `chain_id` - The chain ID for EIP-155 replay protection
    pub fn new(proxy_url: impl Into<String>, address: Address, chain_id: u64) -> Self {
        Self {
            client: reqwest::Client::new(),
            proxy_url: proxy_url.into(),
            address,
            chain_id,
        }
    }

    /// Creates a new remote signer with a custom HTTP client.
    pub fn with_client(
        client: reqwest::Client,
        proxy_url: impl Into<String>,
        address: Address,
        chain_id: u64,
    ) -> Self {
        Self {
            client,
            proxy_url: proxy_url.into(),
            address,
            chain_id,
        }
    }

    /// Returns the signer's address.
    pub const fn address(&self) -> Address {
        self.address
    }

    /// Returns the chain ID.
    pub const fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Signs a transaction via the remote signer-proxy.
    ///
    /// Returns the signed transaction as raw bytes, ready to be broadcast
    /// via `provider.send_raw_transaction()`.
    pub async fn sign_transaction(&self, tx: TransactionRequest) -> Result<Bytes> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            method: "eth_signTransaction",
            params: [tx],
            id: 1,
        };

        let response = self
            .client
            .post(&self.proxy_url)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            bail!("signer-proxy returned {status}: {body}");
        }

        let rpc_response: JsonRpcResponse<SignedTransactionResponse> = response.json().await?;

        match rpc_response.result {
            Some(result) => {
                let bytes: Bytes = result.raw.parse()?;
                Ok(bytes)
            }
            None => {
                let error = rpc_response.error.unwrap_or(JsonRpcError {
                    code: -1,
                    message: "unknown error".to_string(),
                });
                bail!("JSON-RPC error {}: {}", error.code, error.message);
            }
        }
    }

    /// Helper to build a transaction request with the signer's address and chain ID pre-filled.
    pub fn build_transaction(&self) -> TransactionRequest {
        TransactionRequest {
            from: Some(self.address),
            chain_id: Some(self.chain_id),
            ..Default::default()
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonRpcRequest<T> {
    jsonrpc: &'static str,
    method: &'static str,
    params: T,
    id: u32,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// Response from eth_signTransaction containing the signed transaction.
#[derive(Debug, Deserialize)]
struct SignedTransactionResponse {
    /// The signed transaction as hex-encoded RLP.
    raw: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn test_build_transaction() {
        let signer = RemoteSigner::new(
            "http://localhost:9060",
            address!("5CFFA347b0aE99cc01E5c01714cA5658e54a23D1"),
            1,
        );

        let tx = signer.build_transaction();
        assert_eq!(tx.from, Some(signer.address()));
        assert_eq!(tx.chain_id, Some(1));
    }
}
