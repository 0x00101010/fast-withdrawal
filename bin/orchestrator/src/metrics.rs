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

        // Step metrics
        describe_counter!(
            "orchestrator_step_success_total",
            "Total successful step executions by step name"
        );
        describe_counter!(
            "orchestrator_step_failure_total",
            "Total failed step executions by step name"
        );

        // Withdrawal metrics
        describe_counter!(
            "orchestrator_withdrawals_initiated_total",
            "Total number of L2→L1 withdrawals initiated"
        );
        describe_counter!(
            "orchestrator_withdrawals_proven_total",
            "Total number of withdrawals proven on L1"
        );
        describe_counter!(
            "orchestrator_withdrawals_finalized_total",
            "Total number of withdrawals finalized on L1"
        );
        describe_counter!(
            "orchestrator_withdrawal_amount_wei_total",
            "Total amount withdrawn in wei"
        );

        // Deposit metrics
        describe_counter!(
            "orchestrator_deposits_total",
            "Total number of L1→L2 deposits executed"
        );
        describe_counter!(
            "orchestrator_deposit_amount_wei_total",
            "Total amount deposited in wei"
        );

        // Balance metrics (gauges - current values)
        describe_gauge!(
            "orchestrator_l2_eoa_balance_wei",
            "Current L2 EOA balance in wei"
        );
        describe_gauge!(
            "orchestrator_l1_eoa_balance_wei",
            "Current L1 EOA balance in wei"
        );
        describe_gauge!(
            "orchestrator_spoke_pool_balance_wei",
            "Current L2 SpokePool WETH balance in wei"
        );
        describe_gauge!(
            "orchestrator_inflight_deposits_wei",
            "Total in-flight deposit amount in wei"
        );

        // Pending withdrawal counts
        describe_gauge!(
            "orchestrator_pending_withdrawals",
            "Number of pending withdrawals by status"
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
    // Step metrics
    // ─────────────────────────────────────────────────────────────────────────────

    /// Record a step success.
    pub fn record_step_success(&self, step: &str) {
        counter!("orchestrator_step_success_total", "step" => step.to_string()).increment(1);
    }

    /// Record a step failure.
    pub fn record_step_failure(&self, step: &str) {
        counter!("orchestrator_step_failure_total", "step" => step.to_string()).increment(1);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Withdrawal metrics
    // ─────────────────────────────────────────────────────────────────────────────

    /// Record a withdrawal initiation.
    pub fn record_withdrawal_initiated(&self, amount_wei: u128) {
        counter!("orchestrator_withdrawals_initiated_total").increment(1);
        counter!("orchestrator_withdrawal_amount_wei_total").increment(amount_wei as u64);
    }

    /// Record a withdrawal proven.
    pub fn record_withdrawal_proven(&self) {
        counter!("orchestrator_withdrawals_proven_total").increment(1);
    }

    /// Record a withdrawal finalized.
    pub fn record_withdrawal_finalized(&self) {
        counter!("orchestrator_withdrawals_finalized_total").increment(1);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Deposit metrics
    // ─────────────────────────────────────────────────────────────────────────────

    /// Record a deposit execution.
    pub fn record_deposit(&self, amount_wei: u128) {
        counter!("orchestrator_deposits_total").increment(1);
        counter!("orchestrator_deposit_amount_wei_total").increment(amount_wei as u64);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Balance metrics (gauges)
    // ─────────────────────────────────────────────────────────────────────────────

    /// Set the current L2 EOA balance.
    pub fn set_l2_eoa_balance(&self, balance_wei: u128) {
        gauge!("orchestrator_l2_eoa_balance_wei").set(balance_wei as f64);
    }

    /// Set the current L1 EOA balance.
    pub fn set_l1_eoa_balance(&self, balance_wei: u128) {
        gauge!("orchestrator_l1_eoa_balance_wei").set(balance_wei as f64);
    }

    /// Set the current L2 SpokePool balance.
    pub fn set_spoke_pool_balance(&self, balance_wei: u128) {
        gauge!("orchestrator_spoke_pool_balance_wei").set(balance_wei as f64);
    }

    /// Set the current in-flight deposit total.
    pub fn set_inflight_deposits(&self, amount_wei: u128) {
        gauge!("orchestrator_inflight_deposits_wei").set(amount_wei as f64);
    }

    /// Set the count of pending withdrawals by status.
    pub fn set_pending_withdrawals(&self, status: &str, count: usize) {
        gauge!("orchestrator_pending_withdrawals", "status" => status.to_string()).set(count as f64);
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