//! Deposit tracking for Across Protocol.
//!
//! This crate provides functionality to track in-flight deposits from L1 to L2
//! via the Across Protocol. It queries on-chain events to determine which deposits
//! have been initiated but not yet filled.

pub mod state;

pub use state::{
    get_inflight_deposit_total, get_inflight_deposits, DepositStateProvider, InFlightDeposit,
};
