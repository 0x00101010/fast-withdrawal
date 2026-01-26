# fast-withdrawal justfile
# Common development tasks

# Set default shell options
set positional-arguments

# Nightly version (override with NIGHTLY env var)
nightly := env_var_or_default('NIGHTLY', 'nightly')

# Aliases for common commands
alias f := fix
alias l := lint
alias b := build
alias t := test
alias pr := pre-pr

# Show available recipes
default:
    @just --list

# Build the workspace
build:
    cargo build --workspace --all-targets

# Build with release optimizations
build-release:
    cargo build --workspace --all-targets --release

# Run the orchestrator
run *args='':
    cargo run --bin orchestrator {{args}}

# Run the linter and formatter checks
lint: check-fmt clippy check-docs

# Run all tests
test *args='':
    cargo nextest run --workspace {{args}}

# Run deposit integration test (requires funds)
run-deposit:
    cargo nextest run --package orchestrator --test deposit --run-ignored ignored-only test_deposit_action_execute

# Run withdrawal initiation test (requires funds)
run-withdraw:
    cargo nextest run --package orchestrator --test withdraw --run-ignored ignored-only test_withdraw_action_execute

# Run withdrawal prove test (requires funds and initiated withdrawal)
run-prove:
    cargo nextest run --package orchestrator --test prove --run-ignored ignored-only test_prove_action_execute

# Run withdrawal finalize test (requires funds and proven withdrawal after 7 days)
run-finalize:
    cargo nextest run --package orchestrator --test finalize --run-ignored ignored-only test_finalize_action_execute

# Run step: process pending withdrawals (prove + finalize)
step-process-withdrawals:
    cargo run --bin step -- -k "$PRIVATE_KEY" --config ./config.test.toml process-withdrawals

# Run step: initiate L2→L1 withdrawal if threshold met
step-initiate-withdrawal:
    cargo run --bin step -- -k "$PRIVATE_KEY" --config ./config.test.toml initiate-withdrawal

# Run step: deposit from L1 to L2 if needed
step-deposit:
    cargo run --bin step -- -k "$PRIVATE_KEY" --config ./config.test.toml deposit

check-inflight-deposits:
    cargo nextest run --package orchestrator --test inflight --run-ignored ignored-only test_long_lookback_scan_slow

# Fix all auto-fixable issues
fix: fix-fmt clippy-fix

# Check code formatting
check-fmt:
    cargo +{{nightly}} fmt --all --check

# Fix code formatting
fix-fmt:
    cargo +{{nightly}} fmt --all

# Run clippy with strict settings
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Apply clippy fixes
clippy-fix:
    cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged

# Check documentation
check-docs:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items

# Test documentation examples
test-docs:
    cargo test --workspace --doc

# Run benchmarks for a specific crate
bench crate:
    cargo bench -p {{crate}}

# Check for unused dependencies
udeps:
    cargo +{{nightly}} udeps --workspace

# Run pre-PR checks
pre-pr: lint test

# Clean build artifacts
clean:
    cargo clean

# Update dependencies
update:
    cargo update

# Run audit for security vulnerabilities
audit:
    cargo audit

# Install development tools
install-tools:
    cargo install cargo-nextest
    cargo install cargo-udeps --locked
    cargo install cargo-audit