//! Prometheus metrics for the orchestrator.
//!
//! All metrics are aggregated in the [`Metrics`] struct for easy tracking and management.

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use std::time::Duration;

/// Aggregated metrics for the orchestrator.
///
/// This struct provides a centralized interface for recording all orchestrator metrics.
/// Metrics are registered with the global metrics registry on creation.
#[derive(Debug, Clone)]
pub struct Metrics {
    _private: (),
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    /// Create a new metrics instance and register all metric descriptions.
    pub fn new() -> Self {
        Self::register_descriptions();
        Self { _private: () }
    }

    /// Register metric descriptions with the global registry.
    fn register_descriptions() {
        // Cycle metrics
        describe_counter!(
            "orchestrator_cycles_total",
            "Total number of orchestrator cycles executed"
        );
        describe_counter!(
            "orchestrator_cycles_success_total",
            "Total number of successful orchestrator cycles"
        );
        describe_counter!(
            "orchestrator_cycles_failure_total",
            "Total number of failed orchestrator cycles"
        );
        describe_histogram!(
            "orchestrator_cycle_duration_seconds",
            "Duration of each orchestrator cycle in seconds"
        );

        // Balance gauges (point-in-time, queried fresh each cycle)
        describe_gauge!(
            "orchestrator_l1_eoa_balance_eth",
            "Current L1 EOA balance in ETH"
        );
        describe_gauge!(
            "orchestrator_l2_eoa_balance_eth",
            "Current L2 EOA balance in ETH"
        );
        describe_gauge!(
            "orchestrator_spoke_pool_balance_eth",
            "Current Unichain SpokePool WETH balance in ETH"
        );

        // In-flight deposits
        describe_gauge!(
            "orchestrator_inflight_deposits_count",
            "Number of deposits currently in flight (initiated but not filled)"
        );
        describe_gauge!(
            "orchestrator_inflight_deposits_eth",
            "Total amount of in-flight deposits in ETH"
        );

        // In-flight withdrawals (total)
        describe_gauge!(
            "orchestrator_inflight_withdrawals_count",
            "Number of withdrawals currently in flight (initiated but not finalized)"
        );
        describe_gauge!(
            "orchestrator_inflight_withdrawals_eth",
            "Total amount of in-flight withdrawals in ETH"
        );

        // In-flight withdrawals (by status)
        describe_gauge!(
            "orchestrator_withdrawals_initiated_count",
            "Number of withdrawals initiated (pending proof)"
        );
        describe_gauge!(
            "orchestrator_withdrawals_initiated_eth",
            "Total amount of initiated withdrawals in ETH"
        );
        describe_gauge!(
            "orchestrator_withdrawals_proven_count",
            "Number of withdrawals proven (pending finalization)"
        );
        describe_gauge!(
            "orchestrator_withdrawals_proven_eth",
            "Total amount of proven withdrawals in ETH"
        );
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Cycle metrics
    // ─────────────────────────────────────────────────────────────────────────────

    /// Record a completed cycle.
    pub fn record_cycle(&self, success: bool, duration: Duration) {
        counter!("orchestrator_cycles_total").increment(1);
        histogram!("orchestrator_cycle_duration_seconds").record(duration.as_secs_f64());

        if success {
            counter!("orchestrator_cycles_success_total").increment(1);
        } else {
            counter!("orchestrator_cycles_failure_total").increment(1);
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Balance gauges
    // ─────────────────────────────────────────────────────────────────────────────

    /// Set the current L1 EOA balance in ETH.
    pub fn set_l1_eoa_balance_eth(&self, balance_eth: f64) {
        gauge!("orchestrator_l1_eoa_balance_eth").set(balance_eth);
    }

    /// Set the current L2 EOA balance in ETH.
    pub fn set_l2_eoa_balance_eth(&self, balance_eth: f64) {
        gauge!("orchestrator_l2_eoa_balance_eth").set(balance_eth);
    }

    /// Set the current Unichain SpokePool WETH balance in ETH.
    pub fn set_spoke_pool_balance_eth(&self, balance_eth: f64) {
        gauge!("orchestrator_spoke_pool_balance_eth").set(balance_eth);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // In-flight deposits
    // ─────────────────────────────────────────────────────────────────────────────

    /// Set the current in-flight deposit count and total amount.
    pub fn set_inflight_deposits(&self, count: usize, amount_eth: f64) {
        gauge!("orchestrator_inflight_deposits_count").set(count as f64);
        gauge!("orchestrator_inflight_deposits_eth").set(amount_eth);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // In-flight withdrawals
    // ─────────────────────────────────────────────────────────────────────────────

    /// Set the current in-flight withdrawal totals and breakdown by status.
    pub fn set_inflight_withdrawals(
        &self,
        initiated_count: usize,
        initiated_eth: f64,
        proven_count: usize,
        proven_eth: f64,
    ) {
        // Total in-flight
        let total_count = initiated_count + proven_count;
        let total_eth = initiated_eth + proven_eth;
        gauge!("orchestrator_inflight_withdrawals_count").set(total_count as f64);
        gauge!("orchestrator_inflight_withdrawals_eth").set(total_eth);

        // By status
        gauge!("orchestrator_withdrawals_initiated_count").set(initiated_count as f64);
        gauge!("orchestrator_withdrawals_initiated_eth").set(initiated_eth);
        gauge!("orchestrator_withdrawals_proven_count").set(proven_count as f64);
        gauge!("orchestrator_withdrawals_proven_eth").set(proven_eth);
    }
}

/// Install the Prometheus metrics exporter and start the HTTP server.
///
/// Returns an error if the server fails to bind to the specified port.
pub fn install_prometheus_exporter(port: u16) -> eyre::Result<()> {
    use metrics_exporter_prometheus::PrometheusBuilder;
    use std::net::SocketAddr;

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    PrometheusBuilder::new()
        .with_http_listener(addr)
        .install()
        .map_err(|e| eyre::eyre!("Failed to install Prometheus exporter: {}", e))?;

    Ok(())
}
