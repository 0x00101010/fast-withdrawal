//! Across Protocol contract bindings.
//!
//! Includes contracts for cross-chain bridging:
//! - SpokePool (deposit and claim relayer refunds)
//! - HubPool (not currently used)

use alloy_sol_types::sol;

sol! {
    /// SpokePool - Main contract on each chain for deposits and claims
    #[sol(rpc)]
    #[allow(clippy::too_many_arguments)]
    interface ISpokePool {
        /// Emitted when funds are deposited (V3 current format with bytes32)
        /// See: https://github.com/across-protocol/contracts/blob/master/contracts/interfaces/V3SpokePoolInterface.sol
        event FundsDeposited(
            bytes32 inputToken,
            bytes32 outputToken,
            uint256 inputAmount,
            uint256 outputAmount,
            uint256 indexed destinationChainId,
            uint256 indexed depositId,
            uint32 quoteTimestamp,
            uint32 fillDeadline,
            uint32 exclusivityDeadline,
            bytes32 indexed depositor,
            bytes32 recipient,
            bytes32 exclusiveRelayer,
            bytes message
        );

        /// Emitted when a relay is filled on the destination chain
        event FilledRelay(
            bytes32 inputToken,
            bytes32 outputToken,
            uint256 inputAmount,
            uint256 outputAmount,
            uint256 repaymentChainId,
            uint256 indexed originChainId,
            uint256 indexed depositId,
            uint32 fillDeadline,
            uint32 exclusivityDeadline,
            bytes32 exclusiveRelayer,
            bytes32 indexed relayer,
            bytes32 depositor,
            bytes32 recipient,
            bytes32 messageHash,
            V3RelayExecutionEventInfo relayExecutionInfo
        );

        /// Emitted when a relayer refund is claimed
        event ClaimedRelayerRefund(
            address indexed token,
            address indexed relayer,
            uint256 amount
        );

        /// Deposit V3 function
        function depositV3(
            address depositor,
            address recipient,
            address inputToken,
            address outputToken,
            uint256 inputAmount,
            uint256 outputAmount,
            uint256 destinationChainId,
            address exclusiveRelayer,
            uint32 quoteTimestamp,
            uint32 fillDeadline,
            uint32 exclusivityDeadline,
            bytes calldata message
        ) external payable;

        /// Query relayer refund amount for a given token
        function getRelayerRefund(address token, address relayer)
            external view returns (uint256);

        /// Claim relayer refund
        function claimRelayerRefund(address token) external;
    }

    /// Fill type for relay execution
    enum FillType {
        FastFill,
        ReplacedSlowFill,
        SlowFill
    }

    /// Relay execution event info
    struct V3RelayExecutionEventInfo {
        bytes32 updatedRecipient;
        bytes32 updatedMessageHash;
        uint256 updatedOutputAmount;
        FillType fillType;
    }
}
