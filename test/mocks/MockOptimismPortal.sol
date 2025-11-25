// SPDX-License-Identifier: MIT
pragma solidity 0.8.15;

import {Types} from "src/libraries/Types.sol";
import {Hashing} from "src/libraries/Hashing.sol";

/**
 * @title MockOptimismPortal
 * @notice Mock OptimismPortal for testing
 * @dev Mimics real portal behavior: calls target with value, gasLimit, and data
 */
contract MockOptimismPortal {
    mapping(bytes32 => bool) public finalizedWithdrawals;
    bool public shouldRevert;

    function setShouldRevert(bool _shouldRevert) external {
        shouldRevert = _shouldRevert;
    }

    function finalizeWithdrawalTransaction(Types.WithdrawalTransaction calldata withdrawal) external payable {
        if (shouldRevert) {
            revert("MockPortal: Simulated revert");
        }

        bytes32 withdrawalHash = Hashing.hashWithdrawal(withdrawal);
        finalizedWithdrawals[withdrawalHash] = true;

        // Mimic real portal behavior: call target with value and gasLimit
        // Note: The real portal passes withdrawal.data to the target call
        // The target contract processes the data (e.g., routing to custom recipient)
        (bool success,) = withdrawal.target.call{value: withdrawal.value, gas: withdrawal.gasLimit}(withdrawal.data);
        require(success, "Target call failed");
    }

    // Allow funding the portal
    receive() external payable {}
}
