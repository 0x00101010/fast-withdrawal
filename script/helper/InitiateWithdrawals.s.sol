// SPDX-License-Identifier: MIT
pragma solidity 0.8.15;

import {Script} from "forge-std/Script.sol";
import {console} from "forge-std/console.sol";

/**
 * @title InitiateWithdrawals
 * @notice Script to initiate withdrawals from L2 (Unichain Sepolia) to L1 (Sepolia)
 * @dev This script initiates 2 withdrawals:
 *      1. Withdrawal to self (withdrawal recipient = sender)
 *      2. Withdrawal to another address (withdrawal recipient = different address)
 *
 * Contract Details:
 *   - L2ToL1MessagePasser: 0x4200000000000000000000000000000000000016 (predeploy)
 *   - Function: initiateWithdrawal(address _target, uint256 _gasLimit, bytes memory _data)
 *   - Amount: 0.5 ETH per withdrawal (total 1.0 ETH needed)
 *
 * Usage:
 *   # Run script (uses .env.test and .env.test.secrets)
 *   source .env.test
 *   source .env.test.secrets
 *   forge script script/helper/InitiateWithdrawals.s.sol:InitiateWithdrawals \
 *     --rpc-url $L2_RPC_URL \
 *     --broadcast \
 *     --private-key $WITHDRAWAL_INITIATOR_PRIVATE_KEY
 *
 * After running:
 *   - Note the withdrawal transaction hashes
 *   - Wait for L2 transaction finalization
 *   - Extract withdrawal proof data from the transaction events
 *   - Use proof data for testing on Anvil fork
 */
contract InitiateWithdrawals is Script {
    // L2ToL1MessagePasser predeploy address (same for all OP Stack chains)
    address constant L2_TO_L1_MESSAGE_PASSER = 0x4200000000000000000000000000000000000016;

    // Withdrawal parameters
    uint256 constant WITHDRAWAL_AMOUNT = 0.5 ether;
    uint256 constant GAS_LIMIT = 100_000; // Standard gas limit for simple ETH transfer

    // Configuration (loaded from .env.test)
    address public withdrawalInitiator;
    address public otherRecipientAddress;

    function run() external {
        // Load configuration from .env.test
        loadConfig();

        console.log("\n=== Initiate Withdrawals from L2 ===\n");
        console.log("L2ToL1MessagePasser:", L2_TO_L1_MESSAGE_PASSER);
        console.log("Withdrawal Initiator:", withdrawalInitiator);
        console.log("Other Recipient:", otherRecipientAddress);
        console.log("Withdrawal Amount:", WITHDRAWAL_AMOUNT / 1 ether, "ETH per withdrawal");
        console.log("Total ETH needed:", (WITHDRAWAL_AMOUNT * 2) / 1 ether, "ETH");
        console.log("");

        // Check initiator balance
        uint256 initiatorBalance = withdrawalInitiator.balance;
        console.log("Initiator balance:", initiatorBalance / 1 ether, "ETH");
        require(initiatorBalance >= WITHDRAWAL_AMOUNT * 2, "Insufficient balance for 2 withdrawals");

        // Start broadcasting transactions
        vm.startBroadcast();

        // ============================================
        // Withdrawal 1: To self (initiator)
        // ============================================
        console.log("\n[1/2] Initiating withdrawal to self...");
        console.log("Target:", withdrawalInitiator);
        console.log("Amount:", WITHDRAWAL_AMOUNT / 1 ether, "ETH");

        (bool success1,) = L2_TO_L1_MESSAGE_PASSER.call{value: WITHDRAWAL_AMOUNT}(
            abi.encodeWithSignature("initiateWithdrawal(address,uint256,bytes)", withdrawalInitiator, GAS_LIMIT, "")
        );
        require(success1, "Withdrawal 1 failed");

        console.log("[OK] Withdrawal 1 initiated successfully");
        console.log("");

        // ============================================
        // Withdrawal 2: To other address
        // ============================================
        console.log("[2/2] Initiating withdrawal to other address...");
        console.log("Target:", otherRecipientAddress);
        console.log("Amount:", WITHDRAWAL_AMOUNT / 1 ether, "ETH");

        (bool success2,) = L2_TO_L1_MESSAGE_PASSER.call{value: WITHDRAWAL_AMOUNT}(
            abi.encodeWithSignature("initiateWithdrawal(address,uint256,bytes)", otherRecipientAddress, GAS_LIMIT, "")
        );
        require(success2, "Withdrawal 2 failed");

        console.log("[OK] Withdrawal 2 initiated successfully");
        console.log("");

        vm.stopBroadcast();

        // ============================================
        // Summary
        // ============================================
        console.log("=== Withdrawal Summary ===");
        console.log("Total withdrawals initiated: 2");
        console.log("Total ETH withdrawn:", (WITHDRAWAL_AMOUNT * 2) / 1 ether, "ETH");
        console.log("");
        console.log("Initiator balance after:", withdrawalInitiator.balance / 1 ether, "ETH");
        console.log("");

        console.log("=== Next Steps ===");
        console.log("1. Wait for L2 transaction finalization");
        console.log("2. Note the transaction hashes from the output above");
        console.log("3. Extract withdrawal proof data using:");
        console.log("   - View transaction on Unichain Sepolia explorer");
        console.log("   - Look for MessagePassed event");
        console.log("   - Save nonce, sender, target, value, gasLimit, data, withdrawalHash");
        console.log("4. Use this data in Anvil fork tests");
        console.log("5. After 7 days (or skip time in Anvil), finalize on L1");
    }

    /**
     * @notice Load configuration from environment variables (.env.test)
     */
    function loadConfig() internal {
        // Load withdrawal initiator address (the account initiating withdrawals)
        withdrawalInitiator = vm.envAddress("WITHDRAWAL_INITIATOR");

        // Load other recipient address (for second withdrawal)
        otherRecipientAddress = vm.envAddress("OTHER_RECIPIENT_ADDRESS");
    }
}
