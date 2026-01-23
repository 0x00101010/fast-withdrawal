use alloy_primitives::B256;

pub type WithdrawalHash = B256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WithdrawalStatus {
    Initiated,
    Proven { timestamp: u64 },
    Finalized,
}
