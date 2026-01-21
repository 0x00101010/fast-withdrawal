use alloy_primitives::{Address, Bytes, B256, U256};

/// WithdrawalTransaction represents an onchain withdrawal.
pub struct WithdrawalTransaction {
    pub nonce: U256,
    pub sender: Address,
    pub target: Address,
    pub value: U256,
    pub gas_limit: U256,
    pub data: Bytes,
}

pub type WithdrawalHash = B256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WithdrawalStatus {
    Initiated,
    Proven { timestamp: u64 },
    Finalized,
}

pub struct ProvenWithdrawal {
    pub dispute_game_proxy: Address,
    pub timestamp: u64,
}