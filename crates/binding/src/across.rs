//! Across Protocol contract bindings.
//!
//! Includes contracts for cross-chain bridging:
//! - SpokePool (deposit and claim relayer refunds)
//! - HubPool (not currently used)

use alloy_sol_types::sol;

sol! {
    /// SpokePool - Main contract on each chain for deposits and claims
    #[sol(rpc)]
    interface SpokePool {
        /// Emitted when funds are deposited
        event FundsDeposited(
            uint256 amount,
            uint256 originChainId,
            uint256 indexed destinationChainId,
            int64 relayerFeePct,
            uint32 indexed depositId,
            uint32 quoteTimestamp,
            address originToken,
            address recipient,
            address indexed depositor,
            bytes message
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
}
