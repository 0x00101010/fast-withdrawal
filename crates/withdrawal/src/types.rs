use alloy_primitives::{Address, B256};

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
