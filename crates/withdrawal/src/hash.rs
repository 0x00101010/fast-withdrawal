use crate::types::WithdrawalHash;
use alloy_primitives::keccak256;
use alloy_sol_types::SolValue;
use binding::opstack::WithdrawalTransaction;

pub fn compute_withdrawal_hash(tx: &WithdrawalTransaction) -> WithdrawalHash {
    // Solidity's Hashing.hashWithdrawal uses:
    // keccak256(abi.encode(_tx.nonce, _tx.sender, _tx.target, _tx.value, _tx.gasLimit, _tx.data))
    // We need to use abi_encode_sequence to encode the fields directly without a wrapper offset
    let encoded = (
        &tx.nonce,
        &tx.sender,
        &tx.target,
        &tx.value,
        &tx.gasLimit,
        &tx.data,
    )
        .abi_encode_sequence();

    keccak256(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{hex, Address, Bytes, B256, U256};

    #[test]
    fn test_compute_withdrawal_hash_deterministic() {
        // Create a withdrawal transaction
        let tx = WithdrawalTransaction {
            nonce: U256::from(1),
            sender: Address::from([0x01; 20]),
            target: Address::from([0x02; 20]),
            value: U256::from(1_000_000),
            gasLimit: U256::from(100_000),
            data: Bytes::from(vec![0xaa, 0xbb, 0xcc]),
        };

        // Compute hash twice
        let hash1 = compute_withdrawal_hash(&tx);
        let hash2 = compute_withdrawal_hash(&tx);

        // Should be deterministic
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, B256::ZERO);
    }

    #[test]
    fn test_compute_withdrawal_hash_known_value() {
        // Real withdrawal from Unichain Mainnet
        // TX: 0x91b374b5403401198a892f62db8843b60125cfb3e28ec1664089d9158424dc4a
        // Block: 23969114

        let tx = WithdrawalTransaction {
            nonce: U256::from_be_bytes(
                hex!("0001000000000000000000000000000000000000000000000000000000000818")
            ),
            sender: Address::from_slice(
                &hex!("000040D6c85A13a1AA74565FDe87e499dC023C6f")
            ),
            target: Address::from_slice(
                &hex!("B03eEF386A61b5b462051636001485FFfdD3d843")
            ),
            value: U256::ZERO,
            gasLimit: U256::from(200_000), // 0x30d40
            data: Bytes::from(hex!(
                "095ea7b3000000000000000000000000000040d6c85a13a1aa74565fde87e499dc023c6fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
            )),
        };

        let hash = compute_withdrawal_hash(&tx);

        // Expected hash from the MessagePassed event on chain
        let expected = B256::from_slice(&hex!(
            "49c43b60ec99e99046b54aec4c90419ff194300e567de63423c3b974ae46bd28"
        ));

        assert_eq!(hash, expected, "Hash mismatch!");
    }

    #[test]
    fn test_withdrawal_hash_collision_resistance() {
        // Test that similar but different transactions produce different hashes
        let base_tx = WithdrawalTransaction {
            nonce: U256::from(100),
            sender: Address::from([0x01; 20]),
            target: Address::from([0x02; 20]),
            value: U256::from(1_000_000),
            gasLimit: U256::from(100_000),
            data: Bytes::new(),
        };

        let mut hashes = std::collections::HashSet::new();

        // Generate 10 transactions with slightly different nonces
        for i in 100..110 {
            let tx = WithdrawalTransaction {
                nonce: U256::from(i),
                sender: base_tx.sender,
                target: base_tx.target,
                value: base_tx.value,
                gasLimit: base_tx.gasLimit,
                data: base_tx.data.clone(),
            };

            let hash = compute_withdrawal_hash(&tx);
            assert!(hashes.insert(hash), "Hash collision detected!");
        }

        assert_eq!(hashes.len(), 10);
    }
}
