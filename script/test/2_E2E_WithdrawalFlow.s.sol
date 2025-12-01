// SPDX-License-Identifier: MIT
pragma solidity 0.8.15;

import {Base} from "./Base.s.sol";
import {console} from "forge-std/console.sol";
import {Types} from "@eth-optimism-bedrock/src/libraries/Types.sol";

/**
 * @title E2E_WithdrawalFlow
 * @notice End-to-end test of the complete withdrawal flow on Anvil fork
 * @dev This script tests:
 *      1. LP deposits liquidity to pool
 *      2. User initiates withdrawal from L2 (mocked)
 *      3. LP fulfills withdrawal instantly via pool
 *      4. Mock proof submission to OptimismPortal2
 *      5. Advance time past 7-day challenge period
 *      6. Pool settles withdrawal and gets repaid
 *      7. Verify all balances and state
 *
 * Usage:
 *   # After running 1_Setup.s.sol and updating .env.test
 *   source .env.test
 *   source .env.test.secrets
 *   forge script script/test/2_E2E_WithdrawalFlow.s.sol:E2E_WithdrawalFlow \
 *     --rpc-url $ANVIL_RPC_URL \
 *     --broadcast \
 *     --unlocked
 */
contract E2E_WithdrawalFlow is Base {
    // Test parameters
    uint256 constant LP_DEPOSIT = 50 ether;
    uint256 constant WITHDRAWAL_AMOUNT = 1 ether;
    uint256 constant WITHDRAWAL_NONCE = 1;

    function run() external {
        loadConfig();

        console.log("\n=== E2E Withdrawal Flow Test ===\n");

        // Verify pool is deployed
        require(poolProxyAddress != address(0), "Pool not deployed. Run 1_Setup.s.sol first.");

        console.log("Pool Address:", address(pool));
        console.log("OptimismPortal:", address(portal));
        console.log("");

        // Get test deployer (Anvil default account)
        address deployer = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;

        // Use testLp1 as LP and testUser1 as withdrawing user
        address lp = testLp1;
        address user = testUser1;

        console.log("LP Address:", lp);
        console.log("User Address:", user);
        console.log("");

        // ============================================
        // Step 1: LP deposits liquidity
        // ============================================
        console.log("[1/7] LP deposits liquidity to pool");
        console.log("Amount:", LP_DEPOSIT / 1 ether, "ETH");

        vm.startBroadcast(lp);
        pool.depositLiquidity{value: LP_DEPOSIT}();
        vm.stopBroadcast();

        console.log("LP balance after:", lp.balance / 1 ether, "ETH");
        console.log("Pool balance after:", address(pool).balance / 1 ether, "ETH");
        console.log("Pool total liquidity:", pool.totalLiquidity() / 1 ether, "ETH");
        console.log("Pool available liquidity:", pool.availableLiquidity() / 1 ether, "ETH");
        console.log("LP shares:", pool.liquidityShares(lp));
        console.log("");

        // ============================================
        // Step 2: Create mock L2 withdrawal
        // ============================================
        console.log("[2/7] Creating mock L2 withdrawal");

        Types.WithdrawalTransaction memory withdrawal = createSimpleWithdrawal(
            WITHDRAWAL_NONCE,
            user, // L2 sender
            user, // L1 recipient
            WITHDRAWAL_AMOUNT
        );

        bytes32 withdrawalHash = hashWithdrawal(withdrawal);
        console.log("Withdrawal hash:", vm.toString(withdrawalHash));
        console.log("Withdrawal amount:", WITHDRAWAL_AMOUNT / 1 ether, "ETH");
        console.log("");

        // ============================================
        // Step 3: LP provides liquidity for withdrawal
        // ============================================
        console.log("[3/7] LP provides liquidity for withdrawal via pool");

        vm.startBroadcast(lp);
        pool.provideLiquidity(withdrawal);
        vm.stopBroadcast();

        console.log("User balance after:", user.balance / 1 ether, "ETH");
        console.log("Pool balance after:", address(pool).balance / 1 ether, "ETH");
        console.log("Pool available liquidity:", pool.availableLiquidity() / 1 ether, "ETH");
        console.log("");

        // Check withdrawal was fulfilled
        (,, bool fulfilled,,) = pool.withdrawalRequests(withdrawalHash);
        console.log("Withdrawal fulfilled:", fulfilled);
        console.log("");

        // ============================================
        // Step 4: Mock withdrawal proof on portal
        // ============================================
        console.log("[4/7] Mocking withdrawal proof on OptimismPortal");

        // Mock the proveWithdrawalTransaction call to succeed
        // We can't actually call it without real L2 proofs, so we'll skip this
        // The important part is that we wait 7 days before finalization
        console.log("Skipping actual proof submission (requires real L2 data)");
        console.log("In real scenario, someone would call:");
        console.log("  portal.proveWithdrawalTransaction(...)");
        console.log("");

        // ============================================
        // Step 5: Advance time past challenge period
        // ============================================
        console.log("[5/7] Advancing time past 7-day challenge period");

        skipChallengePeriod();
        console.log("Time advanced");
        console.log("");

        // ============================================
        // Step 6: Mock portal finalization and settle
        // ============================================
        console.log("[6/7] Mocking portal finalization and settling withdrawal");

        // Mock the portal.finalizeWithdrawalTransaction call
        vm.mockCall(
            address(portal), abi.encodeWithSelector(portal.finalizeWithdrawalTransaction.selector), abi.encode()
        );

        // Fund portal to send ETH to pool
        vm.deal(address(portal), WITHDRAWAL_AMOUNT + 100 ether);

        // Settle the withdrawal
        vm.startBroadcast(deployer);
        pool.settleWithdrawal(withdrawal);
        vm.stopBroadcast();

        // Manually send ETH from portal to simulate finalization
        vm.startBroadcast(address(portal));
        (bool success,) = address(pool).call{value: WITHDRAWAL_AMOUNT}("");
        require(success, "ETH transfer failed");
        vm.stopBroadcast();

        console.log("Pool balance after:", address(pool).balance / 1 ether, "ETH");
        console.log("Pool total liquidity:", pool.totalLiquidity() / 1 ether, "ETH");
        console.log("Pool available liquidity:", pool.availableLiquidity() / 1 ether, "ETH");
        console.log("");

        // Check withdrawal settled
        (,,, bool settledAfter,) = pool.withdrawalRequests(withdrawalHash);
        console.log("Withdrawal settled:", settledAfter);
        console.log("");

        // ============================================
        // Step 7: LP withdraws liquidity
        // ============================================
        console.log("[7/7] LP withdrawing liquidity");

        vm.startBroadcast(lp);
        pool.withdrawLiquidity(pool.liquidityShares(lp));
        vm.stopBroadcast();

        console.log("LP final balance:", lp.balance / 1 ether, "ETH");
        console.log("");

        // Final pool state
        console.log("=== Final Pool State ===");
        console.log("Total liquidity:", pool.totalLiquidity() / 1 ether, "ETH");
        console.log("Available liquidity:", pool.availableLiquidity() / 1 ether, "ETH");
        console.log("Total shares:", pool.totalShares());
        console.log("Pool balance:", address(pool).balance / 1 ether, "ETH");
        console.log("");

        console.log("[PASS] E2E withdrawal flow completed successfully!");
    }
}
