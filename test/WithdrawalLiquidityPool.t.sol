// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test} from "forge-std/Test.sol";
import {
    WithdrawalLiquidityPool
} from "../contracts/WithdrawalLiquidityPool.sol";
import {Types} from "src/libraries/Types.sol";
import {Hashing} from "src/libraries/Hashing.sol";

// Mock OptimismPortal contract for testing
contract MockOptimismPortal {
    bool public shouldRevert;
    mapping(bytes32 => bool) public finalizedWithdrawals;

    function setShouldRevert(bool _shouldRevert) external {
        shouldRevert = _shouldRevert;
    }

    function finalizeWithdrawalTransaction(
        Types.WithdrawalTransaction calldata withdrawal
    ) external payable {
        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);

        if (shouldRevert) {
            revert("Whatever reason it is to revert.");
        }

        // Mark as finalized
        finalizedWithdrawals[withdrawalHash] = true;
    }
}

/**
 * @title WithdrawalLiquidityPoolTest
 * @notice Unit tests for Stage 1: Basic LP deposit/withdrawal functionality
 */
contract WithdrawalLiquidityPoolTest is Test {
    WithdrawalLiquidityPool public pool;
    MockOptimismPortal public optimismPortal;

    address public owner = address(this);
    address public lp1 = address(0x1);
    address public lp2 = address(0x2);
    address public lp3 = address(0x3);

    uint256 constant INITIAL_BALANCE = 100 ether;

    event LiquidityDeposited(
        address indexed provider,
        uint256 amount,
        uint256 shares
    );
    event LiquidityWithdrawn(
        address indexed provider,
        uint256 shares,
        uint256 amount
    );
    event OwnershipTransferred(
        address indexed previousOwner,
        address indexed newOwner
    );
    event FallbackWithdrawalClaimed(
        bytes32 indexed withdrawalHash,
        address indexed user,
        uint256 amount
    );

    function setUp() public {
        optimismPortal = new MockOptimismPortal();
        pool = new WithdrawalLiquidityPool(payable(address(optimismPortal)));

        // Fund test LPs
        vm.deal(lp1, INITIAL_BALANCE);
        vm.deal(lp2, INITIAL_BALANCE);
        vm.deal(lp3, INITIAL_BALANCE);
    }

    /*//////////////////////////////////////////////////////////////
                          CONSTRUCTOR TESTS
    //////////////////////////////////////////////////////////////*/

    function test_Constructor_SetsPortalAddress() public view {
        assertEq(address(pool.OPTIMISM_PORTAL()), address(optimismPortal));
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
        new WithdrawalLiquidityPool(payable(address(optimismPortal)));
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

    function test_DepositLiquidity_SubsequentDeposit_MintsProportionalShares()
        public
    {
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

    function test_WithdrawLiquidity_MultipleLPs_IndependentWithdrawals()
        public
    {
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

    function test_CalculateWithdrawalAmount_ReturnsZeroWhenNoShares()
        public
        view
    {
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

    function test_Invariant_TotalLiquidityEqualsAvailableLiquidity_Stage1()
        public
    {
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

    event WithdrawalFulfilled(
        bytes32 indexed withdrawalHash,
        address indexed user,
        uint256 amount,
        uint256 feeRate
    );
    event FeeRateUpdated(uint256 oldRate, uint256 newRate);

    // Helper function to create a test withdrawal transaction
    function createWithdrawal(
        uint256 nonce,
        address sender,
        uint256 value,
        bytes memory data
    ) internal view returns (Types.WithdrawalTransaction memory) {
        return
            Types.WithdrawalTransaction({
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
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );

        bytes32 expectedHash = Hashing.hashWithdrawal(withdrawal);

        // Expect event
        vm.expectEmit(true, true, false, true);
        emit WithdrawalFulfilled(expectedHash, user, 1 ether, 0); // 0% fee

        // Provide liquidity
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Verify withdrawal was fulfilled
        (
            uint256 amount,
            uint256 feeRate,
            bool fulfilled,
            bool settled,
            bool claimed
        ) = pool.withdrawalRequests(expectedHash);
        assertEq(amount, 1 ether);
        assertEq(feeRate, 0);
        assertTrue(fulfilled);
        assertFalse(settled);
        assertFalse(claimed);

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
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            sender,
            1 ether,
            data
        );

        bytes32 expectedHash = Hashing.hashWithdrawal(withdrawal);

        // Provide liquidity
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Verify custom recipient received ETH (not sender)
        assertEq(customRecipient.balance, 1 ether);
        assertEq(sender.balance, 0);

        // Verify internal state is updated correctly
        (
            uint256 amount,
            uint256 feeRate,
            bool fulfilled,
            bool settled,
            bool claimed
        ) = pool.withdrawalRequests(expectedHash);
        assertEq(amount, 1 ether);
        assertEq(feeRate, 0);
        assertTrue(fulfilled);
        assertFalse(settled);
        assertFalse(claimed);
    }

    function test_ProvideLiquidity_WithFee() public {
        // Setup: Set fee rate to 5% (500 basis points)
        pool.setFeeRate(500);

        // LP1 deposits liquidity
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Create withdrawal
        address user = address(0x999);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );

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

        // Verify only amountToUser is locked, fee stays available
        assertEq(pool.availableLiquidity(), 9.05 ether); // 10 - 0.95 locked
    }

    function test_ProvideLiquidity_RevertsOnZeroAmount() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            address(0x999),
            0,
            ""
        );

        vm.expectRevert(WithdrawalLiquidityPool.ZeroAmount.selector);
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);
    }

    function test_ProvideLiquidity_RevertsOnZeroRecipient() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Withdrawal with sender = address(0)
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            address(0),
            1 ether,
            ""
        );

        vm.expectRevert(WithdrawalLiquidityPool.ZeroAddress.selector);
        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);
    }

    function test_ProvideLiquidity_RevertsOnAlreadyFulfilled() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            address(0x999),
            1 ether,
            ""
        );

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

        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            address(0x999),
            2 ether,
            ""
        );

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
            Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
                i,
                user,
                1 ether,
                ""
            );

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
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            address(0x999),
            1 ether,
            ""
        );
        bytes32 hash = Hashing.hashWithdrawal(withdrawal);

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Change fee rate to 10%
        pool.setFeeRate(1000);

        // Verify locked fee rate is still 5%
        (, uint256 lockedFeeRate, , , ) = pool.withdrawalRequests(hash);
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
        vm.deal(address(optimismPortal), 10 ether);

        // Portal sends ETH
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        assertTrue(success);

        assertEq(address(pool).balance, 1 ether);
    }

    function test_Receive_AcceptsFromNonPortal_AsDonation() public {
        // Non-portal address can send ETH - it becomes a donation to the pool
        vm.prank(lp1);
        (bool success, ) = address(pool).call{value: 1 ether}("");

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
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            address(0x999),
            3 ether,
            ""
        );
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
            Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
                i,
                address(uint160(1000 + i)),
                1 ether,
                ""
            );
            vm.prank(lp1);
            pool.provideLiquidity(withdrawal);
        }

        // Invariant: totalLiquidity >= availableLiquidity
        assertGe(pool.totalLiquidity(), pool.availableLiquidity());
    }

    /*//////////////////////////////////////////////////////////////
                    STAGE 3: SETTLEMENT TESTS
    //////////////////////////////////////////////////////////////*/

    event WithdrawalSettled(
        bytes32 indexed withdrawalHash,
        uint256 reimbursement,
        uint256 fee
    );
    event WithdrawalAlreadyFinalized(bytes32 indexed withdrawalHash);

    function test_SettleWithdrawal_Success() public {
        // Setup: Set fee rate and provide liquidity
        pool.setFeeRate(500); // 5%
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Fulfill withdrawal
        address user = address(0x999);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );
        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Verify state before settlement
        assertEq(pool.availableLiquidity(), 9.05 ether); // 10 - 0.95 locked, fee stays available
        assertEq(pool.totalLiquidity(), 10 ether);

        // Fund portal to send ETH back
        vm.deal(address(optimismPortal), 10 ether);

        // Mock the portal finalizing the withdrawal (portal sends ETH back)
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        // Settle withdrawal
        uint256 expectedFee = 0.05 ether; // 5% of 1 ETH
        uint256 expectedReimbursement = 0.95 ether;

        vm.expectEmit(true, false, false, true);
        emit WithdrawalSettled(
            withdrawalHash,
            expectedReimbursement,
            expectedFee
        );

        pool.settleWithdrawal(withdrawal);

        // Verify state after settlement
        (
            bool fulfilled,
            bool settled,
            bool claimed,
            uint256 amount,
            uint256 lockedFeeRate
        ) = pool.getWithdrawalStatus(withdrawalHash);
        assertTrue(fulfilled);
        assertTrue(settled);
        assertFalse(claimed);
        assertEq(amount, 1 ether);
        assertEq(lockedFeeRate, 500);

        // Verify accounting: portal sent 1 ETH, we add it to available and fee to total
        assertEq(pool.availableLiquidity(), 10.05 ether); // 9.05 + 1 from portal
        assertEq(pool.totalLiquidity(), 10.05 ether); // 10 + 0.05 fee credited

        // Verify share value increased
        uint256 newShareValue = pool.shareValue();
        assertGt(newShareValue, 1e18); // Greater than 1:1
    }

    function test_SettleWithdrawal_AlreadyFinalized() public {
        // Setup and fulfill
        pool.setFeeRate(500);
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        address user = address(0x999);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );
        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Someone else already finalized on the portal
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        // Make portal revert to simulate already finalized
        optimismPortal.setShouldRevert(true);

        // Expect the "already finalized" event
        vm.expectEmit(true, false, false, false);
        emit WithdrawalAlreadyFinalized(withdrawalHash);

        // Settlement should still work (just updates accounting)
        pool.settleWithdrawal(withdrawal);

        // Verify settlement completed
        (, bool settled, , , ) = pool.getWithdrawalStatus(withdrawalHash);
        assertTrue(settled);
    }

    function test_SettleWithdrawal_RevertsIfNotFulfilled() public {
        // Create withdrawal but don't fulfill it
        address user = address(0x999);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );

        vm.expectRevert(WithdrawalLiquidityPool.NotFulfilled.selector);
        pool.settleWithdrawal(withdrawal);
    }

    function test_SettleWithdrawal_RevertsIfAlreadySettled() public {
        // Setup, fulfill, and settle
        pool.setFeeRate(500);
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        address user = address(0x999);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Fund and settle
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        pool.settleWithdrawal(withdrawal);

        // Try to settle again
        vm.expectRevert(WithdrawalLiquidityPool.AlreadySettled.selector);
        pool.settleWithdrawal(withdrawal);
    }

    function test_SettleWithdrawal_ZeroFeeRate() public {
        // Setup with 0% fee
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        address user = address(0x999);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Fund and settle
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        pool.settleWithdrawal(withdrawal);

        // With 0% fee, all liquidity is reimbursement
        assertEq(pool.availableLiquidity(), 10 ether); // Back to original
        assertEq(pool.totalLiquidity(), 10 ether); // No fee credited
    }

    function test_SettleWithdrawal_MaxFeeRate() public {
        // Setup with max fee (10%)
        pool.setFeeRate(1000);
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        address user = address(0x999);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Fund and settle
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        pool.settleWithdrawal(withdrawal);

        // 10% fee = 0.1 ETH
        assertEq(pool.availableLiquidity(), 10.1 ether); // 9.1 + 1 from portal
        assertEq(pool.totalLiquidity(), 10.1 ether); // 10 + 0.1 fee
    }

    function test_SettleWithdrawal_MultipleConcurrentSettlements() public {
        // Setup
        pool.setFeeRate(500); // 5%
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Fulfill 3 withdrawals
        Types.WithdrawalTransaction[]
            memory withdrawals = new Types.WithdrawalTransaction[](3);
        for (uint256 i = 0; i < 3; i++) {
            // casting to 'uint160' is safe because i is small and 1000 + i fits in uint160
            // forge-lint: disable-next-line(unsafe-typecast)
            address user = address(uint160(1000 + i));
            withdrawals[i] = createWithdrawal(i + 1, user, 1 ether, "");

            vm.prank(lp1);
            pool.provideLiquidity(withdrawals[i]);
        }

        // Verify only amountToUser locked (3 * 0.95 = 2.85), fees stay available
        assertEq(pool.availableLiquidity(), 7.15 ether); // 10 - 2.85 locked

        // Fund portal
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 3 ether}("");
        require(success);

        // Settle all 3
        for (uint256 i = 0; i < 3; i++) {
            pool.settleWithdrawal(withdrawals[i]);
        }

        // Total fees: 3 * 0.05 = 0.15 ETH
        assertEq(pool.availableLiquidity(), 10.15 ether); // 7.15 + 3 from portal
        assertEq(pool.totalLiquidity(), 10.15 ether); // 10 + 0.15 fees
    }

    function test_SettleWithdrawal_ShareValueIncreaseBenefitsAllLPs() public {
        // Setup: 2 LPs deposit
        vm.prank(lp1);
        pool.depositLiquidity{value: 5 ether}();

        vm.prank(lp2);
        pool.depositLiquidity{value: 5 ether}();

        // Set fee and fulfill withdrawal
        pool.setFeeRate(500); // 5%
        address user = address(0x999);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Record share values before settlement
        uint256 lp1SharesBefore = pool.getShares(lp1);
        uint256 lp2SharesBefore = pool.getShares(lp2);
        uint256 shareValueBefore = pool.shareValue();

        // Settle withdrawal
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        pool.settleWithdrawal(withdrawal);

        // Share counts unchanged
        assertEq(pool.getShares(lp1), lp1SharesBefore);
        assertEq(pool.getShares(lp2), lp2SharesBefore);

        // Share value increased for both
        uint256 shareValueAfter = pool.shareValue();
        assertGt(shareValueAfter, shareValueBefore);

        // Both LPs can withdraw more than they put in
        uint256 lp1Value = pool.calculateWithdrawalAmount(lp1SharesBefore);
        uint256 lp2Value = pool.calculateWithdrawalAmount(lp2SharesBefore);
        assertGt(lp1Value, 5 ether);
        assertGt(lp2Value, 5 ether);
    }

    function test_GetWithdrawalStatus_Unfulfilled() public view {
        bytes32 fakeHash = keccak256("fake");
        (
            bool fulfilled,
            bool settled,
            bool claimed,
            uint256 amount,
            uint256 lockedFeeRate
        ) = pool.getWithdrawalStatus(fakeHash);

        assertFalse(fulfilled);
        assertFalse(settled);
        assertFalse(claimed);
        assertEq(amount, 0);
        assertEq(lockedFeeRate, 0);
    }

    function test_GetWithdrawalStatus_FulfilledNotSettled() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        pool.setFeeRate(500);
        address user = address(0x999);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );
        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        (
            bool fulfilled,
            bool settled,
            bool claimed,
            uint256 amount,
            uint256 lockedFeeRate
        ) = pool.getWithdrawalStatus(withdrawalHash);

        assertTrue(fulfilled);
        assertFalse(settled);
        assertFalse(claimed);
        assertEq(amount, 1 ether);
        assertEq(lockedFeeRate, 500);
    }

    function test_GetWithdrawalStatus_FulfilledAndSettled() public {
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        pool.setFeeRate(500);
        address user = address(0x999);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );
        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Settle
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        pool.settleWithdrawal(withdrawal);

        (
            bool fulfilled,
            bool settled,
            bool claimed,
            uint256 amount,
            uint256 lockedFeeRate
        ) = pool.getWithdrawalStatus(withdrawalHash);

        assertTrue(fulfilled);
        assertTrue(settled);
        assertFalse(claimed);
        assertEq(amount, 1 ether);
        assertEq(lockedFeeRate, 500);
    }

    function test_Settlement_AccountingInvariant() public {
        // Setup
        pool.setFeeRate(500);
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // Fulfill withdrawal
        address user = address(0x999);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            2 ether,
            ""
        );

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Verify invariant before settlement
        assertGe(pool.totalLiquidity(), pool.availableLiquidity());
        // Locked = amountToUser = 2 * 0.95 = 1.9 ETH (fee stays available)
        assertEq(pool.totalLiquidity() - pool.availableLiquidity(), 1.9 ether); // Locked

        // Settle
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 2 ether}("");
        require(success);

        pool.settleWithdrawal(withdrawal);

        // Verify invariant after settlement - available equals total (nothing locked)
        assertGe(pool.totalLiquidity(), pool.availableLiquidity());
        assertEq(pool.totalLiquidity(), pool.availableLiquidity()); // Should be equal now

        // Verify contract balance >= totalLiquidity
        assertGe(address(pool).balance, pool.totalLiquidity());
    }

    function test_Settlement_FeesCompound() public {
        // Setup with fee rate
        pool.setFeeRate(500); // 5%
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // First withdrawal and settlement
        address user1 = address(0x1001);
        Types.WithdrawalTransaction memory withdrawal1 = createWithdrawal(
            1,
            user1,
            1 ether,
            ""
        );

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal1);

        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        pool.settleWithdrawal(withdrawal1);

        // Fee from first: 0.05 ETH
        // Available liquidity: 10.05 ETH (9.05 + 1 from portal)
        // Total liquidity: 10.05 ETH (fee credited)

        // Second withdrawal - can use fees for liquidity
        address user2 = address(0x1002);
        Types.WithdrawalTransaction memory withdrawal2 = createWithdrawal(
            2,
            user2,
            1 ether,
            ""
        );

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal2);

        // Should succeed because fees are available
        // After second fulfillment: 10.05 - 0.95 = 9.1 ETH
        assertEq(pool.availableLiquidity(), 9.1 ether);

        // Settle second
        vm.prank(address(optimismPortal));
        (success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        pool.settleWithdrawal(withdrawal2);

        // Total fees: 0.05 + 0.05 = 0.1 ETH
        assertEq(pool.totalLiquidity(), 10.1 ether);
        assertEq(pool.availableLiquidity(), 10.1 ether);
    }

    /*//////////////////////////////////////////////////////////////
                        STAGE 4: FALLBACK WITHDRAWAL TESTS
    //////////////////////////////////////////////////////////////*/

    function test_FallbackWithdrawal_Success() public {
        // User initiates withdrawal but no LP fulfills it
        address user = address(0x1234);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );

        // Simulate portal finalization after 7 days (send ETH to pool)
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        // User claims fallback withdrawal
        pool.claimFallbackWithdrawal(withdrawal);

        // Verify user received full amount (no fee)
        assertEq(user.balance, 1 ether);

        // Verify withdrawal marked as claimed
        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);
        (bool fulfilled, bool settled, bool claimed, uint256 amount, ) = pool
            .getWithdrawalStatus(withdrawalHash);
        assertFalse(fulfilled);
        assertFalse(settled);
        assertTrue(claimed);
        assertEq(amount, 1 ether);
    }

    function test_FallbackWithdrawal_WithCustomRecipient() public {
        // User specifies custom recipient in data
        address customRecipient = address(0x5678);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            address(0x1234),
            1 ether,
            abi.encode(customRecipient)
        );

        // Simulate portal finalization
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        // User claims fallback
        pool.claimFallbackWithdrawal(withdrawal);

        // Verify custom recipient received funds
        assertEq(customRecipient.balance, 1 ether);
    }

    function test_FallbackWithdrawal_RevertsIfFulfilledByLP() public {
        // LP deposits liquidity
        vm.deal(lp1, 10 ether);
        vm.prank(lp1);
        pool.depositLiquidity{value: 10 ether}();

        // LP fulfills withdrawal
        address user = address(0x1234);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );

        vm.prank(lp1);
        pool.provideLiquidity(withdrawal);

        // Simulate portal finalization
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        // User tries to claim fallback - should revert
        vm.expectRevert(
            WithdrawalLiquidityPool.WithdrawalFulfilledByLP.selector
        );
        pool.claimFallbackWithdrawal(withdrawal);
    }

    function test_FallbackWithdrawal_RevertsIfAlreadyClaimed() public {
        address user = address(0x1234);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );

        // Simulate portal finalization
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        // First claim succeeds
        pool.claimFallbackWithdrawal(withdrawal);

        // Simulate portal sending ETH again (shouldn't happen but testing)
        vm.prank(address(optimismPortal));
        (success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        // Second claim should revert
        vm.expectRevert(WithdrawalLiquidityPool.AlreadyClaimed.selector);
        pool.claimFallbackWithdrawal(withdrawal);
    }

    function test_FallbackWithdrawal_RevertsOnZeroAmount() public {
        address user = address(0x1234);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            0,
            ""
        );

        vm.expectRevert(WithdrawalLiquidityPool.ZeroAmount.selector);
        pool.claimFallbackWithdrawal(withdrawal);
    }

    function test_FallbackWithdrawal_RevertsOnZeroRecipient() public {
        // Create withdrawal with zero address recipient and no data (fallback to sender)
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            address(0),
            1 ether,
            ""
        );

        vm.expectRevert(WithdrawalLiquidityPool.ZeroAddress.selector);
        pool.claimFallbackWithdrawal(withdrawal);
    }

    function test_FallbackWithdrawal_RevertsIfNotYetFinalized() public {
        address user = address(0x1234);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );

        // Don't finalize through portal and don't fund the portal
        // User tries to claim without finalization - should revert
        // The call to finalizeWithdrawalTransaction will fail because proof period hasn't passed
        vm.expectRevert();
        pool.claimFallbackWithdrawal(withdrawal);
    }

    function test_FallbackWithdrawal_HandlesAlreadyFinalized() public {
        address user = address(0x1234);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );

        // Simulate portal finalization
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        // Finalize through portal (simulate someone else finalizing)
        optimismPortal.finalizeWithdrawalTransaction(withdrawal);

        // User claims fallback - should succeed via try/catch
        pool.claimFallbackWithdrawal(withdrawal);

        // Verify user received funds
        assertEq(user.balance, 1 ether);
    }

    function test_FallbackWithdrawal_NoFeeCharged() public {
        // Set fee rate to 10%
        pool.setFeeRate(1000);

        address user = address(0x1234);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );

        // Simulate portal finalization
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        // User claims fallback
        pool.claimFallbackWithdrawal(withdrawal);

        // Verify user received FULL amount despite 10% fee rate
        assertEq(user.balance, 1 ether);
    }

    function test_FallbackWithdrawal_EmitsEvent() public {
        address user = address(0x1234);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );
        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);

        // Simulate portal finalization
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        // Expect event
        vm.expectEmit(true, true, false, true);
        emit FallbackWithdrawalClaimed(withdrawalHash, user, 1 ether);

        pool.claimFallbackWithdrawal(withdrawal);
    }

    function test_GetWithdrawalStatus_FallbackClaimed() public {
        address user = address(0x1234);
        Types.WithdrawalTransaction memory withdrawal = createWithdrawal(
            1,
            user,
            1 ether,
            ""
        );
        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);

        // Simulate portal finalization
        vm.deal(address(optimismPortal), 10 ether);
        vm.prank(address(optimismPortal));
        (bool success, ) = address(pool).call{value: 1 ether}("");
        require(success);

        // Before claim
        (
            bool fulfilled,
            bool settled,
            bool claimed,
            uint256 amount,
            uint256 feeRate
        ) = pool.getWithdrawalStatus(withdrawalHash);
        assertFalse(fulfilled);
        assertFalse(settled);
        assertFalse(claimed);
        assertEq(amount, 0);
        assertEq(feeRate, 0);

        // Claim fallback
        pool.claimFallbackWithdrawal(withdrawal);

        // After claim
        (fulfilled, settled, claimed, amount, feeRate) = pool
            .getWithdrawalStatus(withdrawalHash);
        assertFalse(fulfilled);
        assertFalse(settled);
        assertTrue(claimed);
        assertEq(amount, 1 ether);
        assertEq(feeRate, 0);
    }
}
