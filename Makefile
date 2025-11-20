.PHONY: help
help:
	@echo "Fast Withdrawal Protocol - Makefile Commands"
	@echo ""
	@echo "Setup Commands:"
	@echo "  make deps                  Install all dependencies (OpenZeppelin, Optimism contracts)"
	@echo "  make clean                 Clean lib folder and build artifacts"
	@echo ""
	@echo "Testing Commands:"
	@echo "  make test                  Run all tests"
	@echo "  make test-unit             Run unit tests only"
	@echo "  make test-integration      Run integration tests"
	@echo "  make test-invariant        Run invariant/fuzz tests"
	@echo "  make test-coverage         Run tests with coverage report"
	@echo ""
	@echo "Build Commands:"
	@echo "  make build                 Compile contracts"
	@echo "  make clean-build           Clean and rebuild"
	@echo ""
	@echo "Analysis Commands:"
	@echo "  make slither               Run Slither static analysis"
	@echo "  make mythril               Run Mythril security analysis"
	@echo ""

##
# Dependency Management
##
include .env
export

.PHONY: deps
deps: clean-lib forge-deps checkout-op-commit

.PHONY: clean-lib
clean-lib:
	rm -rf lib/optimism

.PHONY: forge-deps
forge-deps:
	@echo "OpenZeppelin contracts already installed"
	forge install

.PHONY: checkout-op-commit
checkout-op-commit:
	@if [ -z "$(OP_COMMIT)" ]; then \
		echo "OP_COMMIT must be set in .env"; \
		echo "Run: cp .env.example .env"; \
		exit 1; \
	fi
	@echo "Installing Optimism contracts at commit $(OP_COMMIT)..."
	rm -rf lib/optimism
	mkdir -p lib/optimism
	cd lib/optimism; \
	git init; \
	git remote add origin https://github.com/ethereum-optimism/optimism.git; \
	git fetch --depth=1 origin $(OP_COMMIT); \
	git reset --hard FETCH_HEAD

##
# Testing
##
.PHONY: test
test:
	forge test -vvv

.PHONY: test-unit
test-unit:
	forge test --match-path "test/*.t.sol" -vvv

.PHONY: test-integration
test-integration:
	forge test --match-path "test/integration/*.t.sol" -vvv

.PHONY: test-invariant
test-invariant:
	forge test --match-path "test/invariant/*.t.sol" -vvv

.PHONY: test-coverage
test-coverage:
	forge coverage --report summary

##
# Build
##
.PHONY: build
build:
	forge build

.PHONY: clean
clean:
	forge clean
	rm -rf cache out

.PHONY: clean-build
clean-build: clean build

##
# Static Analysis
##
.PHONY: slither
slither:
	@which slither > /dev/null || (echo "Slither not installed. Install with: pip install slither-analyzer" && exit 1)
	slither .

.PHONY: mythril
mythril:
	@which myth > /dev/null || (echo "Mythril not installed. Install with: pip install mythril" && exit 1)
	myth analyze src/WithdrawalLiquidityPool.sol
