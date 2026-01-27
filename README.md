# Fast-Withdrawal

A Rust orchestrator service that manages liquidity between Ethereum L1 and Unichain L2 by automating cross-chain deposits and withdrawals through the Across Protocol and OP Stack native bridge.

## Overview

The orchestrator monitors SpokePool balances on Unichain and automatically:

1. **Deposits (L1→L2)**: When the SpokePool WETH balance exceeds a target threshold, initiates deposits from Ethereum to replenish liquidity via Across Protocol
2. **Withdrawals (L2→L1)**: When the L2 EOA balance exceeds a threshold, initiates native OP Stack withdrawals to move funds back to L1
3. **Proves withdrawals**: Submits proofs for initiated withdrawals once they're eligible
4. **Finalizes withdrawals**: Completes proven withdrawals after the challenge period (7 days)

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Orchestrator                                 │
│                                                                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐               │
│  │   Balance    │  │  Withdrawal  │  │   Deposit    │               │
│  │   Monitor    │  │   Actions    │  │   Actions    │               │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘               │
│         │                 │                 │                        │
│         ▼                 ▼                 ▼                        │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                    Action Trait                              │    │
│  │  is_ready() → is_completed() → execute()                    │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                              │                                       │
└──────────────────────────────┼───────────────────────────────────────┘
                               │
           ┌───────────────────┴───────────────────┐
           ▼                                       ▼
    ┌─────────────┐                         ┌─────────────┐
    │  Ethereum   │                         │  Unichain   │
    │    (L1)     │                         │    (L2)     │
    │             │                         │             │
    │ • SpokePool │◄──── Across Deposit ────│ • SpokePool │
    │ • Portal    │                         │ • L2ToL1    │
    │ • Factory   │──── OP Withdrawal ──────│   Passer    │
    └─────────────┘                         └─────────────┘
```

### Crate Structure

```
fast-withdrawal/
├── bin/
│   └── orchestrator/     # Main service binary
│       └── src/
│           ├── main.rs   # Entry point with main loop
│           ├── config.rs # Configuration management
│           └── metrics.rs# Prometheus metrics
├── crates/
│   ├── action/           # Executable onchain actions (withdraw, prove, finalize, deposit, claim)
│   ├── balance/          # Balance monitoring utilities
│   ├── binding/          # Contract bindings (Across, OP Stack)
│   ├── client/           # RPC client creation
│   ├── config/           # Network configurations (mainnet/testnet addresses)
│   ├── deposit/          # Deposit state tracking
│   └── withdrawal/       # Withdrawal types, hashing, state management
```

## Supported Networks

| Network | L1 | L2 | Status |
|---------|----|----|--------|
| Mainnet | Ethereum (1) | Unichain (130) | ✅ |
| Testnet | Sepolia (11155111) | Unichain Sepolia (1301) | ✅ |

## Configuration

Create a `config.toml` file:

```toml
# RPC endpoints
l1_rpc_url = "https://eth-mainnet.example.com"
l2_rpc_url = "https://unichain-mainnet.example.com"

# Network: "Mainnet" or "Testnet"
network = "Mainnet"

# Your EOA address (must be funded on both chains)
eoa_address = "0x..."

# Deposit triggers: when SpokePool balance exceeds target, deposit down to floor
spoke_pool_target_wei = "75000000000000000000"  # 75 ETH
spoke_pool_floor_wei = "20000000000000000000"   # 20 ETH

# Withdrawal triggers: when L2 EOA balance exceeds threshold, initiate withdrawal
withdrawal_threshold_wei = "75000000000000000000"  # 75 ETH
gas_buffer_wei = "10000000000000000"               # 0.01 ETH (keep for gas)

# Lookback windows
deposit_lookback_secs = 43200      # 12 hours (to track in-flight deposits)
withdrawal_lookback_secs = 1209600 # 2 weeks (to find pending withdrawals)

# Main loop interval
cycle_interval_secs = 30

# Dry-run mode (log actions without executing)
dry_run = false

# Prometheus metrics port
metrics_port = 9090
```

### Environment Variables

The orchestrator requires a funded wallet. Set your private key:

```bash
export PRIVATE_KEY=0x...
```

## Running

### Prerequisites

- Rust 1.92.0+
- Funded EOA on both Ethereum and Unichain

### Build

```bash
# Build all targets
cargo build --workspace --all-targets

# Or use justfile
just build
```

### Run

```bash
# Run with default config.toml
cargo run --bin orchestrator

# Run with custom config
cargo run --bin orchestrator -- --config path/to/config.toml

# Run in dry-run mode (no transactions)
cargo run --bin orchestrator -- --dry-run

# Or use justfile
just run
just run -- --dry-run
```

### Step Commands (Manual Operations)

For testing individual operations:

```bash
# Process pending withdrawals (prove + finalize)
just step-process-withdrawals

# Initiate L2→L1 withdrawal if threshold met
just step-initiate-withdrawal

# Deposit from L1 to L2 if needed
just step-deposit
```

## Metrics

The orchestrator exposes Prometheus metrics on the configured port (default 9090):

### Cycle Metrics
- `orchestrator_cycles_total` - Total cycles executed
- `orchestrator_cycles_success_total` - Successful cycles
- `orchestrator_cycles_failure_total` - Failed cycles
- `orchestrator_cycle_duration_seconds` - Cycle duration histogram

### Balance Gauges
- `orchestrator_l1_eoa_balance_eth` - L1 EOA balance
- `orchestrator_l2_eoa_balance_eth` - L2 EOA balance
- `orchestrator_spoke_pool_balance_eth` - SpokePool WETH balance

### In-Flight Tracking
- `orchestrator_inflight_deposits_count` - Pending deposits count
- `orchestrator_inflight_deposits_eth` - Pending deposits amount
- `orchestrator_inflight_withdrawals_count` - Total pending withdrawals
- `orchestrator_inflight_withdrawals_eth` - Total pending withdrawal amount
- `orchestrator_withdrawals_initiated_count` - Withdrawals awaiting proof
- `orchestrator_withdrawals_proven_count` - Withdrawals awaiting finalization

## Development

### Testing

```bash
# Run all tests
just test

# Run tests including slow/integration
just test --profile all

# Run specific integration tests (require testnet funds)
just run-deposit
just run-withdraw
just run-prove
just run-finalize
```

### Linting

```bash
# Check formatting, clippy, and docs
just lint

# Auto-fix issues
just fix

# Pre-PR checks (lint + test)
just pre-pr
```

### Install Dev Tools

```bash
just install-tools
```

This installs:
- `cargo-nextest` - Fast test runner
- `cargo-udeps` - Unused dependency checker
- `cargo-audit` - Security vulnerability scanner

## How It Works

### Main Loop

Each cycle (default 30s):

1. **Process Pending Withdrawals**
   - Scan for withdrawals initiated in the lookback window
   - For proven withdrawals: check if mature, then finalize
   - For initiated withdrawals: submit proof

2. **Maybe Initiate Withdrawal**
   - Check L2 EOA balance
   - If above threshold, initiate L2→L1 withdrawal (keeping gas buffer)

3. **Maybe Deposit**
   - Check SpokePool WETH balance
   - Subtract in-flight deposits for projected balance
   - If projected > target, deposit (projected - floor) to L2

### Withdrawal Lifecycle (OP Stack)

```
Initiate (L2)  →  Prove (L1)  →  Wait 7 days  →  Finalize (L1)
     │                │                              │
     └── L2ToL1MessagePasser.initiateWithdrawal     │
                      │                              │
                      └── OptimismPortal2.proveWithdrawalTransaction
                                                     │
                                                     └── OptimismPortal2.finalizeWithdrawalTransaction
```

### Deposit Flow (Across Protocol)

```
Deposit (L1 SpokePool)  →  Across Slow fills (L2)
        │                        │
        └── depositV3()          └── settlement system fills on L2
```

## License

MIT OR Apache-2.0