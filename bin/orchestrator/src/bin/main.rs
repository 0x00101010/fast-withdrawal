use alloy_primitives::Address;
use balance::monitor::BalanceMonitor;
use config::NetworkConfig;
use orchestrator::{check_l2_spoke_pool_balance, config::Config};
use std::time::Duration;
use tokio::time;
use tracing::{error, info};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Starting Orchestrator");

    // TODO: use proper cli lib
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());

    // Debug: print current directory and full path
    let current_dir = std::env::current_dir()?;
    info!("Current working directory: {:?}", current_dir);
    info!("Loading config: {}", config_path);
    info!("Full config path: {:?}", current_dir.join(&config_path));

    let config = Config::from_file(&config_path)?;
    // TODO: make network configurable
    let network_config = NetworkConfig::sepolia();
    let spoke_pool = network_config.unichain.spoke_pool;

    info!("Loaded config:");
    info!("  L1 RPC URL: {}", config.l1_rpc_url);
    info!("  L2 RPC URL: {}", config.l2_rpc_url);
    info!("  L2 SpokePool: {}", spoke_pool);
    info!("  EOA: {}", config.eoa_address);

    // Create L1 provider and monitor
    info!("Connecting to L1...");
    let l1_provider = client::create_provider(&config.l1_rpc_url).await?;
    let _l1_monitor = BalanceMonitor::new(l1_provider);

    // Create L2 provider and monitor
    let l2_provider = client::create_provider(&config.l2_rpc_url).await?;
    let l2_monitor = BalanceMonitor::new(l2_provider);

    // TODO: (make interval configurable)
    info!("Starting monitoring loop...");

    let mut interval = time::interval(Duration::from_secs(30));

    loop {
        match check_l2_spoke_pool_balance(
            &l2_monitor,
            spoke_pool,
            Address::ZERO, // ETH
            config.eoa_address,
        )
        .await
        {
            Ok(balance) => {
                // TODO: add more complex strategy here.
                info!("L2 Spoke Pool Balance: {} ETH", balance.amount);
            }
            Err(e) => {
                error!("Failed to query L2 SpokePool balance: {}", e);
                continue;
            }
        }

        interval.tick().await;
    }
}
