use alloy_primitives::{utils::format_ether, Address, Bytes, U256};
use alloy_provider::Provider;
use alloy_sol_types::sol;

// Define Across SpokePool contract interface
// https://github.com/across-protocol/contracts/tree/v4.1.21
sol! {
   #[sol(rpc)]
   #[allow(clippy::too_many_arguments)]
   interface ISpokePool {
       /// Deposit tokens cross-chain via Across Protocol (V3)
       function depositV3(
           address depositor,
           address recipient,
           address inputToken,
           address outputToken,
           uint256 inputAmount,
           uint256 outputAmount,
           uint256 destinationChainId,
           address exclusiveRelayer,
           uint32 quoteTimestamp,
           uint32 fillDeadline,
           uint32 exclusivityParameter,
           bytes calldata message
       ) external payable;

       /// Event emitted when funds are deposited
       event FundsDeposited(
           address inputToken,
           address outputToken,
           uint256 inputAmount,
           uint256 outputAmount,
           uint256 indexed destinationChainId,
           uint32 indexed depositId,
           uint32 quoteTimestamp,
           uint32 fillDeadline,
           uint32 exclusivityDeadline,
           address indexed depositor,
           address recipient,
           address exclusiveRelayer,
           bytes message
       );
   }
}

/// Configuration for a deposit action.
#[derive(Debug, Clone)]
pub struct DepositConfig {
    /// SpokePool contract address on source chain
    pub spoke_pool: Address,
    /// Depositor address (who initiates the deposit)
    pub depositor: Address,
    /// Recipient address on destination chain
    pub recipient: Address,
    /// Input token on source chain
    /// See <https://github.com/across-protocol/contracts/blob/68a31fd4e9bdc080c86136650420d2c2ecbd1268/contracts/SpokePool.sol#L591-L593>
    /// for details. TLDR:
    /// Use WETH address and set input_amount = msg.value.
    pub input_token: Address,
    /// Output token on destination chain
    pub output_token: Address,
    /// Amount to deposit (in wei)
    pub input_amount: U256,
    /// Amount recipient receives (after fees)
    pub output_amount: U256,
    /// Destination chain ID
    pub destination_chain_id: u64,
    /// Exclusive relayer (address(0) for any relayer)
    pub exclusive_relayer: Address,
    /// Fill deadline (unix timestamp in seconds)
    pub fill_deadline: u32,
    /// Exclusivity parameter (0 for no exclusivity)
    pub exclusivity_parameter: u32,
    /// Optional message data
    pub message: Bytes,
}

/// Deposit action for sending tokens cross-chain via Across Protocol.
pub struct DepositAction<P> {
    provider: P,
    config: DepositConfig,
}

impl<P> DepositAction<P>
where
    P: Provider + Clone,
{
    /// Create a new deposit action.
    pub const fn new(provider: P, config: DepositConfig) -> Self {
        Self { provider, config }
    }

    /// Get the current timestamp (wall clock time).
    fn get_current_timestamp(&self) -> u32 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs() as u32
    }

    /// Validate the deposit configuration.
    fn validate_config(&self) -> eyre::Result<()> {
        if self.config.spoke_pool == Address::ZERO {
            eyre::bail!("SpokePool address is zero");
        }

        if self.config.recipient == Address::ZERO {
            eyre::bail!("Recipient address is zero");
        }

        if self.config.input_amount == U256::ZERO {
            eyre::bail!("Input amount is zero");
        }

        if self.config.output_amount > self.config.input_amount {
            eyre::bail!("Output amount exceeds input amount");
        }

        Ok(())
    }
}

impl<P> crate::Action for DepositAction<P>
where
    P: Provider + Clone + Send + Sync,
{
    fn is_ready(&self) -> bool {
        // Basic validation - can be executed synchronously
        self.config.spoke_pool != Address::ZERO
            && self.config.recipient != Address::ZERO
            && self.config.input_amount > U256::ZERO
            && self.config.output_amount <= self.config.input_amount
    }

    async fn is_completed(&self) -> eyre::Result<bool> {
        // TODO: Query if deposit was already made by checking V3FundsDeposited events
        // For now, always return false (idempotency handled by caller)
        Ok(false)
    }

    async fn execute(&self) -> eyre::Result<crate::Result> {
        // Validate before executing
        self.validate_config()?;

        if !self.is_ready() {
            eyre::bail!("Deposit not ready");
        }

        // Get current timestamp for quote
        let quote_timestamp = self.get_current_timestamp();

        // Create contract instance
        let contract = ISpokePool::new(self.config.spoke_pool, &self.provider);

        // Execute depositV3
        let pending_tx = contract
            .depositV3(
                self.config.depositor,
                self.config.recipient,
                self.config.input_token,
                self.config.output_token,
                self.config.input_amount,
                self.config.output_amount,
                U256::from(self.config.destination_chain_id),
                self.config.exclusive_relayer,
                quote_timestamp,
                self.config.fill_deadline,
                self.config.exclusivity_parameter,
                self.config.message.clone(),
            )
            .value(self.config.input_amount)
            .send()
            .await?;

        let tx_hash = *pending_tx.tx_hash();

        // Wait for confirmation
        let receipt = pending_tx.get_receipt().await?;

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
        let eth_amount = format_ether(self.config.input_amount);
        format!(
            "Deposit {} ETH from {} to chain {}",
            eth_amount, self.config.depositor, self.config.destination_chain_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_utils::MockProvider, Action};

    fn mock_config() -> DepositConfig {
        DepositConfig {
            spoke_pool: Address::from([1u8; 20]),
            depositor: Address::from([2u8; 20]),
            recipient: Address::from([3u8; 20]),
            input_token: Address::from([4u8; 20]),
            output_token: Address::from([5u8; 20]),
            input_amount: U256::from(1_000_000),
            output_amount: U256::from(990_000),
            destination_chain_id: 130,
            exclusive_relayer: Address::ZERO,
            fill_deadline: 1234567890,
            exclusivity_parameter: 0,
            message: Bytes::new(),
        }
    }

    #[test]
    fn test_is_ready_with_valid_config() {
        let config = mock_config();
        let action = DepositAction {
            provider: MockProvider {},
            config,
        };

        assert!(action.is_ready());
    }

    #[test]
    fn test_is_ready_with_zero_spoke_pool() {
        let mut config = mock_config();
        config.spoke_pool = Address::ZERO;
        let action = DepositAction {
            provider: MockProvider {},
            config,
        };

        assert!(!action.is_ready());
    }

    #[test]
    fn test_is_ready_with_zero_recipient() {
        let mut config = mock_config();
        config.recipient = Address::ZERO;
        let action = DepositAction {
            provider: MockProvider {},
            config,
        };

        assert!(!action.is_ready());
    }

    #[test]
    fn test_is_ready_with_zero_amount() {
        let mut config = mock_config();
        config.input_amount = U256::ZERO;
        let action = DepositAction {
            provider: MockProvider {},
            config,
        };

        assert!(!action.is_ready());
    }

    #[test]
    fn test_is_ready_with_output_exceeds_input() {
        let mut config = mock_config();
        config.input_amount = U256::from(100);
        config.output_amount = U256::from(200);
        let action = DepositAction {
            provider: MockProvider {},
            config,
        };

        assert!(!action.is_ready());
    }

    #[test]
    fn test_validate_config_success() {
        let config = mock_config();
        let action = DepositAction {
            provider: MockProvider {},
            config,
        };

        assert!(action.validate_config().is_ok());
    }

    #[test]
    fn test_validate_config_zero_spoke_pool() {
        let mut config = mock_config();
        config.spoke_pool = Address::ZERO;
        let action = DepositAction {
            provider: MockProvider {},
            config,
        };

        let result = action.validate_config();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SpokePool"));
    }

    #[test]
    fn test_validate_config_zero_recipient() {
        let mut config = mock_config();
        config.recipient = Address::ZERO;
        let action = DepositAction {
            provider: MockProvider {},
            config,
        };

        let result = action.validate_config();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Recipient"));
    }

    #[test]
    fn test_validate_config_zero_input_amount() {
        let mut config = mock_config();
        config.input_amount = U256::ZERO;
        let action = DepositAction {
            provider: MockProvider {},
            config,
        };

        let result = action.validate_config();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Input amount"));
    }

    #[test]
    fn test_validate_config_output_exceeds_input() {
        let mut config = mock_config();
        config.input_amount = U256::from(100);
        config.output_amount = U256::from(200);
        let action = DepositAction {
            provider: MockProvider {},
            config,
        };

        let result = action.validate_config();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Output amount exceeds"));
    }

    #[test]
    fn test_description() {
        let config = mock_config();
        let action = DepositAction {
            provider: MockProvider {},
            config: config.clone(),
        };

        let desc = action.description();
        assert!(desc.contains("Deposit"));
        assert!(desc.contains("ETH"));
        assert!(desc.contains(&config.destination_chain_id.to_string()));
    }

    #[test]
    fn test_deposit_config_fields() {
        let config = mock_config();

        assert_ne!(config.spoke_pool, Address::ZERO);
        assert_ne!(config.depositor, Address::ZERO);
        assert_ne!(config.recipient, Address::ZERO);
        assert!(config.input_amount > U256::ZERO);
        assert!(config.output_amount > U256::ZERO);
        assert!(config.output_amount <= config.input_amount);
    }
}
