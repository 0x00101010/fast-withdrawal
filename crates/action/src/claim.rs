use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use binding::across::ISpokePool;

/// Input for a claim action.
#[derive(Debug, Clone)]
pub struct Claim {
    /// ISpokePool contract address
    pub spoke_pool: Address,
    /// Token address to claim
    pub token: Address,
    /// Address to send claimed tokens to
    pub refund_address: Address,
    /// Relayer address (msg.sender) - must match the account signing the transaction.
    pub relayer: Address,
}

/// Claim action for claiming relayer refunds from ISpokePool.
pub struct ClaimAction<P> {
    provider: P,
    claim: Claim,
}

impl<P> ClaimAction<P>
where
    P: Provider + Clone,
{
    pub const fn new(provider: P, claim: Claim) -> Self {
        Self { provider, claim }
    }

    fn validate_claim(&self) -> eyre::Result<()> {
        if self.claim.spoke_pool == Address::ZERO {
            eyre::bail!("Spoke pool must not be zero");
        }

        if self.claim.token == Address::ZERO {
            eyre::bail!("Token must not be zero");
        }

        if self.claim.refund_address == Address::ZERO {
            eyre::bail!("Refund address must not be zero");
        }

        if self.claim.relayer == Address::ZERO {
            eyre::bail!("Relayer must not be zero");
        }

        Ok(())
    }

    /// Query the claimable balance for the relayer.
    pub async fn get_claimable_balance(&self) -> eyre::Result<U256> {
        let contract = ISpokePool::new(self.claim.spoke_pool, &self.provider);
        let balance = contract
            .getRelayerRefund(self.claim.token, self.claim.relayer)
            .call()
            .await?;
        Ok(balance)
    }
}

impl<P> crate::Action for ClaimAction<P>
where
    P: Provider + Clone,
{
    async fn is_ready(&self) -> eyre::Result<bool> {
        // TODO: check against strategy
        Ok(true)
    }

    async fn is_completed(&self) -> eyre::Result<bool> {
        let _claimable = self.get_claimable_balance().await?;

        // TODO: check against strategy
        Ok(true)
    }

    async fn execute(&mut self) -> eyre::Result<crate::Result> {
        self.validate_claim()?;

        if !self.is_ready().await? {
            eyre::bail!("Claim not ready");
        }

        let contract = ISpokePool::new(self.claim.spoke_pool, &self.provider);
        let tx = contract.claimRelayerRefund(self.claim.token).send().await?;

        let tx_hash = *tx.tx_hash();
        let receipt = tx.get_receipt().await?;
        if !receipt.status() {
            eyre::bail!("Transaction reverted");
        }

        Ok(crate::Result {
            tx_hash,
            block_number: receipt.block_number,
            gas_used: Some(U256::from(receipt.gas_used)),
        })
    }

    fn description(&self) -> String {
        format!(
            "Claim relayer refund for {} from ISpokePool to {} to {}",
            self.claim.spoke_pool, self.claim.token, self.claim.refund_address,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_utils::MockProvider, Action};

    #[test]
    fn test_claim_validation() {
        let valid_claim = Claim {
            spoke_pool: Address::repeat_byte(1),
            token: Address::repeat_byte(2),
            refund_address: Address::repeat_byte(3),
            relayer: Address::repeat_byte(4),
        };

        let action = ClaimAction::new(MockProvider, valid_claim);
        assert!(action.validate_claim().is_ok());
    }

    #[test]
    fn test_claim_validation_zero_spoke_pool() {
        let invalid_claim = Claim {
            spoke_pool: Address::ZERO,
            token: Address::repeat_byte(2),
            refund_address: Address::repeat_byte(3),
            relayer: Address::repeat_byte(4),
        };

        let action = ClaimAction::new(MockProvider, invalid_claim);
        let result = action.validate_claim();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Spoke pool"));
    }

    #[test]
    fn test_claim_validation_zero_token() {
        let invalid_claim = Claim {
            spoke_pool: Address::repeat_byte(1),
            token: Address::ZERO,
            refund_address: Address::repeat_byte(3),
            relayer: Address::repeat_byte(4),
        };

        let action = ClaimAction::new(MockProvider, invalid_claim);
        let result = action.validate_claim();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Token"));
    }

    #[test]
    fn test_claim_validation_zero_refund_address() {
        let invalid_claim = Claim {
            spoke_pool: Address::repeat_byte(1),
            token: Address::repeat_byte(2),
            refund_address: Address::ZERO,
            relayer: Address::repeat_byte(4),
        };

        let action = ClaimAction::new(MockProvider, invalid_claim);
        let result = action.validate_claim();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Refund address"));
    }

    #[test]
    fn test_claim_validation_zero_relayer() {
        let invalid_claim = Claim {
            spoke_pool: Address::repeat_byte(1),
            token: Address::repeat_byte(2),
            refund_address: Address::repeat_byte(3),
            relayer: Address::ZERO,
        };

        let action = ClaimAction::new(MockProvider, invalid_claim);
        let result = action.validate_claim();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Relayer"));
    }

    #[tokio::test]
    async fn test_is_ready() {
        let claim = Claim {
            spoke_pool: Address::repeat_byte(1),
            token: Address::repeat_byte(2),
            refund_address: Address::repeat_byte(3),
            relayer: Address::repeat_byte(4),
        };

        let action = ClaimAction::new(MockProvider, claim);
        // Currently always returns true (TODO in implementation)
        assert!(action.is_ready().await.unwrap());
    }

    #[test]
    fn test_description() {
        let claim = Claim {
            spoke_pool: Address::repeat_byte(1),
            token: Address::repeat_byte(2),
            refund_address: Address::repeat_byte(3),
            relayer: Address::repeat_byte(4),
        };

        let action = ClaimAction::new(MockProvider, claim);
        let desc = action.description();

        assert!(desc.contains("Claim relayer refund"));
        assert!(desc.contains("0x0101010101010101010101010101010101010101")); // spoke_pool
        assert!(desc.contains("0x0202020202020202020202020202020202020202")); // token
        assert!(desc.contains("0x0303030303030303030303030303030303030303")); // refund_address
    }
}
