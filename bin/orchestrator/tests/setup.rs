//! Common test setup utilities shared across integration tests.
#![allow(dead_code)] // used in ignored tests

use action::SignerFn;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer_local::PrivateKeySigner;
use orchestrator::config::Config;
use serde::Deserialize;
use std::sync::Arc;

/// Local configuration overrides (git-ignored file)
#[derive(Debug, Default, Deserialize)]
struct LocalConfig {
    private_key: Option<String>,
    l1_rpc_url: Option<String>,
    l2_rpc_url: Option<String>,
}

/// Load local config overrides from tests/test-config.local.toml
fn load_local_config() -> LocalConfig {
    let local_config_path = "tests/test-config.local.toml";
    if let Ok(contents) = std::fs::read_to_string(local_config_path) {
        if let Ok(config) = toml::from_str::<LocalConfig>(&contents) {
            return config;
        }
    }
    LocalConfig::default()
}

/// Load test configuration. Panics if not found or invalid.
///
/// Supports overrides via environment variables or tests/test-config.local.toml:
/// - L1_RPC_URL: Override L1 RPC endpoint
/// - L2_RPC_URL: Override L2 RPC endpoint
pub fn load_test_config() -> Config {
    let config_path = "tests/test-config.toml";

    // Debug: print current directory
    let current_dir = std::env::current_dir().unwrap();
    eprintln!("Current working directory: {:?}", current_dir);
    eprintln!("Looking for config at: {:?}", current_dir.join(config_path));

    let mut config =
        Config::from_file(config_path).expect("Failed to load tests/test-config.toml.");

    // Load local overrides
    let local = load_local_config();

    // Override L1 RPC URL (env var takes priority over local config)
    if let Ok(url) = std::env::var("L1_RPC_URL") {
        eprintln!("✓ Overriding L1 RPC URL from L1_RPC_URL env var");
        config.l1_rpc_url = url;
    } else if let Some(url) = local.l1_rpc_url {
        eprintln!("✓ Overriding L1 RPC URL from test-config.local.toml");
        config.l1_rpc_url = url;
    }

    // Override L2 RPC URL (env var takes priority over local config)
    if let Ok(url) = std::env::var("L2_RPC_URL") {
        eprintln!("✓ Overriding L2 RPC URL from L2_RPC_URL env var");
        config.l2_rpc_url = url;
    } else if let Some(url) = local.l2_rpc_url {
        eprintln!("✓ Overriding L2 RPC URL from test-config.local.toml");
        config.l2_rpc_url = url;
    }

    config
}

/// Load private key for signing transactions.
///
/// Tries multiple sources in order:
/// 1. PRIVATE_KEY environment variable
/// 2. tests/test-config.local.toml file (git-ignored)
///
/// Returns None if no private key is found.
pub fn load_private_key() -> Option<String> {
    // Try 1: Environment variable
    if let Ok(pk) = std::env::var("PRIVATE_KEY") {
        eprintln!("✓ Loaded private key from PRIVATE_KEY environment variable");
        return Some(pk);
    }

    // Try 2: Local config file (git-ignored)
    let local = load_local_config();
    if let Some(pk) = local.private_key {
        eprintln!("✓ Loaded private key from test-config.local.toml");
        return Some(pk);
    }

    eprintln!("⚠ No private key found. Checked:");
    eprintln!("  1. PRIVATE_KEY environment variable");
    eprintln!("  2. tests/test-config.local.toml file");
    None
}

/// Common test setup: load config and create provider
pub async fn setup_provider(url: &str) -> impl Provider + Clone {
    client::create_provider(url)
        .await
        .expect("Failed to create L1 provider")
}

/// Create a wallet provider for signing transactions.
///
/// Requires a private key from either:
/// - PRIVATE_KEY environment variable, or
/// - tests/test-config.local.toml file
///
/// # Panics
/// Panics if no private key is found or if the private key is invalid.
pub async fn setup_wallet_provider(url: &str) -> impl Provider + Clone {
    let private_key = load_private_key().expect(
        "Private key required for transaction signing.\n\
         Set PRIVATE_KEY environment variable or create tests/test-config.local.toml\n\
         See tests/test-config.local.toml.example for template.",
    );

    // Parse private key (handles with or without 0x prefix)
    let signer: PrivateKeySigner = private_key
        .parse()
        .expect("Invalid private key format. Expected hex string with optional 0x prefix.");

    eprintln!("✓ Created signer with address: {}", signer.address());

    // Create wallet from signer
    let wallet = alloy_network::EthereumWallet::from(signer);

    // Build provider with wallet for signing
    let url = url.parse().expect("Invalid L1 RPC URL");
    ProviderBuilder::new().wallet(wallet).connect_http(url)
}

/// Create a mock signer for tests that don't execute transactions.
/// Will panic if actually called.
pub fn mock_signer() -> SignerFn {
    Arc::new(|_tx| Box::pin(async { panic!("mock signer should not be called") }))
}

/// Create a real SignerFn for tests that do execute transactions.
///
/// Requires a private key from either:
/// - PRIVATE_KEY environment variable, or
/// - tests/test-config.local.toml file
///
/// # Panics
/// Panics if no private key is found or if the private key is invalid.
pub fn setup_signer<P>(chain_id: u64, provider: P) -> SignerFn
where
    P: alloy_provider::Provider + Clone + 'static,
{
    let private_key = load_private_key().expect(
        "Private key required for transaction signing.\n\
         Set PRIVATE_KEY environment variable or create tests/test-config.local.toml\n\
         See tests/test-config.local.toml.example for template.",
    );

    client::local_signer_fn(&private_key, chain_id, provider)
        .expect("Failed to create local signer")
}
