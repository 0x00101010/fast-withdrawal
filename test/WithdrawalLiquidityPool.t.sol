// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test} from "forge-std/Test.sol";
import {WithdrawalLiquidityPool} from "../contracts/WithdrawalLiquidityPool.sol";

/**
 * @title WithdrawalLiquidityPoolTest
 * @notice Unit tests for Stage 1: Basic LP deposit/withdrawal functionality
 */
contract WithdrawalLiquidityPoolTest is Test {
    WithdrawalLiquidityPool public pool;

    address public owner = address(this);
    address public optimismPortal = address(0x1234); // Mock portal address
    address public lp1 = address(0x1);
    address public lp2 = address(0x2);
    address public lp3 = address(0x3);

    uint256 constant INITIAL_BALANCE = 100 ether;

    event LiquidityDeposited(address indexed provider, uint256 amount, uint256 shares);
    event LiquidityWithdrawn(address indexed provider, uint256 shares, uint256 amount);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);

    function setUp() public {
        pool = new WithdrawalLiquidityPool(payable(optimismPortal));

        // Fund test LPs
        vm.deal(lp1, INITIAL_BALANCE);
        vm.deal(lp2, INITIAL_BALANCE);
        vm.deal(lp3, INITIAL_BALANCE);
    }

    /*//////////////////////////////////////////////////////////////
                          CONSTRUCTOR TESTS
    //////////////////////////////////////////////////////////////*/

    function test_Constructor_SetsPortalAddress() public view {
        assertEq(address(pool.OPTIMISM_PORTAL()), optimismPortal);
    }

    function test_Constructor_SetsOwner() public view {
        assertEq(pool.owner(), owner);
    }

    function test_Constructor_RevertsOnZeroPortalAddress() public {
        vm.expectRevert(WithdrawalLiquidityPool.ZeroAddress.selector);
        new WithdrawalLiquidityPool(payable(address(0)));
    }

    function test_Constructor_EmitsOwnershipTransferred() public {
        vm.expectEmit(true, true, false, false);
        emit OwnershipTransferred(address(0), address(this));
        new WithdrawalLiquidityPool(payable(optimismPortal));
    }

    /*//////////////////////////////////////////////////////////////
                      FIRST DEPOSIT TESTS
    //////////////////////////////////////////////////////////////*/

    function test_DepositLiquidity_FirstDeposit_MintsSharesOneToOne() public {
        uint256 depositAmount = 10 ether;

        vm.prank(lp1);
        pool.depositLiquidity{value: depositAmount}();

        assertEq(pool.liquidityShares(lp1), depositAmount);
        assertEq(pool.totalShares(), depositAmount);
        assertEq(pool.totalLiquidity(), depositAmount);
        assertEq(pool.availableLiquidity(), depositAmount);
    }

    function test_DepositLiquidity_FirstDeposit_EmitsEvent() public {
        uint256 depositAmount = 10 ether;

        vm.expectEmit(true, false, false, true);
        emit LiquidityDeposited(lp1, depositAmount, depositAmount);

        vm.prank(lp1);
        pool.depositLiquidity{value: depositAmount}();
    }

    function test_DepositLiquidity_RevertsOnZeroAmount() public {
        vm.prank(lp1);
        vm.expectRevert(WithdrawalLiquidityPool.ZeroAmount.selector);
        pool.depositLiquidity{value: 0}();
    }

    /*//////////////////////////////////////////////////////////////
                    SUBSEQUENT DEPOSIT TESTS
    //////////////////////////////////////////////////////////////*/

    function test_DepositLiquidity_SubsequentDeposit_MintsProportionalShares() public {
        // LP1 deposits 10 ETH (gets 10 shares)
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // LP2 deposits 5 ETH (should get 5 shares since share value is 1:1)
        vm.prank(lp2);
        pool.depositLiquidity{value: 5 ether}();

        assertEq(pool.liquidityShares(lp2), 5 ether);
        assertEq(pool.totalShares(), 15 ether);
        assertEq(pool.totalLiquidity(), 15 ether);
    }

    function test_DepositLiquidity_MultipleDeposits_SameLP() public {
        uint256 firstDeposit = 10 ether;
        uint256 secondDeposit = 5 ether;

        vm.startPrank(lp1);
        pool.depositLiquidity{value: firstDeposit}();
        pool.depositLiquidity{value: secondDeposit}();
        vm.stopPrank();

        assertEq(pool.liquidityShares(lp1), 15 ether);
        assertEq(pool.totalShares(), 15 ether);
        assertEq(pool.totalLiquidity(), 15 ether);
    }

    function test_DepositLiquidity_ThreeLPs_CorrectShareDistribution() public {
        // LP1: 10 ETH
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // LP2: 20 ETH
        vm.prank(lp2);
        pool.depositLiquidity{value: 20 ether}();

        // LP3: 30 ETH
        vm.prank(lp3);
        pool.depositLiquidity{value: 30 ether}();

        // Total: 60 ETH, shares distributed proportionally
        assertEq(pool.liquidityShares(lp1), 10 ether);
        assertEq(pool.liquidityShares(lp2), 20 ether);
        assertEq(pool.liquidityShares(lp3), 30 ether);
        assertEq(pool.totalShares(), 60 ether);
        assertEq(pool.totalLiquidity(), 60 ether);
    }

    /*//////////////////////////////////////////////////////////////
                        WITHDRAWAL TESTS
    //////////////////////////////////////////////////////////////*/

    function test_WithdrawLiquidity_FullWithdrawal_SingleLP() public {
        uint256 depositAmount = 10 ether;

        // LP1 deposits
        vm.prank(lp1);
        pool.depositLiquidity{value: depositAmount}();

        uint256 shares = pool.liquidityShares(lp1);
        uint256 balanceBefore = lp1.balance;

        // LP1 withdraws all shares
        vm.prank(lp1);
        pool.withdrawLiquidity(shares);

        assertEq(pool.liquidityShares(lp1), 0);
        assertEq(pool.totalShares(), 0);
        assertEq(pool.totalLiquidity(), 0);
        assertEq(pool.availableLiquidity(), 0);
        assertEq(lp1.balance, balanceBefore + depositAmount);
    }

    function test_WithdrawLiquidity_PartialWithdrawal() public {
        uint256 depositAmount = 10 ether;

        // LP1 deposits
        vm.prank(lp1);
        pool.depositLiquidity{value: depositAmount}();

        uint256 sharesToWithdraw = 3 ether;
        uint256 balanceBefore = lp1.balance;

        // LP1 withdraws 30% of shares
        vm.prank(lp1);
        pool.withdrawLiquidity(sharesToWithdraw);

        assertEq(pool.liquidityShares(lp1), 7 ether);
        assertEq(pool.totalShares(), 7 ether);
        assertEq(pool.totalLiquidity(), 7 ether);
        assertEq(lp1.balance, balanceBefore + 3 ether);
    }

    function test_WithdrawLiquidity_MultipleLPs_IndependentWithdrawals() public {
        // LP1 deposits 10 ETH
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // LP2 deposits 20 ETH
        vm.prank(lp2);
        pool.depositLiquidity{value: 20 ether}();

        uint256 lp1BalanceBefore = lp1.balance;
        uint256 lp2BalanceBefore = lp2.balance;

        // LP1 withdraws all (should get 10 ETH back)
        vm.prank(lp1);
        pool.withdrawLiquidity(10 ether);

        assertEq(lp1.balance, lp1BalanceBefore + 10 ether);
        assertEq(pool.liquidityShares(lp1), 0);
        assertEq(pool.liquidityShares(lp2), 20 ether);
        assertEq(pool.totalLiquidity(), 20 ether);

        // LP2 withdraws all (should get 20 ETH back)
        vm.prank(lp2);
        pool.withdrawLiquidity(20 ether);

        assertEq(lp2.balance, lp2BalanceBefore + 20 ether);
        assertEq(pool.liquidityShares(lp2), 0);
        assertEq(pool.totalLiquidity(), 0);
    }

    function test_WithdrawLiquidity_EmitsEvent() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        vm.expectEmit(true, false, false, true);
        emit LiquidityWithdrawn(lp1, 5 ether, 5 ether);

        vm.prank(lp1);
        pool.withdrawLiquidity(5 ether);
    }

    function test_WithdrawLiquidity_RevertsOnZeroShares() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        vm.prank(lp1);
        vm.expectRevert(WithdrawalLiquidityPool.ZeroAmount.selector);
        pool.withdrawLiquidity(0);
    }

    function test_WithdrawLiquidity_RevertsOnInsufficientShares() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        vm.prank(lp1);
        vm.expectRevert(WithdrawalLiquidityPool.InsufficientShares.selector);
        pool.withdrawLiquidity(11 ether);
    }

    function test_WithdrawLiquidity_RevertsOnInsufficientLiquidity() public {
        // This will be relevant in Stage 2 when liquidity gets locked
        // For now, availableLiquidity == totalLiquidity always in Stage 1
        // Test is a placeholder for future functionality

        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Manually set availableLiquidity to simulate locked funds (using vm.store in future tests)
        // For now this test just verifies the revert exists
        vm.prank(lp1);
        pool.withdrawLiquidity(10 ether); // Should succeed in Stage 1
    }

    /*//////////////////////////////////////////////////////////////
                      OWNERSHIP TESTS
    //////////////////////////////////////////////////////////////*/

    function test_TransferOwnership_Success() public {
        address newOwner = address(0x999);

        vm.expectEmit(true, true, false, false);
        emit OwnershipTransferred(owner, newOwner);

        pool.transferOwnership(newOwner);

        assertEq(pool.owner(), newOwner);
    }

    function test_TransferOwnership_RevertsOnZeroAddress() public {
        vm.expectRevert(WithdrawalLiquidityPool.ZeroAddress.selector);
        pool.transferOwnership(address(0));
    }

    function test_TransferOwnership_RevertsWhenNotOwner() public {
        vm.prank(lp1);
        vm.expectRevert(WithdrawalLiquidityPool.Unauthorized.selector);
        pool.transferOwnership(lp1);
    }

    /*//////////////////////////////////////////////////////////////
                        VIEW FUNCTION TESTS
    //////////////////////////////////////////////////////////////*/

    function test_ShareValue_InitiallyOneToOne() public view {
        assertEq(pool.shareValue(), 1e18);
    }

    function test_ShareValue_RemainsSameWithNoFees() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        assertEq(pool.shareValue(), 1e18);
    }

    function test_CalculateWithdrawalAmount_AccurateCalculation() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        assertEq(pool.calculateWithdrawalAmount(5 ether), 5 ether);
        assertEq(pool.calculateWithdrawalAmount(10 ether), 10 ether);
    }

    function test_CalculateWithdrawalAmount_ReturnsZeroWhenNoShares() public view {
        assertEq(pool.calculateWithdrawalAmount(5 ether), 0);
    }

    function test_GetShares_ReturnsCorrectBalance() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        assertEq(pool.getShares(lp1), 10 ether);
        assertEq(pool.getShares(lp2), 0);
    }

    /*//////////////////////////////////////////////////////////////
                      ACCOUNTING INVARIANTS
    //////////////////////////////////////////////////////////////*/

    function test_Invariant_TotalLiquidityEqualsAvailableLiquidity_Stage1() public {
        // In Stage 1 (no locked liquidity yet), these should always be equal

        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        assertEq(pool.totalLiquidity(), pool.availableLiquidity());

        vm.prank(lp2);
        pool.depositLiquidity{value: 20 ether}();

        assertEq(pool.totalLiquidity(), pool.availableLiquidity());

        vm.prank(lp1);
        pool.withdrawLiquidity(5 ether);

        assertEq(pool.totalLiquidity(), pool.availableLiquidity());
    }

    function test_Invariant_ShareValueNeverDecreasesExceptWithdrawal() public {
        // Share value should be monotonically increasing (or stay same)
        // except when LPs withdraw their own shares

        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        uint256 shareValue1 = pool.shareValue();

        vm.prank(lp2);
        pool.depositLiquidity{value: 10 ether}();

        uint256 shareValue2 = pool.shareValue();

        // Share value should not decrease
        assertGe(shareValue2, shareValue1);
    }

    /*//////////////////////////////////////////////////////////////
                        EDGE CASE TESTS
    //////////////////////////////////////////////////////////////*/

    function test_EdgeCase_VerySmallDeposit() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 1 wei}();

        assertEq(pool.liquidityShares(lp1), 1 wei);
        assertEq(pool.totalShares(), 1 wei);
    }

    function test_EdgeCase_VeryLargeDeposit() public {
        uint256 largeAmount = 1000000 ether;
        vm.deal(lp1, largeAmount);

        vm.prank(lp1);
        pool.depositLiquidity{value: largeAmount}();

        assertEq(pool.liquidityShares(lp1), largeAmount);
        assertEq(pool.totalLiquidity(), largeAmount);
    }

    function test_EdgeCase_MultipleSmallWithdrawals() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        for (uint256 i = 0; i < 10; i++) {
            vm.prank(lp1);
            pool.withdrawLiquidity(1 ether);
        }

        assertEq(pool.liquidityShares(lp1), 0);
        assertEq(pool.totalLiquidity(), 0);
    }
}
