// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test} from "forge-std/Test.sol";
import {WithdrawalLiquidityPool} from "../contracts/WithdrawalLiquidityPool.sol";
import {Types} from "src/libraries/Types.sol";
import {Hashing} from "src/libraries/Hashing.sol";

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

    /*//////////////////////////////////////////////////////////////
                    STAGE 2: INSTANT WITHDRAWAL TESTS
    //////////////////////////////////////////////////////////////*/

    event WithdrawalFulfilled(bytes32 indexed withdrawalHash, address indexed user, uint256 amount, uint256 feeRate);
    event FeeRateUpdated(uint256 oldRate, uint256 newRate);

    // Helper function to create a test withdrawal transaction
    function createWithdrawal(uint256 nonce, address sender, uint256 value, bytes memory data)
        internal
        view
        returns (Types.WithdrawalTransaction memory)
    {
        return Types.WithdrawalTransaction({
            nonce: nonce,
            sender: sender,
            target: address(pool), // Pool receives after 7 days
            value: value,
            gasLimit: 100000,
            data: data
        });
    }

    function test_ProvideLiquidity_Success() public {
        // Setup: LP1 deposits liquidity
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Create withdrawal transaction (no custom recipient)
        address user = address(0x999);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(1, user, 1 ether, "");

        bytes32 expectedHash = Hashing.hashWithdrawal(withdrawal);

        // Expect event
        vm.expectEmit(true, true, false, true);
        emit WithdrawalFulfilled(expectedHash, user, 1 ether, 0); // 0% fee

        // Provide liquidity
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Verify withdrawal was fulfilled
        (uint256 amount, uint256 feeRate, bool fulfilled, bool settled) = pool.withdrawalRequests(expectedHash);
        assertEq(amount, 1 ether);
        assertEq(feeRate, 0);
        assertTrue(fulfilled);
        assertFalse(settled);

        // Verify accounting
        assertEq(pool.availableLiquidity(), 9 ether); // 10 - 1 locked
        assertEq(pool.totalLiquidity(), 10 ether); // Unchanged
        assertEq(user.balance, 1 ether); // User received ETH
    }

    function test_ProvideLiquidity_WithCustomRecipient() public {
        // Setup: LP1 deposits liquidity
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Create withdrawal with custom recipient in data
        address sender = address(0x888);
        address customRecipient = address(0x999);
        bytes memory data = abi.encode(customRecipient);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(1, sender, 1 ether, data);

        bytes32 expectedHash = Hashing.hashWithdrawal(withdrawal);

        // Provide liquidity
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Verify custom recipient received ETH (not sender)
        assertEq(customRecipient.balance, 1 ether);
        assertEq(sender.balance, 0);

        // Verify internal state is updated correctly
        (uint256 amount, uint256 feeRate, bool fulfilled, bool settled) = pool.withdrawalRequests(expectedHash);
        assertEq(amount, 1 ether);
        assertEq(feeRate, 0);
        assertTrue(fulfilled);
        assertFalse(settled);
    }

    function test_ProvideLiquidity_WithFee() public {
        // Setup: Set fee rate to 5% (500 basis points)
        pool.setFeeRate(500);

        // LP1 deposits liquidity
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Create withdrawal
        address user = address(0x999);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(1, user, 1 ether, "");

        bytes32 expectedHash = Hashing.hashWithdrawal(withdrawal);

        // Expected: user gets 0.95 ETH (1 ETH - 5% fee)
        uint256 expectedUserAmount = 0.95 ether;

        vm.expectEmit(true, true, false, true);
        emit WithdrawalFulfilled(expectedHash, user, expectedUserAmount, 500);

        // Provide liquidity
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Verify user received amount after fee
        assertEq(user.balance, expectedUserAmount);

        // Verify full amount locked in pool
        assertEq(pool.availableLiquidity(), 9 ether); // 10 - 1 locked
    }

    function test_ProvideLiquidity_RevertsOnZeroAmount() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(1, address(0x999), 0, "");

        vm.expectRevert(WithdrawalLiquidityPool.ZeroAmount.selector);
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);
    }

    function test_ProvideLiquidity_RevertsOnZeroRecipient() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Withdrawal with sender = address(0)
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(1, address(0), 1 ether, "");

        vm.expectRevert(WithdrawalLiquidityPool.ZeroAddress.selector);
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);
    }

    function test_ProvideLiquidity_RevertsOnAlreadyFulfilled() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(1, address(0x999), 1 ether, "");

        // First fulfillment succeeds
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Second fulfillment reverts
        vm.expectRevert(WithdrawalLiquidityPool.AlreadyFulfilled.selector);
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);
    }

    function test_ProvideLiquidity_RevertsOnInsufficientLiquidity() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 1 ether}();

        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(1, address(0x999), 2 ether, "");

        vm.expectRevert(WithdrawalLiquidityPool.InsufficientLiquidity.selector);
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);
    }

    function test_ProvideLiquidity_MultipleConcurrentWithdrawals() public {
        // Setup: LP1 deposits liquidity
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Fulfill 3 withdrawals
        for (uint256 i = 1; i <= 3; i++) {
            // casting to 'uint160' is safe because i is small (1-3) and 1000 + i fits in uint160
            // forge-lint: disable-next-line(unsafe-typecast)
            address user = address(uint160(1000 + i));
            Types.WithdrawalTransaction memory withdrawal = createWithdrawal(i, user, 1 ether, "");

            vm.prank(lp1);
            pool.provideLiquidity(withdrawal);
        }

        // Verify accounting
        assertEq(pool.availableLiquidity(), 7 ether); // 10 - 3 locked
        assertEq(pool.totalLiquidity(), 10 ether);
    }

    function test_ProvideLiquidity_FeeRateLocked() public {
        // Setup
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Set fee rate to 5%
        pool.setFeeRate(500);

        // Create and fulfill withdrawal
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(1, address(0x999), 1 ether, "");
        bytes32 hash = Hashing.hashWithdrawal(withdrawal);

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Change fee rate to 10%
        pool.setFeeRate(1000);

        // Verify locked fee rate is still 5%
        (, uint256 lockedFeeRate,,) = pool.withdrawalRequests(hash);
        assertEq(lockedFeeRate, 500);
    }

    /*//////////////////////////////////////////////////////////////
                        FEE MANAGEMENT TESTS
    //////////////////////////////////////////////////////////////*/

    function test_SetFeeRate_Success() public {
        vm.expectEmit(false, false, false, true);
        emit FeeRateUpdated(0, 500);

        pool.setFeeRate(500);

        assertEq(pool.feeRate(), 500);
    }

    function test_SetFeeRate_RevertsWhenNotOwner() public {
        vm.expectRevert(WithdrawalLiquidityPool.Unauthorized.selector);
        vm.prank(lp1);
        pool.setFeeRate(500);
    }

    function test_SetFeeRate_RevertsOnExceedingMax() public {
        uint256 maxFeeRate = pool.MAX_FEE_RATE();

        vm.expectRevert(WithdrawalLiquidityPool.InvalidFeeRate.selector);
        pool.setFeeRate(maxFeeRate + 1);
    }

    function test_SetFeeRate_AllowsMaxRate() public {
        uint256 maxFeeRate = pool.MAX_FEE_RATE();

        pool.setFeeRate(maxFeeRate);

        assertEq(pool.feeRate(), maxFeeRate);
    }

    /*//////////////////////////////////////////////////////////////
                        RECEIVE FUNCTION TESTS
    //////////////////////////////////////////////////////////////*/

    function test_Receive_AcceptsFromPortal() public {
        // Fund the portal address
        vm.deal(optimismPortal, 10 ether);

        // Portal sends ETH
        vm.prank(optimismPortal);
        (bool success,) = address(pool).call{value: 1 ether}("");
        assertTrue(success);

        assertEq(address(pool).balance, 1 ether);
    }

    function test_Receive_AcceptsFromNonPortal_AsDonation() public {
        // Non-portal address can send ETH - it becomes a donation to the pool
        vm.prank(lp1);
        (bool success,) = address(pool).call{value: 1 ether}("");

        // Call should succeed
        assertTrue(success);

        // Verify pool received the ETH
        assertEq(address(pool).balance, 1 ether);

        // NOTE: These funds are NOT tracked in totalLiquidity/availableLiquidity
        // They will be sitting in the contract until Stage 3 settlement logic
        // or until someone manually calls contributeLiquidity() to properly account for them
    }

    /*//////////////////////////////////////////////////////////////
                        ACCOUNTING INVARIANT TESTS
    //////////////////////////////////////////////////////////////*/

    function test_Invariant_AvailableLiquidityAfterFulfillment() public {
        // Setup
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        uint256 initialAvailable = pool.availableLiquidity();

        // Fulfill withdrawal
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(1, address(0x999), 3 ether, "");
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Invariant: availableLiquidity decreased by locked amount
        assertEq(pool.availableLiquidity(), initialAvailable - 3 ether);

        // Invariant: totalLiquidity unchanged (just locked)
        assertEq(pool.totalLiquidity(), 10 ether);
    }

    function test_Invariant_TotalLiquidityGreaterThanAvailable() public {
        // Setup
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Fulfill withdrawals
        for (uint256 i = 1; i <= 5; i++) {
            // casting to 'uint160' is safe because i is small (1-5) and 1000 + i fits in uint160
            // forge-lint: disable-next-line(unsafe-typecast)
            Types.WithdrawalTransaction memory withdrawal = createWithdrawal(i, address(uint160(1000 + i)), 1 ether, "");
            vm.prank(lp1);
            pool.provideLiquidity(withdrawal);
        }

        // Invariant: totalLiquidity >= availableLiquidity
        assertGe(pool.totalLiquidity(), pool.availableLiquidity());
    }
}
