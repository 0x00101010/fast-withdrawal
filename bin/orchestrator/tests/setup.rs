//! Common test setup utilities shared across integration tests.
#![allow(dead_code)] // used in ignored tests

use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer_local::PrivateKeySigner;
use orchestrator::config::Config;
use serde::Deserialize;

/// Local configuration with private key (git-ignored file)
#[derive(Debug, Deserialize)]
struct LocalConfig {
    private_key: String,
}

/// Load test configuration. Panics if not found or invalid.
pub fn load_test_config() -> Config {
    let config_path = "tests/test-config.toml";

    // Debug: print current directory
    let current_dir = std::env::current_dir().unwrap();
    eprintln!("Current working directory: {:?}", current_dir);
    eprintln!("Looking for config at: {:?}", current_dir.join(config_path));

    Config::from_file(config_path).expect("Failed to load tests/test-config.toml.")
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
    let local_config_path = "tests/test-config.local.toml";
    if let Ok(contents) = std::fs::read_to_string(local_config_path) {
        if let Ok(config) = toml::from_str::<LocalConfig>(&contents) {
            eprintln!("✓ Loaded private key from {}", local_config_path);
            return Some(config.private_key);
        }
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
