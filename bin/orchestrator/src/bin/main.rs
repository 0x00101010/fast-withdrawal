use orchestrator::{
    config::Config, maybe_deposit, maybe_initiate_withdrawal, process_pending_withdrawals,
};
use std::time::Duration;
use tokio::time;
use tracing::{info, warn};

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

    let config = Config::from_file(&config_path)?;
    let network = config.network_config();

    info!("Loaded config:");
    info!("  Network: {:?}", config.network);
    info!("  L2 SpokePool: {}", network.unichain.spoke_pool);
    info!("  L1 Portal: {}", network.unichain.l1_portal);
    info!("  EOA: {}", config.eoa_address);
    info!("  Cycle interval: {}s", config.cycle_interval_secs);

    // Create providers
    info!("Connecting to L1...");
    let l1_provider = client::create_provider(&config.l1_rpc_url).await?;

    info!("Connecting to L2...");
    let l2_provider = client::create_provider(&config.l2_rpc_url).await?;

    info!("Starting main loop...");

    let mut interval = time::interval(Duration::from_secs(config.cycle_interval_secs));

    loop {
        interval.tick().await;

        // 1. Process pending withdrawals (finalize + prove)
        if let Err(e) =
            process_pending_withdrawals(l1_provider.clone(), l2_provider.clone(), &config).await
        {
            warn!(error = %e, "Failed to process pending withdrawals");
        }

        // 2. Maybe initiate new withdrawal (L2→L1)
        if let Err(e) = maybe_initiate_withdrawal(l2_provider.clone(), &config).await {
            warn!(error = %e, "Failed to check/initiate withdrawal");
        }

        // 3. Maybe deposit to L2 (L1→L2)
        if let Err(e) = maybe_deposit(l1_provider.clone(), l2_provider.clone(), &config).await {
            warn!(error = %e, "Failed to check/execute deposit");
        }
    }
}
