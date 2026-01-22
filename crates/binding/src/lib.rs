//! Contract bindings for all external contracts.
//!
//! This crate consolidates all Solidity contract interfaces used across the project:
//! - Across Protocol contracts (SpokePool, HubPool)
//! - OP Stack contracts (OptimismPortal2, L2ToL1MessagePasser, DisputeGameFactory)
//! - ERC20 tokens
//!
//! All bindings are generated using alloy's `sol!` macro.

pub mod across;
pub mod opstack;
pub mod token;
