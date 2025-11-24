// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test} from "forge-std/Test.sol";
import {WithdrawalLiquidityPool} from "../../contracts/WithdrawalLiquidityPool.sol";
import {Types} from "src/libraries/Types.sol";
import {Hashing} from "src/libraries/Hashing.sol";
import {MockOptimismPortal} from "../mocks/MockOptimismPortal.sol";

/**
 * @title IntegrationTest
 * @notice End-to-end integration tests for the WithdrawalLiquidityPool
 * @dev Tests complete workflows including happy paths and edge cases
 */
contract IntegrationTest is Test {
    WithdrawalLiquidityPool public pool;
    MockOptimismPortal public optimismPortal;

    address public owner = address(this);
    address public lp1 = address(0x1);
    address public lp2 = address(0x2);
    address public user1 = address(0x1001);
    address public user2 = address(0x1002);

    function setUp() public {
        optimismPortal = new MockOptimismPortal();
        pool = new WithdrawalLiquidityPool(payable(address(optimismPortal)));

        // Fund LPs
        vm.deal(lp1, 100 ether);
        vm.deal(lp2, 100 ether);
    }

    function createWithdrawal(uint256 nonce, address sender, uint256 value, bytes memory data)
        internal
        view
        returns (Types.WithdrawalTransaction memory)
    {
        return Types.WithdrawalTransaction({
            nonce: nonce,
            sender: sender,
            target: address(pool),
            value: value,
            gasLimit: 100000,
            data: data
        });
    }

    /*//////////////////////////////////////////////////////////////
                        FULL INSTANT WITHDRAWAL FLOW
    //////////////////////////////////////////////////////////////*/

    function test_Integration_FullInstantWithdrawalFlow() public {
        // Step 1: LP deposits liquidity
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Verify LP received shares
        assertEq(pool.getShares(lp1), 10 ether);
        assertEq(pool.totalLiquidity(), 10 ether);

        // Step 2: Set fee rate
        pool.setFeeRate(500); // 5%

        // Step 3: User initiates withdrawal on L2 (simulated)
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(1, user1, 1 ether, "");
        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);

        // Step 4: LP provides instant liquidity
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Verify user received funds (minus fee)
        assertEq(user1.balance, 0.95 ether);

        // Verify withdrawal status
        (bool fulfilled, bool settled, bool claimed,,) = pool.getWithdrawalStatus(withdrawalHash);
        assertTrue(fulfilled);
        assertFalse(settled);
        assertFalse(claimed);

        // Step 5: Wait 7 days and settle withdrawal (portal will send ETH automatically)
        vm.warp(block.timestamp + 7 days);

        // Fund portal so it can send ETH to pool
        vm.deal(address(optimismPortal), 10 ether);

        // Step 6: Settle withdrawal (portal automatically sends ETH to pool)
        pool.settleWithdrawal(withdrawal);

        // Verify settlement
        (, settled,,,) = pool.getWithdrawalStatus(withdrawalHash);
        assertTrue(settled);

        // Verify LP earned fee
        assertEq(pool.totalLiquidity(), 10.05 ether); // 10 + 0.05 fee
        assertEq(pool.availableLiquidity(), 10.05 ether);

        // Step 7: LP withdraws liquidity with profit
        uint256 lp1SharesBefore = pool.getShares(lp1);
        uint256 totalSharesBefore = pool.totalShares();
        uint256 totalLiqBefore = pool.totalLiquidity();

        // Debug: Log values
        emit log_named_uint("LP1 shares", lp1SharesBefore);
        emit log_named_uint("Total shares", totalSharesBefore);
        emit log_named_uint("Total liquidity", totalLiqBefore);

        vm.prank(lp1);
        pool.withdrawLiquidity(lp1SharesBefore);

        // LP should have profit from fee
        // Started with 100, deposited 10, withdrew 10.05
        assertEq(lp1.balance, 100.05 ether);
    }

    /*//////////////////////////////////////////////////////////////
                        FULL FALLBACK FLOW
    //////////////////////////////////////////////////////////////*/

    function test_Integration_FullFallbackFlow() public {
        // Step 1: User initiates withdrawal on L2 (simulated)
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(1, user1, 1 ether, "");
        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);

        // Step 2: No LP provides liquidity (7 days pass)
        vm.warp(block.timestamp + 7 days);

        // Step 3: Fund portal so it can finalize
        vm.deal(address(optimismPortal), 10 ether);

        // Step 4: User claims fallback withdrawal (portal will finalize and send ETH automatically)
        pool.claimFallbackWithdrawal(withdrawal);

        // Verify user received full amount (no fee)
        assertEq(user1.balance, 1 ether);

        // Verify withdrawal status
        (bool fulfilled, bool settled, bool claimed,,) = pool.getWithdrawalStatus(withdrawalHash);
        assertFalse(fulfilled);
        assertFalse(settled);
        assertTrue(claimed);
    }

    /*//////////////////////////////////////////////////////////////
                        MULTI-LP COMPETITION
    //////////////////////////////////////////////////////////////*/

    function test_Integration_MultiLPCompetition() public {
        // Both LPs deposit
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        vm.prank(lp2);
        pool.depositLiquidity{value: 10 ether}();

        // Create withdrawal
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(1, user1, 1 ether, "");

        // LP1 fulfills first
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // LP2 tries to fulfill - should revert
        vm.expectRevert(WithdrawalLiquidityPool.AlreadyFulfilled.selector);
        vm.prank(lp2);
        pool.provideLiquidity(withdrawal);

        // Verify only user received funds once
        assertEq(user1.balance, 1 ether);
    }

    /*//////////////////////////////////////////////////////////////
                        MIXED INSTANT + FALLBACK SCENARIOS
    //////////////////////////////////////////////////////////////*/

    function test_Integration_MixedInstantAndFallback() public {
        // Setup
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();
        pool.setFeeRate(500);

        // Withdrawal 1: Instant (LP fulfills)
        Types.WithdrawalTransaction memory withdrawal1 = createWithdrawal(1, user1, 1 ether, "");
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal1);

        // Withdrawal 2: Fallback (no LP fulfills)
        Types.WithdrawalTransaction memory withdrawal2 = createWithdrawal(2, user2, 1 ether, "");

        // Wait 7 days
        vm.warp(block.timestamp + 7 days);

        // Fund portal for finalization
        vm.deal(address(optimismPortal), 10 ether);

        // Settle withdrawal 1 (instant) - portal sends ETH automatically
        pool.settleWithdrawal(withdrawal1);

        // Claim withdrawal 2 (fallback) - portal sends ETH automatically
        pool.claimFallbackWithdrawal(withdrawal2);

        // Verify both users received funds
        assertEq(user1.balance, 0.95 ether); // Got instant with fee
        assertEq(user2.balance, 1 ether); // Got fallback no fee

        // Verify pool earned fee from instant withdrawal only
        assertEq(pool.totalLiquidity(), 10.05 ether);
    }

    /*//////////////////////////////////////////////////////////////
                        MULTIPLE CONCURRENT WITHDRAWALS
    //////////////////////////////////////////////////////////////*/

    function test_Integration_MultipleConcurrentWithdrawals() public {
        // Setup large liquidity
        vm.prank(lp1);
        pool.depositLiquidity{value: 100 ether}();
        pool.setFeeRate(500);

        // Fulfill 10 withdrawals
        Types.WithdrawalTransaction[] memory withdrawals = new Types.WithdrawalTransaction[](10);
        for (uint256 i = 0; i < 10; i++) {
            address user = address(uint160(1000 + i));
            withdrawals[i] = createWithdrawal(i + 1, user, 1 ether, "");

            vm.prank(lp1);
            pool.provideLiquidity(withdrawals[i]);
        }

        // Verify liquidity locked correctly
        assertEq(pool.availableLiquidity(), 90.5 ether); // 100 - (10 * 0.95)

        // Fund portal for all settlements
        vm.deal(address(optimismPortal), 20 ether);

        // Settle all withdrawals (portal sends ETH automatically for each)
        for (uint256 i = 0; i < 10; i++) {
            pool.settleWithdrawal(withdrawals[i]);
        }

        // Verify final state
        assertEq(pool.totalLiquidity(), 100.5 ether); // 100 + (10 * 0.05 fees)
        assertEq(pool.availableLiquidity(), 100.5 ether);
    }

    /*//////////////////////////////////////////////////////////////
                        SHARE VALUE DYNAMICS
    //////////////////////////////////////////////////////////////*/

    function test_Integration_ShareValueIncreasesOverTime() public {
        // LP1 deposits first
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        uint256 initialShareValue = pool.shareValue();

        // Process some profitable withdrawals
        pool.setFeeRate(500);
        for (uint256 i = 0; i < 5; i++) {
            Types.WithdrawalTransaction memory withdrawal =
                createWithdrawal(i + 1, address(uint160(1000 + i)), 1 ether, "");

            vm.prank(lp1);
            pool.provideLiquidity(withdrawal);

            // Fund portal and settle (portal sends ETH automatically)
            vm.deal(address(optimismPortal), 10 ether);
            pool.settleWithdrawal(withdrawal);
        }

        // Verify share value increased
        uint256 finalShareValue = pool.shareValue();
        assertGt(finalShareValue, initialShareValue);

        // LP2 deposits after fees earned (should get fewer shares for same ETH)
        vm.prank(lp2);
        pool.depositLiquidity{value: 10 ether}();

        uint256 lp1Shares = pool.getShares(lp1);
        uint256 lp2Shares = pool.getShares(lp2);

        // LP1 has more shares despite same initial deposit
        assertGt(lp1Shares, lp2Shares);

        // But both can withdraw proportional to their shares
        uint256 lp1Withdrawal = pool.calculateWithdrawalAmount(lp1Shares);
        uint256 lp2Withdrawal = pool.calculateWithdrawalAmount(lp2Shares);

        // LP1 gets more ETH back (earned fees)
        assertGt(lp1Withdrawal, 10 ether);
        // LP2 gets approximately what they put in (minor rounding from share math)
        assertApproxEqAbs(lp2Withdrawal, 10 ether, 0.01 ether);
    }

    /*//////////////////////////////////////////////////////////////
                        EDGE CASE: EXTERNAL FINALIZATION
    //////////////////////////////////////////////////////////////*/

    function test_Integration_ExternalFinalization() public {
        // LP fulfills withdrawal
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();
        pool.setFeeRate(500);

        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(1, user1, 1 ether, "");
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // External party finalizes through portal directly
        vm.deal(address(optimismPortal), 10 ether);
        optimismPortal.finalizeWithdrawalTransaction(withdrawal);

        // Settlement should still work (already finalized path, won't double-send ETH)
        pool.settleWithdrawal(withdrawal);

        // Verify settlement succeeded
        (, bool settled,,,) = pool.getWithdrawalStatus(Hashing.hashWithdrawal(withdrawal));
        assertTrue(settled);
    }
}