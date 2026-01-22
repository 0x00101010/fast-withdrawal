//! ERC20 token contract bindings.

use alloy_sol_types::sol;

sol! {
    /// Standard ERC20 token interface
    #[sol(rpc)]
    interface ERC20 {
        /// Emitted when tokens are transferred
        event Transfer(
            address indexed from,
            address indexed to,
            uint256 value
        );

        /// Emitted when an allowance is set
        event Approval(
            address indexed owner,
            address indexed spender,
            uint256 value
        );

        /// Get token balance of an account
        function balanceOf(address account) external view returns (uint256);

        /// Get allowance granted by owner to spender
        function allowance(address owner, address spender) external view returns (uint256);

        /// Approve spender to spend tokens
        function approve(address spender, uint256 amount) external returns (bool);

        /// Transfer tokens to recipient
        function transfer(address recipient, uint256 amount) external returns (bool);

        /// Transfer tokens from sender to recipient (requires allowance)
        function transferFrom(address sender, address recipient, uint256 amount) external returns (bool);

        /// Get token name
        function name() external view returns (string memory);

        /// Get token symbol
        function symbol() external view returns (string memory);

        /// Get token decimals
        function decimals() external view returns (uint8);

        /// Get total supply
        function totalSupply() external view returns (uint256);
    }
}
