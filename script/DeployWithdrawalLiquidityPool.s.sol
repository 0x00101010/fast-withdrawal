// SPDX-License-Identifier: MIT
pragma solidity 0.8.15;

import {Script} from "forge-std/Script.sol";
import {console} from "forge-std/console.sol";
import {WithdrawalLiquidityPool} from "../contracts/WithdrawalLiquidityPool.sol";
import {Proxy} from "src/universal/Proxy.sol";
import {ProxyAdmin} from "src/universal/ProxyAdmin.sol";

/**
 * @title DeployWithdrawalLiquidityPool
 * @notice Deployment script for WithdrawalLiquidityPool using Optimism's proxy pattern
 * @dev This script deploys:
 *      1. ProxyAdmin (if not provided)
 *      2. WithdrawalLiquidityPool implementation
 *      3. Proxy pointing to the implementation
 *      4. Initializes the proxy with provided parameters
 *
 * Usage:
 *   forge script script/DeployWithdrawalLiquidityPool.s.sol:DeployWithdrawalLiquidityPool \
 *     --rpc-url $RPC_URL \
 *     --broadcast \
 *     --verify
 *
 * Environment variables:
 *   OWNER_ADDRESS         - Address that will own the pool contract
 *   OPTIMISM_PORTAL       - Address of the OptimismPortal2 contract
 *   INITIAL_FEE_RATE      - Initial fee rate in basis points (optional, defaults to 50 = 0.5%)
 *   PROXY_ADMIN_ADDRESS   - Existing ProxyAdmin address (optional, deploys new if not provided)
 *   PROXY_ADMIN_OWNER     - Address that will own the ProxyAdmin (optional, defaults to OWNER_ADDRESS)
 */
contract DeployWithdrawalLiquidityPool is Script {
    function run() external {
        // Read environment variables
        address owner = vm.envAddress("OWNER_ADDRESS");
        address payable optimismPortal = payable(vm.envAddress("OPTIMISM_PORTAL"));
        uint256 initialFeeRate = vm.envOr("INITIAL_FEE_RATE", uint256(50)); // Default 0.5%

        // Optional: use existing ProxyAdmin or deploy new one
        address proxyAdminAddress = vm.envOr("PROXY_ADMIN_ADDRESS", address(0));
        address proxyAdminOwner = vm.envOr("PROXY_ADMIN_OWNER", owner);

        console.log("Deploying WithdrawalLiquidityPool...");
        console.log("Owner:", owner);
        console.log("OptimismPortal:", optimismPortal);
        console.log("Initial Fee Rate:", initialFeeRate, "basis points");

        vm.startBroadcast();

        // Get the deployer address (whoever is broadcasting)
        address deployer = msg.sender;

        // Step 1: Deploy or use existing ProxyAdmin
        ProxyAdmin proxyAdmin;
        if (proxyAdminAddress == address(0)) {
            console.log("Deploying new ProxyAdmin...");
            // Deploy with deployer as temporary owner so we can initialize
            proxyAdmin = new ProxyAdmin(deployer);
            console.log("ProxyAdmin deployed at:", address(proxyAdmin));
            console.log("ProxyAdmin owner (temporary):", deployer);
        } else {
            console.log("Using existing ProxyAdmin at:", proxyAdminAddress);
            proxyAdmin = ProxyAdmin(proxyAdminAddress);
        }

        // Step 2: Deploy implementation
        console.log("Deploying WithdrawalLiquidityPool implementation...");
        WithdrawalLiquidityPool implementation = new WithdrawalLiquidityPool();
        console.log("Implementation deployed at:", address(implementation));

        // Step 3: Deploy proxy with ProxyAdmin as admin
        console.log("Deploying Proxy...");
        Proxy proxy = new Proxy(address(proxyAdmin));
        console.log("Proxy deployed at:", address(proxy));

        // Step 4: Encode initialization call
        bytes memory initData =
            abi.encodeCall(WithdrawalLiquidityPool.initialize, (owner, optimismPortal, initialFeeRate));

        // Step 5: Set implementation and initialize through ProxyAdmin
        console.log("Initializing proxy...");
        proxyAdmin.upgradeAndCall(payable(address(proxy)), address(implementation), initData);

        // Step 6: Transfer ProxyAdmin ownership if needed
        if (proxyAdminAddress == address(0) && proxyAdminOwner != deployer) {
            console.log("Transferring ProxyAdmin ownership to:", proxyAdminOwner);
            proxyAdmin.transferOwnership(proxyAdminOwner);
        }

        vm.stopBroadcast();

        // Wrap proxy in ABI for easier interaction
        WithdrawalLiquidityPool pool = WithdrawalLiquidityPool(payable(address(proxy)));

        console.log("\n=== Deployment Summary ===");
        console.log("ProxyAdmin:      ", address(proxyAdmin));
        console.log("ProxyAdmin Owner:", proxyAdmin.owner());
        console.log("Implementation:  ", address(implementation));
        console.log("Proxy:           ", address(proxy));
        console.log("Pool Contract:   ", address(pool));
        console.log("\nPool Configuration:");
        console.log("Owner:           ", pool.owner());
        console.log("OptimismPortal:  ", address(pool.optimismPortal()));
        console.log("Fee Rate:        ", pool.feeRate(), "bps");
        console.log("Start Block:     ", pool.startBlock());
    }
}
