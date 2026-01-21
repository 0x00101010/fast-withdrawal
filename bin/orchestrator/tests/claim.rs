#[path = "setup.rs"]
mod setup;

use action::{
    claim::{Claim, ClaimAction},
    Action,
};
use alloy_primitives::Address;
use config::NetworkConfig;
use setup::{load_test_config, setup_provider};

const fn create_claim(relayer: Address) -> Claim {
    let network_config = NetworkConfig::sepolia();

    Claim {
        spoke_pool: network_config.unichain.spoke_pool,
        token: network_config.unichain.weth,
        refund_address: relayer,
        relayer,
    }
}

#[tokio::test]
async fn test_get_claimable_balance() -> eyre::Result<()> {
    let config = load_test_config();
    let claim = create_claim(config.eoa_address);

    println!("\n=== Claim Test Configuration ===");
    println!("L2 RPC URL: {}", config.l2_rpc_url);
    println!("EOA Address: {}", config.eoa_address);
    println!("\n=== Claim Details ===");
    println!("Spoke Pool: {}", claim.spoke_pool);
    println!("Token (WETH): {}", claim.token);
    println!("Refund Address: {}", claim.refund_address);
    println!("Relayer: {}", claim.relayer);
    println!();

    let provider = setup_provider(&config.l2_rpc_url).await;
    let action = ClaimAction::new(provider, claim);

    // Verify action is ready
    println!("Checking if action is ready...");
    assert!(action.is_ready(), "Claim action should be ready");
    println!("✓ Action is ready");

    // Get claimable balance
    println!("\nQuerying claimable balance...");
    let result = action.get_claimable_balance().await?;

    println!("✓ Successfully retrieved claimable balance");
    println!("Claimable balance: {} wei", result);
    println!(
        "Claimable balance: {} ETH",
        alloy_primitives::utils::format_ether(result)
    );

    Ok(())
}
