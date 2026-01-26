use orchestrator::{
    config::Config, maybe_deposit, maybe_initiate_withdrawal, process_pending_withdrawals,
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

/// Cumulative metrics for the orchestrator
#[derive(Debug, Default)]
struct CycleMetrics {
    total_successes: u64,
    total_failures: u64,
}

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
    let mut metrics = CycleMetrics::default();

    loop {
        interval.tick().await;
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

        // Update cumulative metrics
        let cycle_duration = cycle_start.elapsed();
        let has_failure = process_result.is_failure()
            || initiate_result.is_failure()
            || deposit_result.is_failure();

        if has_failure {
            metrics.total_failures += 1;
        } else {
            metrics.total_successes += 1;
        }

        // Log cycle summary
        info!(
            "Cycle {} completed in {:.1}s: process_withdrawals={}, initiate_withdrawal={}, deposit={}",
            cycle_number,
            cycle_duration.as_secs_f64(),
            process_result.as_str(),
            initiate_result.as_str(),
            deposit_result.as_str(),
        );
        info!(
            "Cumulative: successes={}, failures={}",
            metrics.total_successes, metrics.total_failures
        );

        // Check if shutdown was requested after completing the cycle
        if shutdown_requested.load(Ordering::SeqCst) {
            info!("Cycle completed, shutting down gracefully");
            break;
        }
    }

    Ok(())
}
