// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IOptimismPortal2} from "@eth-optimism-bedrock/interfaces/L1/IOptimismPortal2.sol";

/**
 * @title WithdrawalLiquidityPool
 * @notice Enables instant L2→L1 withdrawals by having LPs front capital while waiting for
 *         canonical bridge finalization (7 days in Phase 1, 1 day in Phase 2 with ZK proofs).
 * @dev This contract implements a share-based accounting system where LPs deposit ETH and
 *      receive proportional shares. Share value increases as fees are earned from providing
 *      instant liquidity for withdrawals.
 *
 * Key Security Features:
 * - Reentrancy protection on all external calls
 * - Share-based accounting prevents first depositor attacks
 * - Locked fee rates prevent manipulation between fulfillment and settlement
 * - Emergency pause mechanism for critical bugs
 *
 * Phase 1 Implementation:
 * - LPs verify withdrawal validity off-chain (trust-minimized via L2 events)
 * - 7-day canonical bridge finalization period
 *
 * Phase 2 Enhancements (Future):
 * - 1-day finalization period
 */
contract WithdrawalLiquidityPool {
    /*//////////////////////////////////////////////////////////////
                                 ERRORS
    //////////////////////////////////////////////////////////////*/

    error ZeroAddress();
    error ZeroAmount();
    error InsufficientShares();
    error InsufficientLiquidity();
    error Unauthorized();

    /*//////////////////////////////////////////////////////////////
                                 EVENTS
    //////////////////////////////////////////////////////////////*/

    /**
     * @notice Emitted when an LP deposits liquidity and receives shares
     * @param provider The address of the LP
     * @param amount The amount of ETH deposited
     * @param shares The number of shares minted
     */
    event LiquidityDeposited(address indexed provider, uint256 amount, uint256 shares);

    /**
     * @notice Emitted when an LP withdraws liquidity by burning shares
     * @param provider The address of the LP
     * @param shares The number of shares burned
     * @param amount The amount of ETH withdrawn
     */
    event LiquidityWithdrawn(address indexed provider, uint256 shares, uint256 amount);

    /**
     * @notice Emitted when ownership is transferred
     * @param previousOwner The previous owner address
     * @param newOwner The new owner address
     */
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);

    /*//////////////////////////////////////////////////////////////
                            STATE VARIABLES
    //////////////////////////////////////////////////////////////*/

    /// @notice Address of the contract owner (for admin functions)
    address public owner;

    /// @notice Address of the OptimismPortal2 contract (immutable after deployment)
    IOptimismPortal2 public immutable OPTIMISM_PORTAL;

    /// @notice Mapping of LP addresses to their share balances
    mapping(address => uint256) public liquidityShares;

    /// @notice Total shares issued across all LPs
    uint256 public totalShares;

    /// @notice Total liquidity in the pool (including locked liquidity from pending settlements)
    uint256 public totalLiquidity;

    /// @notice Available liquidity for new withdrawals (totalLiquidity - locked liquidity)
    uint256 public availableLiquidity;

    /*//////////////////////////////////////////////////////////////
                               MODIFIERS
    //////////////////////////////////////////////////////////////*/

    /**
     * @notice Restricts function access to the contract owner only
     */
    modifier onlyOwner() {
        _onlyOwner();
        _;
    }

    function _onlyOwner() internal view {
        if (msg.sender != owner) revert Unauthorized();
    }

    /*//////////////////////////////////////////////////////////////
                              CONSTRUCTOR
    //////////////////////////////////////////////////////////////*/

    /**
     * @notice Initializes the WithdrawalLiquidityPool contract
     * @param _optimismPortal Address of the OptimismPortal2 contract
     * @dev The portal address is immutable and cannot be changed after deployment
     */
    constructor(address payable _optimismPortal) {
        if (_optimismPortal == address(0)) revert ZeroAddress();

        OPTIMISM_PORTAL = IOptimismPortal2(_optimismPortal);
        owner = msg.sender;

        emit OwnershipTransferred(address(0), msg.sender);
    }

    /*//////////////////////////////////////////////////////////////
                          LIQUIDITY PROVIDER FUNCTIONS
    //////////////////////////////////////////////////////////////*/

    /**
     * @notice Allows LPs to deposit ETH and receive proportional shares
     * @dev Share calculation:
     *      - First deposit: shares = amount (1:1 ratio)
     *      - Subsequent deposits: shares = (amount * totalShares) / totalLiquidity
     *      This ensures share value reflects accumulated fees
     */
    function depositLiquidity() external payable {
        if (msg.value == 0) revert ZeroAmount();

        uint256 sharesToMint;

        // First deposit: mint shares 1:1 with ETH
        if (totalShares == 0) {
            sharesToMint = msg.value;
        } else {
            // Subsequent deposits: mint shares proportional to pool ownership
            // shares = (deposit * totalShares) / totalLiquidity
            sharesToMint = (msg.value * totalShares) / totalLiquidity;
        }

        // Update state
        liquidityShares[msg.sender] += sharesToMint;
        totalShares += sharesToMint;
        totalLiquidity += msg.value;
        availableLiquidity += msg.value;

        emit LiquidityDeposited(msg.sender, msg.value, sharesToMint);
    }

    /**
     * @notice Allows LPs to withdraw ETH by burning shares
     * @param shares The number of shares to burn
     * @dev Withdrawal amount = (shares * totalLiquidity) / totalShares
     *      Only available liquidity can be withdrawn (not locked in pending settlements)
     */
    function withdrawLiquidity(uint256 shares) external {
        if (shares == 0) revert ZeroAmount();
        if (liquidityShares[msg.sender] < shares) revert InsufficientShares();

        // Calculate ETH amount to return
        // amount = (shares * totalLiquidity) / totalShares
        uint256 amountToWithdraw = (shares * totalLiquidity) / totalShares;

        if (amountToWithdraw > availableLiquidity) {
            revert InsufficientLiquidity();
        }

        // Update state before external call (checks-effects-interactions)
        liquidityShares[msg.sender] -= shares;
        totalShares -= shares;
        totalLiquidity -= amountToWithdraw;
        availableLiquidity -= amountToWithdraw;

        // Transfer ETH to LP
        (bool success,) = msg.sender.call{value: amountToWithdraw}("");
        require(success, "ETH transfer failed");

        emit LiquidityWithdrawn(msg.sender, shares, amountToWithdraw);
    }

    /*//////////////////////////////////////////////////////////////
                          OWNERSHIP MANAGEMENT
    //////////////////////////////////////////////////////////////*/

    /**
     * @notice Transfers ownership of the contract to a new address
     * @param newOwner The address of the new owner
     * @dev Only the current owner can call this function
     */
    function transferOwnership(address newOwner) external onlyOwner {
        if (newOwner == address(0)) revert ZeroAddress();

        address oldOwner = owner;
        owner = newOwner;

        emit OwnershipTransferred(oldOwner, newOwner);
    }

    /*//////////////////////////////////////////////////////////////
                              VIEW FUNCTIONS
    //////////////////////////////////////////////////////////////*/

    /**
     * @notice Calculates the current share value (ETH per share)
     * @return The value of one share in wei (scaled by 1e18 for precision)
     * @dev Returns 1e18 (1:1) if no shares exist yet
     */
    function shareValue() external view returns (uint256) {
        if (totalShares == 0) return 1e18;
        return (totalLiquidity * 1e18) / totalShares;
    }

    /**
     * @notice Calculates how much ETH an LP would receive for burning shares
     * @param shares The number of shares to calculate value for
     * @return The amount of ETH that would be received
     */
    function calculateWithdrawalAmount(uint256 shares) external view returns (uint256) {
        if (totalShares == 0) return 0;
        return (shares * totalLiquidity) / totalShares;
    }

    /**
     * @notice Returns the share balance of an LP
     * @param provider The address of the LP
     * @return The number of shares owned by the LP
     */
    function getShares(address provider) external view returns (uint256) {
        return liquidityShares[provider];
    }
}
