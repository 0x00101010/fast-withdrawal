// SPDX-License-Identifier: MIT
pragma solidity 0.8.15;

import {Script} from "forge-std/Script.sol";
import {console} from "forge-std/console.sol";
import {
    WithdrawalLiquidityPool
} from "../../contracts/WithdrawalLiquidityPool.sol";
import {ProxyAdmin} from "src/universal/ProxyAdmin.sol";
import {Proxy} from "src/universal/Proxy.sol";

// Optimism L1 contracts
import {
    IOptimismPortal2
} from "@eth-optimism-bedrock/interfaces/L1/IOptimismPortal2.sol";
import {
    IDisputeGameFactory
} from "@eth-optimism-bedrock/interfaces/dispute/IDisputeGameFactory.sol";
import {Types} from "src/libraries/Types.sol";
import {Hashing} from "src/libraries/Hashing.sol";

/**
 * @title Base
 * @notice Base contract for Anvil fork simulation scripts
 * @dev Provides utilities for interacting with forked Sepolia contracts:
 *      - Configuration loading from .env.test and .env.test.secrets
 *      - Connection to real OptimismPortal2 and DisputeGameFactory
 *      - Helper functions for creating withdrawals, time manipulation, logging
 *      - Account funding utilities
 *
 * Usage:
 *   All test scripts inherit from this contract to access shared utilities.
 */
contract Base is Script {
    // ============================================
    // Configuration (loaded from .env.test)
    // ============================================

    // L1 Contract Addresses (real Sepolia contracts we're forking)
    address payable public sepoliaOptimismPortal;
    address public sepoliaDisputeGameFactory;

    // Test Accounts
    address public testOwner;
    address public testLp1;
    address public testLp2;
    address public testUser1;
    address public testUser2;

    // Test Parameters
    uint256 public testWithdrawalAmount;
    uint256 public testLpDepositAmount;
    uint256 public testFeeRate;

    // Deployed Pool Contracts (set after running Setup script)
    address public poolProxyAddress;
    address public poolImplementationAddress;
    address public poolProxyAdminAddress;

    // RPC URLs (loaded from .env.test.secrets)
    string public sepoliaRpcUrl;
    string public anvilRpcUrl;

    // ============================================
    // Contract Interfaces
    // ============================================

    IOptimismPortal2 public portal;
    IDisputeGameFactory public disputeGameFactory;
    WithdrawalLiquidityPool public pool;
    ProxyAdmin public proxyAdmin;
    Proxy public proxy;

    // ============================================
    // Setup
    // ============================================

    /**
     * @notice Load configuration from environment files
     * @dev Call this in setUp() or run() of child contracts
     */
    function loadConfig() internal {
        // Load L1 contract addresses
        sepoliaOptimismPortal = payable(
            vm.envAddress("SEPOLIA_OPTIMISM_PORTAL")
        );
        sepoliaDisputeGameFactory = vm.envAddress(
            "SEPOLIA_DISPUTE_GAME_FACTORY"
        );

        // Load test accounts
        testOwner = vm.envAddress("TEST_OWNER");
        testLp1 = vm.envAddress("TEST_LP1");
        testLp2 = vm.envAddress("TEST_LP2");
        testUser1 = vm.envAddress("TEST_USER1");
        testUser2 = vm.envAddress("TEST_USER2");

        // Load test parameters
        testWithdrawalAmount = _parseEther(
            vm.envString("TEST_WITHDRAWAL_AMOUNT")
        );
        testLpDepositAmount = _parseEther(
            vm.envString("TEST_LP_DEPOSIT_AMOUNT")
        );
        testFeeRate = vm.envUint("TEST_FEE_RATE");

        // Load deployed pool addresses (may be empty before Setup script runs)
        poolProxyAddress = vm.envOr("POOL_PROXY_ADDRESS", address(0));
        poolImplementationAddress = vm.envOr(
            "POOL_IMPLEMENTATION_ADDRESS",
            address(0)
        );
        poolProxyAdminAddress = vm.envOr(
            "POOL_PROXY_ADMIN_ADDRESS",
            address(0)
        );

        // Load RPC URLs
        sepoliaRpcUrl = vm.envString("SEPOLIA_L1_URL");
        anvilRpcUrl = vm.envString("ANVIL_RPC_URL");

        // Connect to L1 contracts
        portal = IOptimismPortal2(sepoliaOptimismPortal);
        disputeGameFactory = IDisputeGameFactory(sepoliaDisputeGameFactory);

        // Connect to pool contracts if deployed
        if (poolProxyAddress != address(0)) {
            pool = WithdrawalLiquidityPool(payable(poolProxyAddress));
            proxy = Proxy(payable(poolProxyAddress));
        }
        if (poolProxyAdminAddress != address(0)) {
            proxyAdmin = ProxyAdmin(poolProxyAdminAddress);
        }
    }

    // ============================================
    // Withdrawal Creation Helpers
    // ============================================

    /**
     * @notice Create a mock L2 withdrawal transaction
     * @dev This simulates a withdrawal initiated on L2 without actually interacting with L2
     * @param nonce Withdrawal nonce
     * @param sender L2 sender address
     * @param target L1 target address (usually the pool)
     * @param value Amount of ETH being withdrawn
     * @param gasLimit Gas limit for L1 execution
     * @param data Calldata for L1 execution
     * @return withdrawal The withdrawal transaction struct
     */
    function createWithdrawal(
        uint256 nonce,
        address sender,
        address target,
        uint256 value,
        uint64 gasLimit,
        bytes memory data
    ) public pure returns (Types.WithdrawalTransaction memory withdrawal) {
        withdrawal = Types.WithdrawalTransaction({
            nonce: nonce,
            sender: sender,
            target: target,
            value: value,
            gasLimit: gasLimit,
            data: data
        });
    }

    /**
     * @notice Create a simple ETH withdrawal (most common case)
     * @param nonce Withdrawal nonce
     * @param sender L2 sender address
     * @param recipient L1 recipient address
     * @param value Amount of ETH
     * @return withdrawal The withdrawal transaction struct
     */
    function createSimpleWithdrawal(
        uint256 nonce,
        address sender,
        address recipient,
        uint256 value
    ) public pure returns (Types.WithdrawalTransaction memory withdrawal) {
        return
            createWithdrawal(
                nonce,
                sender,
                recipient,
                value,
                100_000, // Standard gas limit for ETH transfer
                "" // No calldata for simple ETH transfer
            );
    }

    /**
     * @notice Compute withdrawal hash
     * @param withdrawal The withdrawal transaction
     * @return Hash of the withdrawal
     */
    function hashWithdrawal(
        Types.WithdrawalTransaction memory withdrawal
    ) public pure returns (bytes32) {
        return Hashing.hashWithdrawal(withdrawal);
    }

    // ============================================
    // Time Manipulation
    // ============================================

    /**
     * @notice Advance blockchain time by a duration
     * @param duration Seconds to advance
     */
    function advanceTime(uint256 duration) public {
        vm.warp(block.timestamp + duration);
        console.log("Advanced time by (seconds):", duration);
        console.log("Current timestamp:", block.timestamp);
    }

    /**
     * @notice Advance to a specific timestamp
     * @param timestamp Target timestamp
     */
    function advanceToTimestamp(uint256 timestamp) public {
        require(
            timestamp > block.timestamp,
            "Target timestamp must be in the future"
        );
        vm.warp(timestamp);
        console.log("Advanced to timestamp:", timestamp);
    }

    /**
     * @notice Skip the 7-day challenge period
     */
    function skipChallengePeriod() public {
        uint256 challengePeriod = 7 days;
        advanceTime(challengePeriod + 1); // Add 1 second to ensure we're past the period
        console.log("Skipped 7-day challenge period");
    }

    // ============================================
    // Account Funding
    // ============================================

    /**
     * @notice Fund an account with ETH
     * @param account Address to fund
     * @param amount Amount of ETH (in wei)
     */
    function fundAccount(address account, uint256 amount) public {
        vm.deal(account, amount);
        console.log("Funded account:", account);
        console.log("Amount (wei):", amount);
    }

    /**
     * @notice Fund all test accounts with default amounts
     */
    function fundAllTestAccounts() public {
        fundAccount(testOwner, 100 ether);
        fundAccount(testLp1, 100 ether);
        fundAccount(testLp2, 100 ether);
        fundAccount(testUser1, 10 ether);
        fundAccount(testUser2, 10 ether);
        console.log("Funded all test accounts");
    }

    // ============================================
    // Logging Utilities
    // ============================================

    /**
     * @notice Log balances of all test accounts
     */
    function logBalances() public view {
        console.log("\n=== Account Balances ===");
        console.log("Owner:  ", testOwner.balance);
        console.log("LP1:    ", testLp1.balance);
        console.log("LP2:    ", testLp2.balance);
        console.log("User1:  ", testUser1.balance);
        console.log("User2:  ", testUser2.balance);
        if (address(pool) != address(0)) {
            console.log("Pool:   ", address(pool).balance);
        }
    }

    /**
     * @notice Log pool state
     */
    function logPoolState() public view {
        require(address(pool) != address(0), "Pool not deployed");
        console.log("\n=== Pool State ===");
        console.log("Address:        ", address(pool));
        console.log("Owner:          ", pool.owner());
        console.log("OptimismPortal: ", address(pool.optimismPortal()));
        console.log("Fee Rate (bps): ", pool.feeRate());
        console.log("Start Block:    ", pool.startBlock());
        console.log("Balance:        ", address(pool).balance);
        console.log("Total Liquidity:", pool.totalLiquidity());
    }

    /**
     * @notice Log withdrawal status
     * @param withdrawalHash Hash of the withdrawal
     */
    function logWithdrawalStatus(bytes32 withdrawalHash) public view {
        require(address(pool) != address(0), "Pool not deployed");
        console.log("\n=== Withdrawal Status ===");
        console.log("Hash:", vm.toString(withdrawalHash));

        // Check withdrawal request status
        (
            uint256 amount,
            uint256 feeRateLocked,
            bool fulfilled,
            bool settled,
            bool claimed
        ) = pool.withdrawalRequests(withdrawalHash);
        console.log("Amount:    ", amount);
        console.log("Fee Rate:  ", feeRateLocked);
        console.log("Fulfilled: ", fulfilled);
        console.log("Settled:   ", settled);
        console.log("Claimed:   ", claimed);

        // Check if withdrawal is finalized on portal
        bool finalized = portal.finalizedWithdrawals(withdrawalHash);
        console.log("Finalized on Portal:", finalized);
    }

    // ============================================
    // Utility Functions
    // ============================================

    /**
     * @notice Parse ether string to wei (e.g., "0.01" -> 10000000000000000)
     * @param amountStr Amount in ether as string
     * @return Amount in wei
     */
    function _parseEther(
        string memory amountStr
    ) internal pure returns (uint256) {
        bytes memory amountBytes = bytes(amountStr);
        uint256 integerPart = 0;
        uint256 fractionalPart = 0;
        uint256 fractionalDigits = 0;
        bool parsingFractional = false;

        for (uint256 i = 0; i < amountBytes.length; i++) {
            bytes1 char = amountBytes[i];

            if (char == ".") {
                parsingFractional = true;
                continue;
            }

            require(char >= "0" && char <= "9", "Invalid number format");
            uint256 digit = uint256(uint8(char)) - 48; // ASCII '0' = 48

            if (parsingFractional) {
                fractionalPart = fractionalPart * 10 + digit;
                fractionalDigits++;
            } else {
                integerPart = integerPart * 10 + digit;
            }
        }

        // Convert to wei (1 ether = 10^18 wei)
        uint256 result = integerPart * 1 ether;
        if (fractionalDigits > 0) {
            result += fractionalPart * (10 ** (18 - fractionalDigits));
        }

        return result;
    }
}
