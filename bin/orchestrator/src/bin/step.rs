//! CLI tool to run individual orchestrator steps for testing.
//!
//! This binary allows running each main loop step independently:
//! - `process-withdrawals`: Process pending L2→L1 withdrawals (prove + finalize)
//! - `initiate-withdrawal`: Check L2 EOA balance and initiate withdrawal if threshold met
//! - `deposit`: Check SpokePool balance and deposit from L1 if needed

use clap::{Parser, Subcommand};
use orchestrator::{
    config::Config, maybe_deposit, maybe_initiate_withdrawal, process_pending_withdrawals,
};
use tracing::info;

#[derive(Parser)]
#[command(name = "step")]
#[command(about = "Run individual orchestrator steps for testing")]
struct Cli {
    /// Path to the configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: String,

    /// Private key for signing transactions (hex string, with or without 0x prefix)
    #[arg(short = 'k', long, env = "PRIVATE_KEY")]
    private_key: String,

    /// Dry-run mode: log actions without executing transactions
    #[arg(long)]
    dry_run: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Process pending L2→L1 withdrawals (prove + finalize)
    ProcessWithdrawals,

    /// Check L2 EOA balance and initiate withdrawal if threshold met
    InitiateWithdrawal,

    /// Check SpokePool balance and deposit from L1 if needed
    Deposit,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let mut config = Config::from_file(&cli.config)?;

    // Override dry_run from CLI flag
    if cli.dry_run {
        config.dry_run = true;
    }

    let network = config.network_config();

    info!("Loaded config:");
    info!("  Network: {:?}", config.network);
    info!("  L2 SpokePool: {}", network.unichain.spoke_pool);
    info!("  L1 Portal: {}", network.unichain.l1_portal);
    info!("  EOA: {}", config.eoa_address);
    if config.dry_run {
        info!("  Mode: DRY-RUN (no transactions will be executed)");
    }

    match cli.command {
        Command::ProcessWithdrawals => {
            info!("Running: process-withdrawals");

            let l1_provider = client::create_wallet_provider(&config.l1_rpc_url, &cli.private_key)?;
            let l2_provider = client::create_wallet_provider(&config.l2_rpc_url, &cli.private_key)?;

            process_pending_withdrawals(l1_provider, l2_provider, &config).await?;

            info!("Step completed: process-withdrawals");
        }
        Command::InitiateWithdrawal => {
            info!("Running: initiate-withdrawal");

            let l2_provider = client::create_wallet_provider(&config.l2_rpc_url, &cli.private_key)?;

            let result = maybe_initiate_withdrawal(l2_provider, &config).await?;

            match result {
                Some(amount) => {
                    info!(amount = %alloy_primitives::utils::format_ether(amount), "Withdrawal initiated");
                }
                None => {
                    info!("No withdrawal initiated (threshold not met or nothing to withdraw)");
                }
            }

            info!("Step completed: initiate-withdrawal");
        }
        Command::Deposit => {
            info!("Running: deposit");

            let l1_provider = client::create_wallet_provider(&config.l1_rpc_url, &cli.private_key)?;
            let l2_provider = client::create_wallet_provider(&config.l2_rpc_url, &cli.private_key)?;

            let result = maybe_deposit(l1_provider, l2_provider, &config).await?;

            match result {
                Some(amount) => {
                    info!(amount = %alloy_primitives::utils::format_ether(amount), "Deposit executed");
                }
                None => {
                    info!("No deposit executed (conditions not met)");
                }
            }

            info!("Step completed: deposit");
        }
    }

    Ok(())
}
