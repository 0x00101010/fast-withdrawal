use clap::Parser;
use orchestrator::{
    config::Config,
    maybe_deposit, maybe_initiate_withdrawal,
    metrics::{install_prometheus_exporter, Metrics},
    process_pending_withdrawals,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::time;
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "orchestrator")]
#[command(about = "Fast-withdrawal orchestrator for Unichain")]
struct Cli {
    /// Path to the configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: String,

    /// Dry-run mode: log actions without executing transactions
    #[arg(long)]
    dry_run: bool,
}

/// Result status for a cycle step
#[derive(Debug, Clone, Copy)]
enum StepResult {
    Ok,
    Failed,
    #[allow(dead_code)]
    Skipped,
}

impl StepResult {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    const fn is_failure(self) -> bool {
        matches!(self, Self::Failed)
    }
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

    info!("Starting Orchestrator");

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
    info!("  Cycle interval: {}s", config.cycle_interval_secs);
    info!("  Dry-run: {}", config.dry_run);
    info!("  Metrics port: {}", config.metrics_port);

    if config.dry_run {
        warn!("=== DRY-RUN MODE: No transactions will be submitted ===");
    }

    // Start Prometheus metrics server
    info!("Starting metrics server on port {}...", config.metrics_port);
    install_prometheus_exporter(config.metrics_port)?;
    let metrics = Metrics::new();

    // Create providers
    info!("Connecting to L1...");
    let l1_provider = client::create_provider(&config.l1_rpc_url).await?;

    info!("Connecting to L2...");
    let l2_provider = client::create_provider(&config.l2_rpc_url).await?;

    // Set up graceful shutdown handling
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let shutdown_flag = shutdown_requested.clone();

    tokio::spawn(async move {
        let mut sigint =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap();
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();

        tokio::select! {
            _ = sigint.recv() => {
                info!("Received shutdown signal, completing current cycle...");
            }
            _ = sigterm.recv() => {
                info!("Received shutdown signal, completing current cycle...");
            }
        }

        shutdown_flag.store(true, Ordering::SeqCst);
    });

    info!("Starting main loop...");

    let mut interval = time::interval(Duration::from_secs(config.cycle_interval_secs));
    let mut cycle_number: u64 = 0;

    loop {
        // Wait for next tick OR shutdown signal
        tokio::select! {
            _ = interval.tick() => {}
            _ = async {
                while !shutdown_requested.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            } => {
                info!("Shutdown signal received, exiting immediately");
                break;
            }
        }

        // Check again in case we woke up from interval but shutdown was requested
        if shutdown_requested.load(Ordering::SeqCst) {
            info!("Shutdown signal received, exiting immediately");
            break;
        }

        cycle_number += 1;
        let cycle_start = Instant::now();

        // 1. Process pending withdrawals (finalize + prove)
        let process_result =
            match process_pending_withdrawals(l1_provider.clone(), l2_provider.clone(), &config)
                .await
            {
                Ok(_) => StepResult::Ok,
                Err(e) => {
                    warn!(error = %e, "Failed to process pending withdrawals");
                    StepResult::Failed
                }
            };

        // 2. Maybe initiate new withdrawal (L2->L1)
        let initiate_result = match maybe_initiate_withdrawal(l2_provider.clone(), &config).await {
            Ok(_) => StepResult::Ok,
            Err(e) => {
                warn!(error = %e, "Failed to check/initiate withdrawal");
                StepResult::Failed
            }
        };

        // 3. Maybe deposit to L2 (L1->L2)
        let deposit_result =
            match maybe_deposit(l1_provider.clone(), l2_provider.clone(), &config).await {
                Ok(_) => StepResult::Ok,
                Err(e) => {
                    warn!(error = %e, "Failed to check/execute deposit");
                    StepResult::Failed
                }
            };

        // Update metrics
        let cycle_duration = cycle_start.elapsed();
        let has_failure = process_result.is_failure()
            || initiate_result.is_failure()
            || deposit_result.is_failure();

        metrics.record_cycle(!has_failure, cycle_duration);

        // Log cycle summary
        let dry_run_marker = if config.dry_run { " [DRY-RUN]" } else { "" };
        info!(
            "Cycle {}{} completed in {:.1}s: process_withdrawals={}, initiate_withdrawal={}, deposit={}",
            cycle_number,
            dry_run_marker,
            cycle_duration.as_secs_f64(),
            process_result.as_str(),
            initiate_result.as_str(),
            deposit_result.as_str(),
        );

        // Check if shutdown was requested after completing the cycle
        if shutdown_requested.load(Ordering::SeqCst) {
            info!("Cycle completed, shutting down gracefully");
            break;
        }
    }

    Ok(())
}
