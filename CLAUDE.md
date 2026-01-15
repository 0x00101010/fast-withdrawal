# CLAUDE.md

## Overview

This is the `fast-withdrawal` monorepo, a collection of Rust services designed to facilitate fast withdrawal operations for L2 bridge systems. The orchestrator monitors spokepool balances across L2s and triggers withdrawals when needed.

## Repository Structure

The workspace is organized as follows:

- **bin/**: Binary crates (services/executables)
  - `orchestrator`: Monitors L2 spokepool balances and triggers withdrawals
- **crates/**: Shared libraries (to be added as needed)
- **plans/**: Task planning documents (gitignored, for Claude Code planning)

## Essential Commands

### Building
```bash
# Build entire workspace
cargo build --workspace --all-targets

# Build specific binary
cargo build --bin orchestrator

# Build with release optimizations
cargo build --release
```

Or use justfile:
```bash
just build           # Build workspace
just build-release   # Release build
```

### Testing
```bash
# Test entire workspace
cargo nextest run --workspace

# Test specific crate
cargo nextest run -p <crate-name>

# Run slow tests
cargo nextest run --profile slow

# Run all tests including slow/integration
cargo nextest run --profile all
```

Or use justfile:
```bash
just test            # Run tests (excludes slow/integration)
just test --profile all
```

### Linting and Formatting
```bash
# Format code
cargo +nightly fmt --all

# Run clippy
cargo clippy --workspace --all-targets -- -D warnings

# Check everything
just lint            # Runs check-fmt + clippy + check-docs

# Auto-fix issues
just fix             # Runs fix-fmt + clippy-fix
```

### Pre-Push Workflow
```bash
just pre-pr          # Runs lint + test
```

## Key Design Principles

1. **Simplicity**: Code should be obviously correct at a glance
2. **Type Safety**: Leverage Rust's type system for correctness
3. **Comprehensive Testing**: Maintain high test coverage

## Code Style

### Naming Conventions
- **Types**: `PascalCase` (e.g., `WithdrawalRequest`)
- **Functions**: `snake_case` (e.g., `process_withdrawal`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `MAX_WITHDRAWAL_AMOUNT`)
- **Modules**: `snake_case` (e.g., `balance_monitor`)

### Error Handling
- Use `thiserror` for library error types
- Use `anyhow::Result` for application code
- Provide descriptive error messages

### Documentation
- Document all public APIs with `///` doc comments
- Include examples where helpful
- Document panics and safety considerations
- Use `//!` for module-level documentation

### Performance
- Prefer `Bytes` over `Vec<u8>` for network data
- Use `Arc` for cheap cloning of shared state
- Minimize allocations in hot paths
- Profile before optimizing

### Safety
- Minimize `unsafe` blocks
- Always justify with `// SAFETY:` comments

## Testing Strategy

### Test Naming
Tests should be named descriptively:
- `test_<functionality>` for standard tests
- `test_<functionality>_slow` for long-running tests (excluded by default)
- `test_<functionality>_integration` for integration tests (excluded by default)

### Test Organization
- Unit tests in `#[cfg(test)]` modules within source files
- Integration tests in `tests/` directory
- Shared test utilities in `tests/common/`

## Planning Workflow

When working on complex tasks:

1. **Create a plan**: Write implementation plans in the `plans/` directory
2. **Plan format**: Use markdown with clear sections (Overview, Approach, Steps, Files to Change)
3. **Review before implementing**: Plans help clarify approach before coding
4. **Plans are gitignored**: They're working documents, not committed to the repo

Example plan structure:
```markdown
# Feature: <Name>

## Overview
Brief description of what we're building

## Approach
High-level approach and key decisions

## Steps
1. Step one
2. Step two
...

## Files to Change
- path/to/file.rs - what changes
- path/to/other.rs - what changes
```

## Pre-Commit Checklist

Before committing:
- [ ] Code is formatted: `just fix-fmt`
- [ ] No clippy warnings: `just clippy`
- [ ] Tests pass: `just test`
- [ ] Documentation builds: `cargo doc --workspace --no-deps`
- [ ] Run full pre-PR checks: `just pre-pr`

## Workspace Lints

The workspace enforces the following clippy lints:
- `missing-const-for-fn = "warn"`: Suggest const fn where possible
- `use-self = "warn"`: Use Self instead of type name
- `redundant-clone = "warn"`: Avoid unnecessary clones
- `option-if-let-else = "warn"`: Simplify option handling
- `undocumented_unsafe_blocks = "deny"`: All unsafe must be documented

## Development Tools

Install recommended tools:
```bash
just install-tools
```

This installs:
- `cargo-nextest`: Fast test runner
- `cargo-udeps`: Find unused dependencies
- `cargo-audit`: Security vulnerability scanner