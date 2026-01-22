use alloy_sol_types::sol;

sol! {
    #[derive(Debug)]
    struct WithdrawalTransaction {
        uint256 nonce;
        address sender;
        address target;
        uint256 value;
        uint256 gasLimit;
        bytes data;
    }

    #[sol(rpc)]
    interface OptimismPortal2 {
        struct ProvenWithdrawal {
            address disputeGameProxy;
            uint64 timestamp;
        }

        /// Emitted when a withdrawal is proven
        event WithdrawalProven(
            bytes32 indexed withdrawalHash,
            address indexed from,
            address indexed to
        );

        event WithdrawalFinalized(
            bytes32 indexed withdrawalHash,
            bool success
        );

        function provenWithdrawals(
            bytes32 withdrawalHash,
            address proofSubmitter
        ) external view returns (ProvenWithdrawal memory);

        function finalizedWithdrawals(bytes32 withdrawalHash) external view returns (bool);

        /// Get the proof maturity delay (usually 7 days = 604800 seconds)
        function proofMaturityDelaySeconds() external view returns (uint256);

        /// Get the respected game type for filtering dispute games
        function respectedGameType() external view returns (uint32);
    }

    /// DisputeGameFactory - Used to find dispute games for proof generation
    #[sol(rpc)]
    interface DisputeGameFactory {
        /// Dispute game search result
        struct GameSearchResult {
            uint256 index;
            bytes32 metadata;
            uint64 timestamp;
            bytes32 rootClaim;
            bytes extraData;
        }

        function findLatestGames(
            uint32 _gameType,
            uint256 _start,
            uint256 _n
        ) external view returns (GameSearchResult[] memory);
    }
}
