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
	@echo "Deployment Commands:"
	@echo "  make deploy                Deploy using PRIVATE_KEY from .env.secrets"
	@echo "  make deploy-ledger         Deploy using Ledger hardware wallet (most secure)"
	@echo "  make deploy-keystore       Deploy using encrypted keystore file"
	@echo "  make deploy-dry-run        Simulate deployment without broadcasting"
	@echo ""
	@echo "  Add SKIP_VERIFY=1 to skip contract verification:"
	@echo "    make deploy-ledger SKIP_VERIFY=1"
	@echo ""
	@echo "Code Quality Commands:"
	@echo "  make fmt                   Format code with forge fmt"
	@echo "  make lint                  Run forge lint"
	@echo "  make pr                    Run all pre-PR checks (fmt, lint, build, test)"
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
# Code Quality
##
.PHONY: fmt
fmt:
	forge fmt

.PHONY: lint
lint:
	forge lint

.PHONY: pr
pr: clean fmt lint build test
	@echo ""
	@echo "✅ All pre-PR checks passed!"
	@echo "   - Code formatted"
	@echo "   - Linting passed"
	@echo "   - Build successful"
	@echo "   - All tests passed"
	@echo ""

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

##
# Deployment
##
-include .env.secrets
export

.PHONY: deploy
deploy:
	@if [ ! -f .env.secrets ]; then \
		echo "❌ .env.secrets file not found. Copy .env.secrets.example to .env.secrets and configure it."; \
		exit 1; \
	fi
	@echo "🚀 Deploying WithdrawalLiquidityPool..."
	@echo "⚠️  Note: This will use PRIVATE_KEY from .env.secrets or you can add --ledger/--trezor flag"
	forge script script/DeployWithdrawalLiquidityPool.s.sol:DeployWithdrawalLiquidityPool \
		--rpc-url $(RPC_URL) \
		--broadcast \
		$(if $(SKIP_VERIFY),,--verify)

.PHONY: deploy-ledger
deploy-ledger:
	@if [ ! -f .env.secrets ]; then \
		echo "❌ .env.secrets file not found. Copy .env.secrets.example to .env.secrets and configure it."; \
		exit 1; \
	fi
	@echo "🚀 Deploying WithdrawalLiquidityPool with Ledger..."
	@echo "📱 Please confirm transaction on your Ledger device"
	forge script script/DeployWithdrawalLiquidityPool.s.sol:DeployWithdrawalLiquidityPool \
		--rpc-url $(RPC_URL) \
		--ledger \
		--broadcast \
		$(if $(SKIP_VERIFY),,--verify)

.PHONY: deploy-keystore
deploy-keystore:
	@if [ ! -f .env.secrets ]; then \
		echo "❌ .env.secrets file not found. Copy .env.secrets.example to .env.secrets and configure it."; \
		exit 1; \
	fi
	@if [ -z "$(KEYSTORE_PATH)" ]; then \
		echo "❌ KEYSTORE_PATH not set. Set it in .env.secrets or pass as: make deploy-keystore KEYSTORE_PATH=path/to/keystore"; \
		exit 1; \
	fi
	@echo "🚀 Deploying WithdrawalLiquidityPool with keystore..."
	@echo "🔐 You will be prompted for keystore password"
	forge script script/DeployWithdrawalLiquidityPool.s.sol:DeployWithdrawalLiquidityPool \
		--rpc-url $(RPC_URL) \
		--keystore $(KEYSTORE_PATH) \
		--broadcast \
		$(if $(SKIP_VERIFY),,--verify)

.PHONY: deploy-dry-run
deploy-dry-run:
	@if [ ! -f .env.secrets ]; then \
		echo "❌ .env.secrets file not found. Copy .env.secrets.example to .env.secrets and configure it."; \
		exit 1; \
	fi
	@echo "🔍 Simulating deployment (dry run)..."
	forge script script/DeployWithdrawalLiquidityPool.s.sol:DeployWithdrawalLiquidityPool \
		--rpc-url $(RPC_URL)
