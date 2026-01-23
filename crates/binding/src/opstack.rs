//! OP Stack contract bindings.
//!
//! Includes contracts for L2→L1 withdrawals:
//! - L2ToL1MessagePasser (L2 predeploy)
//! - OptimismPortal2 (L1 contract)
//! - DisputeGameFactory (L1 contract)

use alloy_sol_types::sol;

sol! {
    /// L2ToL1MessagePasser - L2 predeploy contract for initiating withdrawals
    /// Address: 0x4200000000000000000000000000000000000016 (on all OP Stack chains)
    #[sol(rpc)]
    interface IL2ToL1MessagePasser {
        /// Emitted when a withdrawal is initiated on L2
        event MessagePassed(
            uint256 indexed nonce,
            address indexed sender,
            address indexed target,
            uint256 value,
            uint256 gasLimit,
            bytes data,
            bytes32 withdrawalHash
        );

        /// Initiate a withdrawal from L2 to L1
        function initiateWithdrawal(
            address _target,
            uint256 _gasLimit,
            bytes calldata _data
        ) external payable;

        /// Check if a withdrawal message has been sent
        function sentMessages(bytes32) external view returns (bool);

        /// Get the current message nonce (with version encoded in top 2 bytes)
        function messageNonce() external view returns (uint256);
    }

    /// OptimismPortal2 - Main L1 contract for withdrawal proving and finalization
    #[sol(rpc)]
    interface IOptimismPortal2 {
        /// Proven withdrawal data stored on L1
        #[derive(Debug)]
        struct ProvenWithdrawal {
            address disputeGameProxy;
            uint64 timestamp;
        }

        /// Emitted when a withdrawal is proven on L1
        event WithdrawalProven(
            bytes32 indexed withdrawalHash,
            address indexed from,
            address indexed to
        );

        /// Emitted when a withdrawal is finalized on L1
        event WithdrawalFinalized(
            bytes32 indexed withdrawalHash,
            bool success
        );

        /// Query proven withdrawals by hash and proof submitter
        function provenWithdrawals(bytes32 withdrawalHash, address proofSubmitter)
            external view returns (ProvenWithdrawal memory);

        /// Query if a withdrawal has been finalized
        function finalizedWithdrawals(bytes32 withdrawalHash)
            external view returns (bool);

        /// Get the proof maturity delay (usually 7 days = 604800 seconds)
        function proofMaturityDelaySeconds()
            external view returns (uint256);

        /// Get the respected game type for filtering dispute games
        function respectedGameType()
            external view returns (uint32);

        /// Prove a withdrawal transaction (requires merkle proof)
        function proveWithdrawalTransaction(
            WithdrawalTransaction calldata _tx,
            uint256 _disputeGameIndex,
            OutputRootProof calldata _outputRootProof,
            bytes[] calldata _withdrawalProof
        ) external;

        /// Finalize a withdrawal transaction using external proof
        function finalizeWithdrawalTransactionExternalProof(
            WithdrawalTransaction calldata _tx,
            address _proofSubmitter
        ) external;
    }

    /// DisputeGameFactory - Used to find dispute games for proof generation
    #[sol(rpc)]
    interface IDisputeGameFactory {
        /// Dispute game search result
        struct GameSearchResult {
            uint256 index;
            bytes32 metadata;
            uint256 timestamp;
            bytes32 rootClaim;
            bytes extraData;
        }

        /// Get the total number of dispute games created
        function gameCount() external view returns (uint256 gameCount_);

        /// Find latest games of a given type
        function findLatestGames(
            uint32 _gameType,
            uint256 _start,
            uint256 _n
        ) external view returns (GameSearchResult[] memory);

        /// Get the address of a dispute game by index
        function gameAtIndex(uint256 _index) external view returns (address);
    }

    /// IFaultDisputeGame - Standard interface for fault dispute games
    #[sol(rpc)]
    interface IFaultDisputeGame {
        /// Get the L2 block number this game is disputing
        function l2BlockNumber() external view returns (uint256);

        /// Get the game status
        function status() external view returns (uint8);

        /// Get the root claim (output root)
        function rootClaim() external view returns (bytes32);
    }

    /// Output root proof structure (used in proving withdrawals)
    #[derive(Debug)]
    struct OutputRootProof {
        bytes32 version;
        bytes32 stateRoot;
        bytes32 messagePasserStorageRoot;
        bytes32 latestBlockhash;
    }

    /// Withdrawal transaction structure (shared across contracts)
    #[derive(Debug)]
    struct WithdrawalTransaction {
        uint256 nonce;
        address sender;
        address target;
        uint256 value;
        uint256 gasLimit;
        bytes data;
    }
}
