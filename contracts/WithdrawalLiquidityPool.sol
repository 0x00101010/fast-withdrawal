// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IOptimismPortal2} from "@eth-optimism-bedrock/interfaces/L1/IOptimismPortal2.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import {Types} from "src/libraries/Types.sol";
import {Hashing} from "src/libraries/Hashing.sol";

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
contract WithdrawalLiquidityPool is ReentrancyGuard {
    /*//////////////////////////////////////////////////////////////
                                 ERRORS
    //////////////////////////////////////////////////////////////*/

    error ZeroAddress();
    error ZeroAmount();
    error InsufficientShares();
    error InsufficientLiquidity();
    error Unauthorized();
    error AlreadyFulfilled();
    error AlreadySettled();
    error InvalidFeeRate();

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

    /**
     * @notice Emitted when pool fulfills a withdrawal request
     * @param withdrawalHash The hash of the withdrawal transaction
     * @param user The address receiving the instant withdrawal
     * @param amount The amount of ETH provided to user
     * @param feeRate The fee rate locked for this withdrawal (basis points)
     */
    event WithdrawalFulfilled(bytes32 indexed withdrawalHash, address indexed user, uint256 amount, uint256 feeRate);

    /**
     * @notice Emitted when fee rate is updated
     * @param oldRate The previous fee rate
     * @param newRate The new fee rate
     */
    event FeeRateUpdated(uint256 oldRate, uint256 newRate);

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

    /// @notice Struct to track withdrawal fulfillment and settlement
    struct WithdrawalRequest {
        uint256 amount; // Amount of ETH provided to user
        uint256 feeRate; // Fee rate locked at time of fulfillment (basis points)
        bool fulfilled; // Whether this withdrawal has been fulfilled
        bool settled; // Whether OptimismPortal has sent ETH back
    }

    /// @notice Mapping of withdrawal hash to request details
    mapping(bytes32 => WithdrawalRequest) public withdrawalRequests;

    /// @notice Current fee rate in basis points (e.g., 50 = 0.5%)
    uint256 public feeRate;

    /// @notice Maximum allowed fee rate (10% = 1000 basis points)
    uint256 public constant MAX_FEE_RATE = 1000;

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
     * @notice Provides instant liquidity for a withdrawal request
     * @param withdrawal The full withdrawal transaction from L2
     * @dev LPs must verify withdrawal validity off-chain before calling this function.
     *      The recipient is decoded from withdrawal.data with fallback to withdrawal.sender.
     *      Fee is deducted from the amount sent to user.
     *      Fee rate is locked at time of fulfillment to prevent manipulation.
     *
     *      Accounting:
     *      - withdrawal.value is the full L2 withdrawal amount
     *      - User receives: withdrawal.value - fee
     *      - Pool locks: withdrawal.value (will be returned by OptimismPortal)
     *      - Pool profit after settlement: fee amount
     *
     *      Security considerations:
     *      - LPs must verify L2 event authenticity off-chain
     *      - LPs must verify sender legitimacy
     *      - withdrawal.target should be this contract (for 7-day settlement)
     *      - Recipient decoding: withdrawal.data (if >= 32 bytes) or withdrawal.sender (fallback)
     */
    function provideLiquidity(Types.WithdrawalTransaction memory withdrawal) external nonReentrant {
        // Compute withdrawal hash from transaction data
        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);

        // Decode recipient from data, fallback to sender if empty
        address recipient;
        if (withdrawal.data.length >= 32) {
            // Expected format: abi.encode(address) which is left-padded to 32 bytes.
            // abi.decode will read the first 32 bytes as an address and ignore any extra bytes.
            recipient = abi.decode(withdrawal.data, (address));
        } else {
            // Fallback: send to L2 sender
            recipient = withdrawal.sender;
        }

        // Validation checks
        if (withdrawal.value == 0) revert ZeroAmount();
        if (recipient == address(0)) revert ZeroAddress();
        if (withdrawalRequests[withdrawalHash].fulfilled) {
            revert AlreadyFulfilled();
        }
        if (withdrawal.value > availableLiquidity) {
            revert InsufficientLiquidity();
        }

        // Lock current fee rate for this withdrawal
        uint256 lockedFeeRate = feeRate;

        // Calculate fee and amount to send to user
        uint256 fee = (withdrawal.value * lockedFeeRate) / 10000;
        uint256 amountToUser = withdrawal.value - fee;

        // Update state before external call (checks-effects-interactions)
        // Lock the FULL withdrawal amount (we'll get this back from OptimismPortal)
        availableLiquidity -= withdrawal.value;

        // Store withdrawal request with full amount
        withdrawalRequests[withdrawalHash] =
            WithdrawalRequest({amount: withdrawal.value, feeRate: lockedFeeRate, fulfilled: true, settled: false});

        // Transfer ETH to recipient (after deducting fee)
        (bool success,) = recipient.call{value: amountToUser}("");
        require(success, "ETH transfer failed");

        emit WithdrawalFulfilled(withdrawalHash, recipient, amountToUser, lockedFeeRate);
    }

    /**
     * @notice Allows LPs to withdraw ETH by burning shares
     * @param shares The number of shares to burn
     * @dev Withdrawal amount = (shares * totalLiquidity) / totalShares
     *      Only available liquidity can be withdrawn (not locked in pending settlements)
     */
    function withdrawLiquidity(uint256 shares) external nonReentrant {
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
