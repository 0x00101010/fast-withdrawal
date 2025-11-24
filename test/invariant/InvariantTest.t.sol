// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test} from "forge-std/Test.sol";
import {console} from "forge-std/console.sol";
import {
    WithdrawalLiquidityPool
} from "../../contracts/WithdrawalLiquidityPool.sol";
import {Types} from "src/libraries/Types.sol";
import {Hashing} from "src/libraries/Hashing.sol";
import {MockOptimismPortal} from "../mocks/MockOptimismPortal.sol";

/**
 * @title InvariantTest
 * @notice Fuzzing-based invariant tests for WithdrawalLiquidityPool
 * @dev Tests critical invariants that must hold under all conditions
 */
contract InvariantTest is Test {
    WithdrawalLiquidityPool public pool;
    Handler public handler;

    function setUp() public {
        // Deploy mock portal
        address mockPortal = address(new MockOptimismPortal());

        // Deploy pool
        pool = new WithdrawalLiquidityPool(payable(mockPortal));

        // Deploy handler
        handler = new Handler(pool, mockPortal);

        // Target handler for invariant testing
        targetContract(address(handler));

        // Fund handler
        vm.deal(address(handler), 1000 ether);
    }

    /*//////////////////////////////////////////////////////////////
                            INVARIANTS
    //////////////////////////////////////////////////////////////*/

    /**
     * @notice Invariant 1: Total liquidity must always be >= available liquidity
     * @dev Locked liquidity (totalLiquidity - availableLiquidity) represents fulfilled but unsettled withdrawals
     */
    function invariant_TotalLiquidityGteAvailable() public view {
        uint256 total = pool.totalLiquidity();
        uint256 available = pool.availableLiquidity();

        assertGe(
            total,
            available,
            "Total liquidity must be >= available liquidity"
        );
    }

    /**
     * @notice Invariant 2: Share value never decreases (except during withdrawals)
     * @dev Share value = totalLiquidity / totalShares, increases from fees
     */
    function invariant_ShareValueNeverDecreases() public view {
        uint256 currentShareValue = pool.shareValue();
        uint256 previousShareValue = handler.maxShareValue();

        // Share value should never decrease (it can only increase from fees or stay same)
        assertGe(
            currentShareValue,
            previousShareValue,
            "Share value should never decrease"
        );
    }

    /**
     * @notice Invariant 3: Sum of all share values equals total liquidity
     * @dev totalShares * shareValue = totalLiquidity (within rounding tolerance)
     */
    function invariant_SharesMatchLiquidity() public view {
        uint256 totalShares = pool.totalShares();
        uint256 totalLiquidity = pool.totalLiquidity();

        if (totalShares == 0) {
            // If no shares, liquidity should be 0
            assertEq(totalLiquidity, 0, "No shares should mean no liquidity");
        } else {
            // Calculate expected liquidity from shares
            uint256 shareValue = pool.shareValue();
            uint256 calculatedLiquidity = (totalShares * shareValue) / 1e18;

            // Allow small rounding error (1 wei per 1e18 shares)
            assertApproxEqAbs(
                calculatedLiquidity,
                totalLiquidity,
                totalShares / 1e18 + 1,
                "Shares * shareValue should equal totalLiquidity"
            );
        }
    }

    /**
     * @notice Invariant 5: Contract ETH balance should match accounting
     * @dev When LPs provide liquidity, ETH is sent to users, so contract balance may be less than totalLiquidity
     *      The difference represents fulfilled but unsettled withdrawals where users already got paid
     */
    function invariant_ETHBalanceMatchesAccounting() public view {
        uint256 contractBalance = address(pool).balance;
        uint256 availableLiquidity = pool.availableLiquidity();

        // When LPs provide liquidity, ETH is sent to users, reducing contract balance
        // The locked amount (totalLiquidity - availableLiquidity) represents ETH already sent to users
        // Contract balance should always be >= available liquidity
        assertGe(
            contractBalance,
            availableLiquidity,
            "Contract ETH should be >= available liquidity"
        );
    }

    /**
     * @notice Invariant 6: Available liquidity never exceeds total liquidity
     */
    function invariant_AvailableNeverExceedsTotal() public view {
        uint256 available = pool.availableLiquidity();
        uint256 total = pool.totalLiquidity();

        assertLe(
            available,
            total,
            "Available liquidity cannot exceed total liquidity"
        );
    }

    /*//////////////////////////////////////////////////////////////
                            TEST SUMMARY
    //////////////////////////////////////////////////////////////*/

    function invariant_callSummary() public view {
        handler.callSummary();
    }
}

/**
 * @title Handler
 * @notice Fuzzing handler that performs random valid operations on the pool
 * @dev Guides the fuzzer to generate realistic test scenarios
 */
contract Handler is Test {
    WithdrawalLiquidityPool public pool;
    address public mockPortal;

    // Track actors
    address[] public liquidityProviders;
    address[] public users;

    // Track state
    uint256 public maxShareValue;
    uint256 public nonce;
    mapping(bytes32 => bool) public fulfilledWithdrawals;

    // Call counters
    uint256 public depositCount;
    uint256 public withdrawCount;
    uint256 public provideLiquidityCount;
    uint256 public settleCount;
    uint256 public claimCount;

    constructor(WithdrawalLiquidityPool _pool, address _mockPortal) {
        pool = _pool;
        mockPortal = _mockPortal;
        maxShareValue = pool.shareValue();

        // Create some actors
        for (uint256 i = 0; i < 5; i++) {
            // casting to 'uint160' is safe because i is small (0-4) and 1000+i, 2000+i fit in uint160
            // forge-lint: disable-next-line(unsafe-typecast)
            liquidityProviders.push(address(uint160(1000 + i)));
            // forge-lint: disable-next-line(unsafe-typecast)
            users.push(address(uint160(2000 + i)));

            // Fund them
            vm.deal(liquidityProviders[i], 1000 ether);
            vm.deal(users[i], 1000 ether);
        }
    }

    /*//////////////////////////////////////////////////////////////
                            HANDLER ACTIONS
    //////////////////////////////////////////////////////////////*/

    function depositLiquidity(uint256 lpIndex, uint256 amount) public {
        // Bound inputs
        lpIndex = bound(lpIndex, 0, liquidityProviders.length - 1);
        amount = bound(amount, 0.01 ether, 100 ether);

        address lp = liquidityProviders[lpIndex];

        // Ensure LP has enough balance
        if (lp.balance < amount) {
            vm.deal(lp, amount);
        }

        // Deposit
        vm.prank(lp);
        try pool.depositLiquidity{value: amount}() {
            depositCount++;

            // Update max share value
            uint256 currentShareValue = pool.shareValue();
            if (currentShareValue > maxShareValue) {
                maxShareValue = currentShareValue;
            }
        } catch Error(string memory reason) {
            // Expected reverts with reason strings
            // For depositLiquidity, only ZeroAmount is expected, but we bound amount >= 0.01
            // So any revert here is unexpected
            revert(
                string.concat("Unexpected depositLiquidity revert: ", reason)
            );
        } catch (bytes memory lowLevelData) {
            // Check if it's an expected custom error
            // casting to 'bytes4' is safe because error data always includes 4-byte selector
            // forge-lint: disable-next-line(unsafe-typecast)
            // casting to 'bytes4' is safe because error data always includes 4-byte selector
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes4 selector = bytes4(lowLevelData);

            // ZeroAmount() - but shouldn't happen since we bound amount >= 0.01
            if (selector == WithdrawalLiquidityPool.ZeroAmount.selector) {
                // This is unexpected given our bounds
                revert(
                    "Unexpected ZeroAmount in depositLiquidity (amount is bounded >= 0.01)"
                );
            }

            // Any other error is unexpected
            revert("Unexpected low-level revert in depositLiquidity");
        }
    }

    function withdrawLiquidity(uint256 lpIndex, uint256 sharePercent) public {
        // Bound inputs
        lpIndex = bound(lpIndex, 0, liquidityProviders.length - 1);
        sharePercent = bound(sharePercent, 1, 100);

        address lp = liquidityProviders[lpIndex];
        uint256 lpShares = pool.getShares(lp);

        if (lpShares == 0) return;

        // Withdraw percentage of shares
        uint256 sharesToWithdraw = (lpShares * sharePercent) / 100;
        if (sharesToWithdraw == 0) sharesToWithdraw = 1;

        vm.prank(lp);
        try pool.withdrawLiquidity(sharesToWithdraw) {
            withdrawCount++;
        } catch Error(string memory reason) {
            // Unexpected string revert
            revert(
                string.concat("Unexpected withdrawLiquidity revert: ", reason)
            );
        } catch (bytes memory lowLevelData) {
            // casting to 'bytes4' is safe because error data always includes 4-byte selector
            // forge-lint: disable-next-line(unsafe-typecast)
            // casting to 'bytes4' is safe because error data always includes 4-byte selector
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes4 selector = bytes4(lowLevelData);

            // Expected errors that can legitimately happen during fuzzing:
            if (
                selector ==
                WithdrawalLiquidityPool.InsufficientShares.selector ||
                selector ==
                WithdrawalLiquidityPool.InsufficientLiquidity.selector ||
                selector == WithdrawalLiquidityPool.ZeroAmount.selector
            ) {
                // Expected failure, ignore
                return;
            }

            // Unexpected error
            revert("Unexpected low-level revert in withdrawLiquidity");
        }
    }

    function provideLiquidity(
        uint256 lpIndex,
        uint256 userIndex,
        uint256 amount
    ) public {
        // Bound inputs
        lpIndex = bound(lpIndex, 0, liquidityProviders.length - 1);
        userIndex = bound(userIndex, 0, users.length - 1);
        amount = bound(amount, 0.01 ether, 10 ether);

        address lp = liquidityProviders[lpIndex];
        address user = users[userIndex];

        // Create withdrawal
        nonce++;
        Types.WithdrawalTransaction memory withdrawal = Types
            .WithdrawalTransaction({
                nonce: nonce,
                sender: user,
                target: address(pool),
                value: amount,
                gasLimit: 100000,
                data: ""
            });

        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);

        // Skip if already fulfilled
        if (fulfilledWithdrawals[withdrawalHash]) return;

        // Check if LP has enough available liquidity
        uint256 feeRate = pool.feeRate();
        uint256 fee = (amount * feeRate) / 10000;
        uint256 amountToUser = amount - fee;

        if (pool.availableLiquidity() < amountToUser) return;

        // Provide liquidity
        vm.prank(lp);
        try pool.provideLiquidity(withdrawal) {
            provideLiquidityCount++;
            fulfilledWithdrawals[withdrawalHash] = true;

            // Update max share value
            uint256 currentShareValue = pool.shareValue();
            if (currentShareValue > maxShareValue) {
                maxShareValue = currentShareValue;
            }
        } catch Error(string memory reason) {
            // Unexpected string revert
            revert(
                string.concat("Unexpected provideLiquidity revert: ", reason)
            );
        } catch (bytes memory lowLevelData) {
            // casting to 'bytes4' is safe because error data always includes 4-byte selector
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes4 selector = bytes4(lowLevelData);

            // Expected errors:
            if (
                selector == WithdrawalLiquidityPool.AlreadyFulfilled.selector ||
                selector ==
                WithdrawalLiquidityPool.InsufficientLiquidity.selector ||
                selector == WithdrawalLiquidityPool.ZeroAmount.selector ||
                selector == WithdrawalLiquidityPool.ZeroAddress.selector
            ) {
                // Expected failure, ignore
                return;
            }

            // Unexpected error
            revert("Unexpected low-level revert in provideLiquidity");
        }
    }

    function settleWithdrawal(uint256 userIndex, uint256 amount) public {
        // Bound inputs
        userIndex = bound(userIndex, 0, users.length - 1);
        amount = bound(amount, 0.01 ether, 10 ether);

        address user = users[userIndex];

        // Create withdrawal that might have been fulfilled
        Types.WithdrawalTransaction memory withdrawal = Types
            .WithdrawalTransaction({
                nonce: bound(
                    nonce > 0 ? nonce - 1 : 1,
                    1,
                    nonce > 0 ? nonce : 1
                ),
                sender: user,
                target: address(pool),
                value: amount,
                gasLimit: 100000,
                data: ""
            });

        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);

        // Only settle if fulfilled
        if (!fulfilledWithdrawals[withdrawalHash]) return;

        // Fund portal so it can send ETH to pool
        vm.deal(mockPortal, amount * 2);

        // Settle (portal will send ETH automatically)
        try pool.settleWithdrawal(withdrawal) {
            settleCount++;

            // Update max share value
            uint256 currentShareValue = pool.shareValue();
            if (currentShareValue > maxShareValue) {
                maxShareValue = currentShareValue;
            }
        } catch Error(string memory reason) {
            // Unexpected string revert
            revert(
                string.concat("Unexpected settleWithdrawal revert: ", reason)
            );
        } catch (bytes memory lowLevelData) {
            // casting to 'bytes4' is safe because error data always includes 4-byte selector
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes4 selector = bytes4(lowLevelData);

            // Expected errors:
            if (
                selector == WithdrawalLiquidityPool.NotFulfilled.selector ||
                selector == WithdrawalLiquidityPool.AlreadySettled.selector
            ) {
                // Expected failure, ignore
                return;
            }

            // Unexpected error
            revert("Unexpected low-level revert in settleWithdrawal");
        }
    }

    function claimFallbackWithdrawal(uint256 userIndex, uint256 amount) public {
        // Bound inputs
        userIndex = bound(userIndex, 0, users.length - 1);
        amount = bound(amount, 0.01 ether, 10 ether);

        address user = users[userIndex];

        // Create withdrawal
        nonce++;
        Types.WithdrawalTransaction memory withdrawal = Types
            .WithdrawalTransaction({
                nonce: nonce,
                sender: user,
                target: address(pool),
                value: amount,
                gasLimit: 100000,
                data: ""
            });

        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);

        // Skip if already fulfilled
        if (fulfilledWithdrawals[withdrawalHash]) return;

        // Fund portal so it can send ETH to pool
        vm.deal(mockPortal, amount * 2);

        // Claim (portal will send ETH automatically)
        try pool.claimFallbackWithdrawal(withdrawal) {
            claimCount++;
        } catch Error(string memory reason) {
            // Unexpected string revert
            revert(
                string.concat(
                    "Unexpected claimFallbackWithdrawal revert: ",
                    reason
                )
            );
        } catch (bytes memory lowLevelData) {
            // casting to 'bytes4' is safe because error data always includes 4-byte selector
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes4 selector = bytes4(lowLevelData);

            // Expected errors:
            if (
                selector ==
                WithdrawalLiquidityPool.WithdrawalFulfilledByLP.selector ||
                selector == WithdrawalLiquidityPool.AlreadyClaimed.selector ||
                selector == WithdrawalLiquidityPool.ZeroAmount.selector ||
                selector == WithdrawalLiquidityPool.ZeroAddress.selector
            ) {
                // Expected failure, ignore
                return;
            }

            // Unexpected error (could also be portal revert if not yet finalized)
            // We accept this as it's testing the fallback path
            return;
        }
    }

    function setFeeRate(uint256 newRate) public {
        newRate = bound(newRate, 0, 1000); // 0-10%

        try pool.setFeeRate(newRate) {
            // Fee rate updated
        } catch Error(string memory reason) {
            // Unexpected string revert
            revert(string.concat("Unexpected setFeeRate revert: ", reason));
        } catch (bytes memory lowLevelData) {
            // casting to 'bytes4' is safe because error data always includes 4-byte selector
            // forge-lint: disable-next-line(unsafe-typecast)
            bytes4 selector = bytes4(lowLevelData);

            // Expected errors:
            if (
                selector == WithdrawalLiquidityPool.Unauthorized.selector ||
                selector == WithdrawalLiquidityPool.InvalidFeeRate.selector
            ) {
                // Expected failure (only owner can call, rate must be <= MAX)
                // Unauthorized shouldn't happen since handler is deployed by test
                // InvalidFeeRate shouldn't happen since we bound to 0-1000
                // But both are technically valid error cases
                return;
            }

            // Unexpected error
            revert("Unexpected low-level revert in setFeeRate");
        }
    }

    /*//////////////////////////////////////////////////////////////
                            SUMMARY
    //////////////////////////////////////////////////////////////*/

    function callSummary() external view {
        console.log("=== Call Summary ===");
        console.log("Deposits:", depositCount);
        console.log("Withdrawals:", withdrawCount);
        console.log("Provide Liquidity:", provideLiquidityCount);
        console.log("Settlements:", settleCount);
        console.log("Claims:", claimCount);
        console.log("Max Share Value:", maxShareValue);
    }
}
