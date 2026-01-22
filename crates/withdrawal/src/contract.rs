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
}
