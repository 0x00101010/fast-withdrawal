// SPDX-License-Identifier: MIT
pragma solidity 0.8.15;

import {Base} from "./Base.s.sol";
import {console} from "forge-std/console.sol";
import {WithdrawalLiquidityPool} from "../../contracts/WithdrawalLiquidityPool.sol";
import {ProxyAdmin} from "@eth-optimism-bedrock/src/universal/ProxyAdmin.sol";
import {Proxy} from "@eth-optimism-bedrock/src/universal/Proxy.sol";

/**
 * @title Setup
 * @notice Deploys WithdrawalLiquidityPool on Anvil fork using deterministic CREATE2
 * @dev This script:
 *      1. Uses existing Sepolia ProxyAdmin (not deploying new one)
 *      2. Deploys Implementation and Proxy with CREATE2 for deterministic addresses
 *      3. Initializes the pool with test configuration
 *      4. Funds all test accounts with ETH
 *      5. Logs deployment summary with addresses
 *
 * Usage:
 *   # Terminal 1: Start Anvil fork
 *   anvil --fork-url $SEPOLIA_L1_URL
 *
 *   # Terminal 2: Run setup
 *   source .env.test
 *   source .env.test.secrets
 *   forge script script/test/1_Setup.s.sol:Setup \
 *     --rpc-url $ANVIL_RPC_URL \
 *     --broadcast
 *
 * After running:
 *   - Copy the deployed addresses from output
 *   - Update .env.test with:
 *     POOL_PROXY_ADDRESS=<address>
 *     POOL_IMPLEMENTATION_ADDRESS=<address>
 *     POOL_PROXY_ADMIN_ADDRESS=<address>
 */
contract Setup is Base {
    function run() external {
        // Load configuration
        loadConfig();

        console.log("\n=== Setup: Deploy Pool on Anvil Fork ===\n");
        console.log("Deploying to:", anvilRpcUrl);
        console.log("OptimismPortal:", sepoliaOptimismPortal);
        console.log("Existing ProxyAdmin:", sepoliaProxyAdmin);
        console.log("Initial Fee Rate:", testFeeRate, "bps");

        // Use Anvil's first default account as deployer/owner
        address deployer = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;
        console.log("Deployer/Owner:", deployer);

        // Use existing ProxyAdmin
        ProxyAdmin proxyAdmin = ProxyAdmin(sepoliaProxyAdmin);
        console.log("\n[1/6] Using existing ProxyAdmin");
        console.log("ProxyAdmin address:", address(proxyAdmin));
        console.log("ProxyAdmin owner:", proxyAdmin.owner());

        // Deterministic salt for CREATE2
        bytes32 salt = bytes32(uint256(1));
        console.log("\n[2/6] Using CREATE2 salt:", vm.toString(salt));

        // Start broadcasting transactions
        vm.startBroadcast(deployer);
        console.log("Broadcasting from:", deployer);

        // Step 2: Deploy implementation with CREATE2
        console.log("\n[3/6] Deploying WithdrawalLiquidityPool implementation with CREATE2...");
        WithdrawalLiquidityPool implementation = new WithdrawalLiquidityPool{salt: salt}();
        console.log("Implementation deployed at:", address(implementation));

        // Step 3: Deploy proxy with CREATE2
        console.log("\n[4/6] Deploying Proxy with CREATE2...");
        Proxy proxy = new Proxy{salt: salt}(deployer);
        console.log("Proxy deployed at:", address(proxy));

        // Step 4: Encode initialization call
        console.log("\n[5/6] Initializing proxy...");
        bytes memory initData =
            abi.encodeCall(WithdrawalLiquidityPool.initialize, (deployer, sepoliaOptimismPortal, testFeeRate));

        // Set implementation and initialize
        proxy.upgradeToAndCall(address(implementation), initData);
        console.log("Proxy initialized");

        // Change proxy admin to ProxyAdmin
        proxy.changeAdmin(address(proxyAdmin));
        console.log("ProxyAdmin set as proxy admin");

        vm.stopBroadcast();

        // Step 5: Fund test accounts (must be done outside broadcast)
        console.log("\n[6/6] Funding test accounts...");
        console.log("Addresses to fund:");
        console.log("  testOwner:", testOwner);
        console.log("  testLp1:", testLp1);
        console.log("  testLp2:", testLp2);
        console.log("  testUser1:", testUser1);
        console.log("  testUser2:", testUser2);

        console.log("\nBalances before funding:");
        console.log("  testOwner:", testOwner.balance / 1 ether, "ETH");
        console.log("  testLp1:", testLp1.balance / 1 ether, "ETH");
        console.log("  testLp2:", testLp2.balance / 1 ether, "ETH");
        console.log("  testUser1:", testUser1.balance / 1 ether, "ETH");
        console.log("  testUser2:", testUser2.balance / 1 ether, "ETH");

        console.log("\nFunding accounts...");
        vm.deal(testOwner, 100 ether);
        console.log("  Funded testOwner");
        vm.deal(testLp1, 100 ether);
        console.log("  Funded testLp1");
        vm.deal(testLp2, 100 ether);
        console.log("  Funded testLp2");
        vm.deal(testUser1, 10 ether);
        console.log("  Funded testUser1");
        vm.deal(testUser2, 10 ether);
        console.log("  Funded testUser2");

        console.log("\nBalances after funding:");
        console.log("  testOwner:", testOwner.balance / 1 ether, "ETH");
        console.log("  testLp1:", testLp1.balance / 1 ether, "ETH");
        console.log("  testLp2:", testLp2.balance / 1 ether, "ETH");
        console.log("  testUser1:", testUser1.balance / 1 ether, "ETH");
        console.log("  testUser2:", testUser2.balance / 1 ether, "ETH");

        // Wrap proxy in ABI for easier interaction
        WithdrawalLiquidityPool pool = WithdrawalLiquidityPool(payable(address(proxy)));

        // Step 6: Transfer pool ownership to testOwner (if different from deployer)
        // Note: Not transferring ProxyAdmin ownership since we're using existing ProxyAdmin
        if (testOwner != deployer) {
            console.log("\n[Note] Transferring pool ownership only...");
            console.log("Transferring pool ownership to:", testOwner);

            vm.startBroadcast(deployer);

            pool.transferOwnership(testOwner);
            console.log("Pool ownership transferred");
            console.log("Note: ProxyAdmin ownership NOT transferred (using existing ProxyAdmin)");

            vm.stopBroadcast();
        }

        // Log deployment summary
        console.log("\n=== Deployment Summary ===");
        console.log("ProxyAdmin:          ", address(proxyAdmin));
        console.log("ProxyAdmin Owner:    ", proxyAdmin.owner());
        console.log("Implementation:      ", address(implementation));
        console.log("Proxy:               ", address(proxy));
        console.log("Pool Contract:       ", address(pool));
        console.log("\nPool Configuration:");
        console.log("Owner:               ", pool.owner());
        console.log("OptimismPortal:      ", address(pool.optimismPortal()));
        console.log("Fee Rate (bps):      ", pool.feeRate());
        console.log("Start Block:         ", pool.startBlock());
        console.log("Total Liquidity:     ", pool.totalLiquidity());

        console.log("\n=== Test Account Balances ===");
        console.log("Owner:               ", testOwner.balance / 1 ether, "ETH");
        console.log("LP1:                 ", testLp1.balance / 1 ether, "ETH");
        console.log("LP2:                 ", testLp2.balance / 1 ether, "ETH");
        console.log("User1:               ", testUser1.balance / 1 ether, "ETH");
        console.log("User2:               ", testUser2.balance / 1 ether, "ETH");

        console.log("\n=== Verification ===");
        console.log("Running automatic verification checks...\n");

        // Verify pool ownership
        require(pool.owner() == testOwner, "VERIFICATION FAILED: Pool owner incorrect");
        console.log("[PASS] Pool owner is testOwner");

        // Note: Skipping ProxyAdmin ownership check (using existing Sepolia ProxyAdmin)
        console.log("[SKIP] ProxyAdmin ownership check (using existing ProxyAdmin)");

        // Verify pool configuration
        require(
            address(pool.optimismPortal()) == sepoliaOptimismPortal,
            "VERIFICATION FAILED: OptimismPortal address incorrect"
        );
        console.log("[PASS] OptimismPortal address correct");

        require(pool.feeRate() == testFeeRate, "VERIFICATION FAILED: Fee rate incorrect");
        console.log("[PASS] Fee rate correct");

        // Verify test account balances
        require(testOwner.balance >= 90 ether, "VERIFICATION FAILED: testOwner balance too low");
        console.log("[PASS] testOwner balance sufficient");

        require(testLp1.balance == 100 ether, "VERIFICATION FAILED: testLp1 balance incorrect");
        console.log("[PASS] testLp1 balance correct");

        require(testLp2.balance == 100 ether, "VERIFICATION FAILED: testLp2 balance incorrect");
        console.log("[PASS] testLp2 balance correct");

        console.log("\n[PASS] All verification checks passed!");

        console.log("\n=== Updating .env.test ===");

        // Prepare new address values
        string memory proxyAddr = vm.toString(address(proxy));
        string memory implAddr = vm.toString(address(implementation));
        string memory adminAddr = vm.toString(address(proxyAdmin));

        // Replace or append deployed addresses
        // Note: vm.replace() doesn't exist, so we'll use a simpler approach
        // Write addresses to a separate deployment file that can be sourced
        string memory deployedAddresses = string.concat(
            "# Deployed Contract Addresses (auto-updated by 1_Setup.s.sol)\n",
            string.concat("POOL_PROXY_ADDRESS=", proxyAddr, "\n"),
            string.concat("POOL_IMPLEMENTATION_ADDRESS=", implAddr, "\n"),
            string.concat("POOL_PROXY_ADMIN_ADDRESS=", adminAddr, "\n")
        );

        // Write to .env.test.deployed for manual merge
        // forge-lint: disable-next-line(unsafe-cheatcode)
        vm.writeFile(".env.test.deployed", deployedAddresses);
        console.log("[OK] Deployed addresses written to .env.test.deployed");
        console.log("");
        console.log("Deployed addresses:");
        console.log("  POOL_PROXY_ADDRESS=", proxyAddr);
        console.log("  POOL_IMPLEMENTATION_ADDRESS=", implAddr);
        console.log("  POOL_PROXY_ADMIN_ADDRESS=", adminAddr);

        console.log("\n=== Next Steps ===");
        console.log("Addresses saved to .env.test.deployed");
        console.log("Run: make test-update-env (to merge into .env.test)");
        console.log("Then run next script:");
        console.log("     forge script script/test/2_InitiateWithdrawal.s.sol");
    }
}
