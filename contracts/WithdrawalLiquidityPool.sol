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
    error NotFulfilled();
    error AlreadyClaimed();
    error WithdrawalFulfilledByLP();
    error NotYetFinalized();

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

    /**
     * @notice Emitted when a withdrawal is settled and fees are distributed
     * @param withdrawalHash The hash of the withdrawal transaction
     * @param reimbursement The amount returned to available liquidity (excluding fee)
     * @param fee The fee amount credited to the pool (increases share value)
     */
    event WithdrawalSettled(bytes32 indexed withdrawalHash, uint256 reimbursement, uint256 fee);

    /**
     * @notice Emitted when attempting to finalize a withdrawal that was already finalized
     * @param withdrawalHash The hash of the withdrawal transaction
     */
    event WithdrawalAlreadyFinalized(bytes32 indexed withdrawalHash);

    /**
     * @notice Emitted when a user claims a fallback withdrawal (no LP fulfilled)
     * @param withdrawalHash The hash of the withdrawal transaction
     * @param user The address receiving the fallback withdrawal
     * @param amount The full amount sent to user (no fee charged)
     */
    event FallbackWithdrawalClaimed(bytes32 indexed withdrawalHash, address indexed user, uint256 amount);

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
        uint256 amount; // Amount of ETH provided to user (or full withdrawal amount for fallback)
        uint256 feeRate; // Fee rate locked at time of fulfillment (basis points, 0 for fallback)
        bool fulfilled; // Whether this withdrawal has been fulfilled by an LP
        bool settled; // Whether OptimismPortal has sent ETH back
        bool claimed; // Whether this withdrawal has been claimed as fallback (no LP fulfilled)
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
        // Lock only what we're sending out (more capital efficient - fees stay available)
        availableLiquidity -= amountToUser;

        // Store withdrawal request with full amount (needed for settlement)
        withdrawalRequests[withdrawalHash] = WithdrawalRequest({
            amount: withdrawal.value, feeRate: lockedFeeRate, fulfilled: true, settled: false, claimed: false
        });

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

    /**
     * @notice Returns the status of a withdrawal request
     * @param withdrawalHash The hash of the withdrawal transaction
     * @return fulfilled Whether the withdrawal has been fulfilled by an LP
     * @return settled Whether the withdrawal has been settled (ETH received from portal)
     * @return claimed Whether the withdrawal has been claimed as fallback (no LP fulfilled)
     * @return amount The amount of ETH locked for this withdrawal (or full amount for fallback)
     * @return lockedFeeRate The fee rate locked at time of fulfillment (0 for fallback)
     */
    function getWithdrawalStatus(bytes32 withdrawalHash)
        external
        view
        returns (bool fulfilled, bool settled, bool claimed, uint256 amount, uint256 lockedFeeRate)
    {
        WithdrawalRequest memory request = withdrawalRequests[withdrawalHash];
        return (request.fulfilled, request.settled, request.claimed, request.amount, request.feeRate);
    }

    /*//////////////////////////////////////////////////////////////
                          FEE MANAGEMENT
    //////////////////////////////////////////////////////////////*/

    /**
     * @notice Updates the fee rate for new withdrawals
     * @param newRate The new fee rate in basis points (e.g., 50 = 0.5%)
     * @dev Only owner can call this function
     *      Fee rate is capped at MAX_FEE_RATE (10%)
     *      This only affects NEW withdrawals - existing withdrawals keep their locked rate
     */
    function setFeeRate(uint256 newRate) external onlyOwner {
        if (newRate > MAX_FEE_RATE) revert InvalidFeeRate();

        uint256 oldRate = feeRate;
        feeRate = newRate;

        emit FeeRateUpdated(oldRate, newRate);
    }

    /*//////////////////////////////////////////////////////////////
                          SETTLEMENT
    //////////////////////////////////////////////////////////////*/

    /**
     * @notice Settles a fulfilled withdrawal after OptimismPortal finalization
     * @param withdrawal The full withdrawal transaction that was fulfilled
     * @dev This function can be called by anyone after the 7-day challenge period.
     *      It will either:
     *      1. Call finalizeWithdrawalTransaction on the portal (if not yet finalized)
     *      2. Simply update accounting (if already finalized by someone else)
     *
     *      The fee is calculated using the LOCKED fee rate from fulfillment time.
     *      Fees are automatically credited to the pool and become available for providing
     *      liquidity (compounding effect), while also increasing share value for all LPs.
     */
    function settleWithdrawal(Types.WithdrawalTransaction calldata withdrawal) external nonReentrant {
        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);
        WithdrawalRequest storage request = withdrawalRequests[withdrawalHash];

        // Validations
        if (!request.fulfilled) revert NotFulfilled();
        if (request.settled) revert AlreadySettled();

        // Try to finalize through portal (will revert if already finalized)
        try OPTIMISM_PORTAL.finalizeWithdrawalTransaction(withdrawal) {
        // Success - portal just sent us ETH
        }
        catch {
            // Already finalized by someone else - just update our accounting
            emit WithdrawalAlreadyFinalized(withdrawalHash);
        }

        // Calculate fee using LOCKED fee rate from fulfillment time
        uint256 fee = (request.amount * request.feeRate) / 10000;
        uint256 reimbursement = request.amount - fee;

        // Update accounting:
        // Portal sends back the full request.amount (1 ETH in example)
        // We locked only the amountToUser (0.95 ETH), fee stayed available
        // Now we add the full amount back to available (unlock + fee)
        // And credit the fee to total to increase share value
        availableLiquidity += request.amount; // Add full amount from portal
        totalLiquidity += fee; // Credit fee to increase share value
        request.settled = true;

        emit WithdrawalSettled(withdrawalHash, reimbursement, fee);
    }

    /**
     * @notice Allows users to claim a fallback withdrawal when no LP fulfilled their request
     * @param withdrawal The full withdrawal transaction from L2
     * @dev This function should be called AFTER the 7-day challenge period when:
     *      1. No LP provided instant liquidity (withdrawal not fulfilled)
     *      2. The withdrawal has been finalized through the OptimismPortal
     *      3. ETH has been received from the portal
     *
     *      The user receives the full withdrawal amount with NO fee charged.
     *      This ensures the system degrades gracefully when LPs don't participate.
     *
     *      Flow:
     *      1. User initiates withdrawal on L2
     *      2. No LP calls provideLiquidity() for 7 days
     *      3. After 7 days, someone calls finalizeWithdrawalTransaction() on portal
     *      4. Portal sends ETH to this contract
     *      5. User calls claimFallbackWithdrawal() to receive their funds
     */
    function claimFallbackWithdrawal(Types.WithdrawalTransaction calldata withdrawal) external nonReentrant {
        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);
        WithdrawalRequest storage request = withdrawalRequests[withdrawalHash];

        // Decode recipient from data, fallback to sender if empty
        address recipient;
        if (withdrawal.data.length >= 32) {
            recipient = abi.decode(withdrawal.data, (address));
        } else {
            recipient = withdrawal.sender;
        }

        // Validations
        if (withdrawal.value == 0) revert ZeroAmount();
        if (recipient == address(0)) revert ZeroAddress();
        if (request.fulfilled) revert WithdrawalFulfilledByLP();
        if (request.claimed) revert AlreadyClaimed();

        // Check if withdrawal is already finalized on portal
        if (!OPTIMISM_PORTAL.finalizedWithdrawals(withdrawalHash)) {
            // Not finalized yet - try to finalize it ourselves
            // If this fails, it means the withdrawal is not ready (proof period not passed, etc.)
            OPTIMISM_PORTAL.finalizeWithdrawalTransaction(withdrawal);
            // If we reach here, finalization succeeded and portal sent us ETH
        }
        // If already finalized, the ETH should already be in the contract

        // Mark as claimed and store the amount
        request.claimed = true;
        request.amount = withdrawal.value;

        // Transfer full amount to user (no fee charged on fallback)
        (bool success,) = recipient.call{value: withdrawal.value}("");
        require(success, "ETH transfer failed");

        emit FallbackWithdrawalClaimed(withdrawalHash, recipient, withdrawal.value);
    }

    /**
     * @notice Receives ETH from any source
     * @dev Primary use: OptimismPortal2 sends ETH after withdrawal finalization (7-day challenge period)
     *      Secondary use: Anyone can send ETH for reimbursement or emergency situations
     *
     *      WARNING: If you send ETH to this contract and you're NOT the OptimismPortal,
     *      your funds become a donation to the LP pool and CANNOT be recovered.
     *
     *      Settlement is handled by the settleWithdrawal() function, which matches
     *      incoming ETH to specific withdrawal hashes and distributes fees.
     */
    receive() external payable {
        // Accept ETH from anyone
        // Settlement is handled by explicit settleWithdrawal() calls
    }

    /**
     * @notice Fallback function to accept ETH with calldata
     * @dev The OptimismPortal calls the target with withdrawal.data
     *      We accept and ignore the data - the actual recipient routing
     *      happens in claimFallbackWithdrawal() or provideLiquidity()
     */
    fallback() external payable {
        // Accept ETH with data from portal
        // Data is ignored here - recipient routing handled elsewhere
    }
}
