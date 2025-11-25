# Deployment Guide

## Environment Configuration

This project uses two environment files to separate public configuration from secrets:

- `.env.secrets` - Private keys, RPC URLs, API keys (gitignored, never commit!)

### Setup

1. Copy example files:
```bash
cp .env.secrets.example .env.secrets
```

2. Edit `.env.secrets` with your configuration:
   - `OWNER_ADDRESS` - Address that will own the pool contract
   - `OPTIMISM_PORTAL` - Address of the OptimismPortal2 contract
   - `INITIAL_FEE_RATE` - Initial fee rate in basis points (default: 50 = 0.5%)
   - `RPC_URL` - Your RPC endpoint with API key
   - `ETHERSCAN_API_KEY` - For contract verification
   - Optional: `PRIVATE_KEY` - Deployer private key (if using keystore, you can omit this)
   - Optional: `PROXY_ADMIN_ADDRESS` - Existing ProxyAdmin (leave unset to deploy new)
   - Optional: `PROXY_ADMIN_OWNER` - Who should own the ProxyAdmin (default: OWNER_ADDRESS)

**Note on ProxyAdmin Ownership:**
- The script deploys ProxyAdmin with the **deployer** as temporary owner (to initialize the proxy)
- After initialization, ownership is transferred to `PROXY_ADMIN_OWNER` (or `OWNER_ADDRESS` if not set)
- This allows the deployer to set everything up, then hand over control

## Deployment Methods

### Method 1: Using Makefile (Recommended)

The Makefile automatically loads both `.env` and `.env.secrets`:

```bash
# Deploy using private key from .env.secrets
make deploy

# Deploy using Ledger hardware wallet (most secure for mainnet)
make deploy-ledger

# Deploy using encrypted keystore file
make deploy-keystore KEYSTORE_PATH=~/.foundry/keystores/deployer

# Dry run (simulate without broadcasting)
make deploy-dry-run
```

**Security Recommendations:**
- **Testnet**: `make deploy` (private key is fine)
- **Mainnet**: `make deploy-ledger` (hardware wallet highly recommended)

### Method 2: Using forge script directly

Load both files explicitly:
```bash
forge script script/DeployWithdrawalLiquidityPool.s.sol:DeployWithdrawalLiquidityPool \
  --env-file .env \
  --env-file .env.secrets \
  --rpc-url $RPC_URL \
  --broadcast \
  --verify
```

### Method 3: Using only secrets file

If all your config is in `.env.secrets`:
```bash
forge script script/DeployWithdrawalLiquidityPool.s.sol:DeployWithdrawalLiquidityPool \
  --env-file .env.secrets \
  --rpc-url $RPC_URL \
  --broadcast \
  --verify
```

## Deployment Output

The script will output:
- ProxyAdmin address
- Implementation address
- Proxy address
- Pool contract address (proxy)

Save these addresses for future reference.

## Verification

After deployment, the script automatically verifies:
- Owner is set correctly
- OptimismPortal address is correct
- Fee rate is set correctly
- Start block is recorded

## Upgrading

To upgrade the implementation:

1. Deploy new implementation:
```bash
forge create contracts/WithdrawalLiquidityPool.sol:WithdrawalLiquidityPool \
  --rpc-url $RPC_URL \
  --verify
```

2. Upgrade through ProxyAdmin (requires ProxyAdmin owner):
```bash
cast send $PROXY_ADMIN_ADDRESS \
  "upgrade(address,address)" \
  $PROXY_ADDRESS \
  $NEW_IMPLEMENTATION_ADDRESS \
  --rpc-url $RPC_URL
```

## Security Notes

- **Never commit `.env.secrets`** - it's gitignored by default
- Use hardware wallets or keystore files for mainnet deployments
- Test on testnet first
- Verify all addresses before mainnet deployment
