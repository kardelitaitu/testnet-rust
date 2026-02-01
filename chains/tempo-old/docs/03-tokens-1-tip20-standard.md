# TIP-20 Token Standard ⋅ Tempo
TIP-20 is Tempo's native token standard for stablecoins and payment tokens. TIP-20 is designed for stablecoin payments, and is the foundation for many token-related functions on Tempo including transaction fees, payment lanes, DEX quote tokens, and optimized routing for DEX liquidity on Tempo.

All TIP-20 tokens are created by interacting with the [TIP-20 Factory contract](https://docs.tempo.xyz/protocol/tip20/spec#tip20factory), calling the `createToken` function. If you're issuing a stablecoin on Tempo, we **strongly recommend** using the TIP-20 standard. Learn more about the benefits, or follow the guide on issuance [here](https://docs.tempo.xyz/guide/issuance).

Benefits & Features of TIP-20 Tokens[](https://docs.tempo.xyz/protocol/tip20/overview#benefits--features-of-tip-20-tokens)
--------------------------------------------------------------------------------------------------------------------------

Below are some of the key benefits and features of TIP-20 tokens:

### Payments[](https://docs.tempo.xyz/protocol/tip20/overview#payments)

### Exchange[](https://docs.tempo.xyz/protocol/tip20/overview#exchange)

### Compliance & Controls[](https://docs.tempo.xyz/protocol/tip20/overview#compliance--controls)

### Pay Fees in Any Stablecoin[](https://docs.tempo.xyz/protocol/tip20/overview#pay-fees-in-any-stablecoin)

Any USD-denominated TIP-20 token can be used to pay transaction fees on Tempo.

The [Fee AMM](https://docs.tempo.xyz/protocol/fees/spec-fee-amm) automatically converts your token to the validator's preferred fee token, eliminating the need for users to hold a separate gas token. This feature works natively: no additional infrastructure or integration required.

Full specification of this feature can be found in the [Payment Lanes Specification](https://docs.tempo.xyz/protocol/blockspace/payment-lane-specification).

### Get Predictable Payment Fees[](https://docs.tempo.xyz/protocol/tip20/overview#get-predictable-payment-fees)

Tempo has dedicated payment lanes: reserved blockspace for payment TIP-20 transactions that other applications cannot consume. Even if there are extremely popular applications on the chain competing for blockspace, payroll runs or customer disbursements execute predictably. Learn more about the [payments lane](https://docs.tempo.xyz/protocol/blockspace/payment-lane-specification).

### Role-Based Access Control (RBAC)
[](https://docs.tempo.xyz/protocol/tip20/overview#role-based-access-control-rbac)

TIP-20 includes a built-in [RBAC system](https://docs.tempo.xyz/protocol/tip403/spec#tip-20-token-roles) that separates administrative responsibilities:

*   **ISSUER\_ROLE**: Grants permission to mint and burn tokens, enabling controlled token issuance
*   **PAUSE\_ROLE** / **UNPAUSE\_ROLE**: Allows pausing and unpausing token transfers for emergency controls
*   **BURN\_BLOCKED\_ROLE**: Permits burning tokens from blocked addresses (e.g., for compliance actions)

Roles can be granted, revoked, and delegated without custom contract changes. This enables issuers to separate operational roles (e.g., who can mint) from administrative roles (e.g., who can pause). Learn more in the [TIP-20 specification](https://docs.tempo.xyz/protocol/tip20/spec#roles).

### TIP-403 Transfer Policies[](https://docs.tempo.xyz/protocol/tip20/overview#tip-403-transfer-policies)

TIP-20 tokens integrate with the [TIP-403 Policy Registry](https://docs.tempo.xyz/protocol/tip403/overview) to enforce compliance policies. Each token can reference a policy that controls who can send and receive tokens:

*   **Whitelist policies**: Only addresses in the whitelist can transfer tokens
*   **Blacklist policies**: Addresses in the blacklist are blocked from transferring tokens

Policies can be shared across multiple tokens, enabling consistent compliance enforcement across your token ecosystem. See the [TIP-403 specification](https://docs.tempo.xyz/protocol/tip403/spec) for details.

### Operational Controls[](https://docs.tempo.xyz/protocol/tip20/overview#operational-controls)

TIP-20 tokens can set **supply caps**, which allow you to set a maximum token supply to control issuance.

TIP-20 tokens also have **pause/unpause** commands, which provide emergency controls to halt transfers when needed.

### Transfer Memos[](https://docs.tempo.xyz/protocol/tip20/overview#transfer-memos)

**Transfer memos** enable you to attach 32-byte memos to transfers for payment references, invoice IDs, or transaction notes.

### Reward Distribution[](https://docs.tempo.xyz/protocol/tip20/overview#reward-distribution)

TIP-20 supports an opt-in [reward distribution system](https://docs.tempo.xyz/protocol/tip20-rewards/overview) that allows issuers to distribute rewards to token holders. Rewards can be claimed by holders or automatically forwarded to designated recipient addresses.

### Currency Declaration[](https://docs.tempo.xyz/protocol/tip20/overview#currency-declaration)

A TIP-20 token can declare a currency identifier (e.g., `"USD"`, `"EUR"`) that identifies the real-world asset backing the token. This enables proper routing and pricing in Tempo's [Stablecoin DEX](https://docs.tempo.xyz/protocol/exchange). USD-denominated TIP-20 tokens can be used to pay transaction fees and serve as quote tokens in the DEX.

### DEX Quote Tokens[](https://docs.tempo.xyz/protocol/tip20/overview#dex-quote-tokens)

TIP-20 tokens can serve as quote tokens in Tempo's decentralized exchange (DEX). When creating trading pairs on the [Stablecoin DEX](https://docs.tempo.xyz/protocol/exchange), TIP-20 tokens function as the quote currency against which other tokens are priced and traded.

This enables efficient stablecoin-to-stablecoin trading and provides optimized routing for liquidity. For example, a USDC TIP-20 token can be paired with other stablecoins, allowing traders to swap between different USD-denominated tokens with minimal slippage through concentrated liquidity pools.

By using TIP-20 tokens as quote tokens, the DEX benefits from the same payment-optimized features like deterministic addresses, currency identifiers, and compliance policies, ensuring secure and efficient exchange operations.

Additional Links[](https://docs.tempo.xyz/protocol/tip20/overview#additional-links)
-----------------------------------------------------------------------------------

# TIP20 ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/tip20/spec#abstract)
---------------------------------------------------------------

TIP20 is a suite of precompiles that provide a built-in optimized token implementation in the core protocol. It extends the ERC-20 token standard with built-in functionality like memo fields and reward distribution.

Motivation[](https://docs.tempo.xyz/protocol/tip20/spec#motivation)
-------------------------------------------------------------------

All major stablecoins today use the ERC-20 token standard. While ERC-20 provides a solid foundation for fungible tokens, it lacks features critical for stablecoin issuers today such as memos, transfer policies, and rewards distribution. Additionally, since each ERC-20 token has its own implementation, integrators can't depend on consistent behavior across tokens. TIP-20 extends ERC-20, building these features into precompiled contracts that anyone can permissionlessly deploy on Tempo. This makes token operations much more efficient, allows issuers to quickly set up on Tempo, and simplifies integrations since it ensures standardized behavior across tokens. It also enables deeper integration with token-specific Tempo features like paying gas in stablecoins and payment lanes.

Specification[](https://docs.tempo.xyz/protocol/tip20/spec#specification)
-------------------------------------------------------------------------

TIP-20 tokens support standard fungible token operations such as transfers, mints, and burns. They also support transfers, mints, and burns with an attached 32-byte memo; a role-based access control system for token administrative operations; and a system for opt-in [reward distribution](https://docs.tempo.xyz/protocol/tip20-rewards/spec).

TIP20[](https://docs.tempo.xyz/protocol/tip20/spec#tip20-1)
-----------------------------------------------------------

The core TIP-20 contract exposes standard ERC-20 functions for balances, allowances, transfers, and delegated transfers, and also adds:

*   32-byte memo support on transfers, mints, and burns.
*   A `TIP20Roles` module for permissioned actions like issuing, pausing, unpausing, and burning blocked balances.
*   Configuration options for currencies, quote tokens, and transfer policies.

The complete TIP20 interface is defined below:

```
interface ITIP20 {
    // =========================================================================
    //                      ERC-20 standard functions
    // =========================================================================
 
    /// @notice Returns the name of the token
    /// @return The token name
    function name() external view returns (string memory);
    
    /// @notice Returns the symbol of the token
    /// @return The token symbol
    function symbol() external view returns (string memory);
    
    /// @notice Returns the number of decimals for the token
    /// @return Always returns 6 for TIP-20 tokens
    function decimals() external pure returns (uint8);
    
    /// @notice Returns the total amount of tokens in circulation
    /// @return The total supply of tokens
    function totalSupply() external view returns (uint256);
    
    /// @notice Returns the token balance of an account
    /// @param account The address to check the balance for
    /// @return The token balance of the account
    function balanceOf(address account) external view returns (uint256);
    
    /// @notice Transfers tokens from caller to recipient
    /// @param to The recipient address
    /// @param amount The amount of tokens to transfer
    /// @return True if successful
    function transfer(address to, uint256 amount) external returns (bool);
    
    /// @notice Returns the remaining allowance for a spender
    /// @param owner The token owner address
    /// @param spender The spender address
    /// @return The remaining allowance amount
    function allowance(address owner, address spender) external view returns (uint256);
    
    /// @notice Approves a spender to spend tokens on behalf of caller
    /// @param spender The address to approve
    /// @param amount The amount to approve
    /// @return True if successful
    function approve(address spender, uint256 amount) external returns (bool);
    
    /// @notice Transfers tokens from one address to another using allowance
    /// @param from The sender address
    /// @param to The recipient address
    /// @param amount The amount to transfer
    /// @return True if successful
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
 
    /// @notice Mints new tokens to an address (requires ISSUER_ROLE)
    /// @param to The recipient address
    /// @param amount The amount of tokens to mint
    function mint(address to, uint256 amount) external;
 
    /// @notice Burns tokens from caller's balance (requires ISSUER_ROLE)
    /// @param amount The amount of tokens to burn
    function burn(uint256 amount) external;
 
    // =========================================================================
    //                      TIP-20 extended functions
    // =========================================================================
 
    /// @notice Transfers tokens from caller to recipient with a memo
    /// @param to The recipient address
    /// @param amount The amount of tokens to transfer
    /// @param memo A 32-byte memo attached to the transfer
    function transferWithMemo(address to, uint256 amount, bytes32 memo) external;
    
    /// @notice Transfers tokens from one address to another with a memo using allowance
    /// @param from The sender address
    /// @param to The recipient address
    /// @param amount The amount to transfer
    /// @param memo A 32-byte memo attached to the transfer
    /// @return True if successful
    function transferFromWithMemo(address from, address to, uint256 amount, bytes32 memo) external returns (bool);
    
    /// @notice Mints new tokens to an address with a memo (requires ISSUER_ROLE)
    /// @param to The recipient address
    /// @param amount The amount of tokens to mint
    /// @param memo A 32-byte memo attached to the mint
    function mintWithMemo(address to, uint256 amount, bytes32 memo) external;
    
    /// @notice Burns tokens from caller's balance with a memo (requires ISSUER_ROLE)
    /// @param amount The amount of tokens to burn
    /// @param memo A 32-byte memo attached to the burn
    function burnWithMemo(uint256 amount, bytes32 memo) external;
    
    /// @notice Burns tokens from a blocked address (requires BURN_BLOCKED_ROLE)
    /// @param from The address to burn tokens from (must be unauthorized by transfer policy)
    /// @param amount The amount of tokens to burn
    function burnBlocked(address from, uint256 amount) external;
    
    /// @notice Returns the quote token used for DEX pairing
    /// @return The quote token address
    function quoteToken() external view returns (ITIP20);
    
    /// @notice Returns the next quote token staged for update
    /// @return The next quote token address (zero if none staged)
    function nextQuoteToken() external view returns (ITIP20);
    
    /// @notice Returns the currency identifier for this token
    /// @return The currency string
    function currency() external view returns (string memory);
    
    /// @notice Returns whether the token is currently paused
    /// @return True if paused, false otherwise
    function paused() external view returns (bool);
    
    /// @notice Returns the maximum supply cap for the token
    /// @return The supply cap (checked on mint operations)
    function supplyCap() external view returns (uint256);
    
    /// @notice Returns the current transfer policy ID from TIP-403 registry
    /// @return The transfer policy ID
    function transferPolicyId() external view returns (uint64);
    
    // =========================================================================
    //                            Admin Functions 
    // =========================================================================
    
    /// @notice Pauses the contract, blocking transfers (requires PAUSE_ROLE)
    function pause() external;
    
    /// @notice Unpauses the contract, allowing transfers (requires UNPAUSE_ROLE)
    function unpause() external;
    
    /// @notice Changes the transfer policy ID (requires DEFAULT_ADMIN_ROLE)
    /// @param newPolicyId The new policy ID from TIP-403 registry
    /// @dev Validates that the policy exists using TIP403Registry.policyExists().
    /// Built-in policies (ID 0 = always-reject, ID 1 = always-allow) are always valid.
    /// For custom policies (ID >= 2), the policy must exist in the TIP-403 registry.
    /// Reverts with InvalidTransferPolicyId if the policy does not exist.
    function changeTransferPolicyId(uint64 newPolicyId) external;
    
    /// @notice Stages a new quote token for update (requires DEFAULT_ADMIN_ROLE)
    /// @param newQuoteToken The new quote token address
    function setNextQuoteToken(ITIP20 newQuoteToken) external;
    
    /// @notice Completes the quote token update process (requires DEFAULT_ADMIN_ROLE)
    function completeQuoteTokenUpdate() external;
    
    /// @notice Sets the maximum supply cap (requires DEFAULT_ADMIN_ROLE)
    /// @param newSupplyCap The new supply cap (cannot be less than current supply)
    function setSupplyCap(uint256 newSupplyCap) external;
    
    // =========================================================================
    //                            Role Management
    // =========================================================================
    
    /// @notice Returns the BURN_BLOCKED_ROLE constant
    /// @return keccak256("BURN_BLOCKED_ROLE")
    function BURN_BLOCKED_ROLE() external view returns (bytes32);
    
    /// @notice Returns the ISSUER_ROLE constant
    /// @return keccak256("ISSUER_ROLE")
    function ISSUER_ROLE() external view returns (bytes32);
    
    /// @notice Returns the PAUSE_ROLE constant
    /// @return keccak256("PAUSE_ROLE")
    function PAUSE_ROLE() external view returns (bytes32);
    
    /// @notice Returns the UNPAUSE_ROLE constant
    /// @return keccak256("UNPAUSE_ROLE")
    function UNPAUSE_ROLE() external view returns (bytes32);
    
    /// @notice Grants a role to an account (requires role admin)
    /// @param role The role to grant (keccak256 hash)
    /// @param account The account to grant the role to
    function grantRole(bytes32 role, address account) external;
    
    /// @notice Revokes a role from an account (requires role admin)
    /// @param role The role to revoke (keccak256 hash)
    /// @param account The account to revoke the role from
    function revokeRole(bytes32 role, address account) external;
    
    /// @notice Allows an account to remove a role from itself
    /// @param role The role to renounce (keccak256 hash)
    function renounceRole(bytes32 role) external;
    
    /// @notice Changes the admin role for a specific role (requires current role admin)
    /// @param role The role whose admin is being changed
    /// @param adminRole The new admin role
    function setRoleAdmin(bytes32 role, bytes32 adminRole) external;
    
    // =========================================================================
    //                            System Functions
    // =========================================================================
    
    /// @notice System-level transfer function (restricted to precompiles)
    /// @param from The sender address
    /// @param to The recipient address
    /// @param amount The amount to transfer
    /// @return True if successful
    function systemTransferFrom(address from, address to, uint256 amount) external returns (bool);
    
    /// @notice Pre-transaction fee transfer (restricted to precompiles)
    /// @param from The account to charge fees from
    /// @param amount The fee amount
    function transferFeePreTx(address from, uint256 amount) external;
    
    /// @notice Post-transaction fee handling (restricted to precompiles)
    /// @param to The account to refund
    /// @param refund The refund amount
    /// @param actualUsed The actual fee used
    function transferFeePostTx(address to, uint256 refund, uint256 actualUsed) external;
 
 
    // =========================================================================
    //                                Events
    // =========================================================================
 
    /// @notice Emitted when a new allowance is set by `owner` for `spender`
    /// @param owner The account granting the allowance
    /// @param spender The account being approved to spend tokens
    /// @param amount The new allowance amount
    event Approval(address indexed owner, address indexed spender, uint256 amount);
 
    /// @notice Emitted when tokens are burned from an address
    /// @param from The address whose tokens were burned
    /// @param amount The amount of tokens that were burned
    event Burn(address indexed from, uint256 amount);
 
    /// @notice Emitted when tokens are burned from a blocked address
    /// @param from The blocked address whose tokens were burned
    /// @param amount The amount of tokens that were burned
    event BurnBlocked(address indexed from, uint256 amount);
 
    /// @notice Emitted when new tokens are minted to an address
    /// @param to The address receiving the minted tokens
    /// @param amount The amount of tokens that were minted
    event Mint(address indexed to, uint256 amount);
 
    /// @notice Emitted when a new quote token is staged for this token
    /// @param updater The account that staged the new quote token
    /// @param nextQuoteToken The quote token that has been staged
    event NextQuoteTokenSet(address indexed updater, ITIP20 indexed nextQuoteToken);
 
    /// @notice Emitted when the pause state of the token changes
    /// @param updater The account that changed the pause state
    /// @param isPaused The new pause state; true if paused, false if unpaused
    event PauseStateUpdate(address indexed updater, bool isPaused);
 
    /// @notice Emitted when the quote token update process is completed
    /// @param updater The account that completed the quote token update
    /// @param newQuoteToken The new quote token that has been set
    event QuoteTokenUpdate(address indexed updater, ITIP20 indexed newQuoteToken);
 
    /// @notice Emitted when a holder sets or updates their reward recipient address
    /// @param holder The token holder configuring the recipient
    /// @param recipient The address that will receive claimed rewards
    event RewardRecipientSet(address indexed holder, address indexed recipient);
 
    /// @notice Emitted when a reward distribution is scheduled
    /// @param funder The account funding the reward distribution
    /// @param amount The total amount of tokens allocated to the reward
    event RewardDistributed(
        address indexed funder,
        uint256 amount,
    );
 
    /// @notice Emitted when the token's supply cap is updated
    /// @param updater The account that updated the supply cap
    /// @param newSupplyCap The new maximum total supply
    event SupplyCapUpdate(address indexed updater, uint256 indexed newSupplyCap);
 
    /// @notice Emitted for all token movements, including mints and burns
    /// @param from The address sending tokens (address(0) for mints)
    /// @param to The address receiving tokens (address(0) for burns)
    /// @param amount The amount of tokens transferred
    event Transfer(address indexed from, address indexed to, uint256 amount);
 
    /// @notice Emitted when the transfer policy ID is updated
    /// @param updater The account that updated the transfer policy
    /// @param newPolicyId The new transfer policy ID from the TIP-403 registry
    event TransferPolicyUpdate(address indexed updater, uint64 indexed newPolicyId);
 
    /// @notice Emitted when a transfer, mint, or burn is performed with an attached memo
    /// @param from The address sending tokens (address(0) for mints)
    /// @param to The address receiving tokens (address(0) for burns)
    /// @param amount The amount of tokens transferred
    /// @param memo The 32-byte memo associated with this movement
    event TransferWithMemo(
        address indexed from,
        address indexed to,
        uint256 amount,
        bytes32 indexed memo
    );
 
    /// @notice Emitted when the membership of a role changes for an account
    /// @param role The role being granted or revoked
    /// @param account The account whose membership was changed
    /// @param sender The account that performed the change
    /// @param hasRole True if the role was granted, false if it was revoked
    event RoleMembershipUpdated(
        bytes32 indexed role,
        address indexed account,
        address indexed sender,
        bool hasRole
    );
 
    /// @notice Emitted when the admin role for a role is updated
    /// @param role The role whose admin role was changed
    /// @param newAdminRole The new admin role for the given role
    /// @param sender The account that performed the update
    event RoleAdminUpdated(
        bytes32 indexed role,
        bytes32 indexed newAdminRole,
        address indexed sender
    );
 
    // =========================================================================
    //                                Errors
    // =========================================================================
 
    /// @notice The token operation is blocked because the contract is currently paused
    error ContractPaused();
 
    /// @notice The spender does not have enough allowance for the attempted transfer
    error InsufficientAllowance();
 
    /// @notice The account does not have the required token balance for the operation
    /// @param currentBalance The current balance of the account
    /// @param expectedBalance The required balance for the operation to succeed
    /// @param token The address of the token contract
    error InsufficientBalance(uint256 currentBalance, uint256 expectedBalance, address token);
 
    /// @notice The provided amount is zero or otherwise invalid for the attempted operation
    error InvalidAmount();
 
    /// @notice The provided currency identifier is invalid or unsupported
    error InvalidCurrency();
 
    /// @notice The specified quote token is invalid, incompatible, or would create a circular reference
    error InvalidQuoteToken();
 
    /// @notice The recipient address is not a valid destination for this operation
    ///         (for example, another TIP-20 token contract)
    error InvalidRecipient();
 
    /// @notice The specified transfer policy ID does not exist in the TIP-403 registry
    error InvalidTransferPolicyId();
 
    /// @notice The new supply cap is invalid, for example lower than the current total supply
    error InvalidSupplyCap();
 
    /// @notice A rewards operation was attempted when no opted-in supply exists
    error NoOptedInSupply();
 
    /// @notice The configured transfer policy denies authorization for the sender or recipient
    error PolicyForbids();
 
    /// @notice The attempted operation would cause total supply to exceed the configured supply cap
    error SupplyCapExceeded();
 
    /// @notice The caller does not have the required role or permission for this operation
    error Unauthorized();
 
}
```


Memos[](https://docs.tempo.xyz/protocol/tip20/spec#memos)
---------------------------------------------------------

Memo functions `transferWithMemo`, `transferFromWithMemo`, `mintWithMemo`, and `burnWithMemo` behave like their ERC-20 equivalents but additionally emit memo data in dedicated events. The memo is always a fixed 32-byte field. Callers should pack shorter strings or identifiers directly into this field, and use hashes or external references when the underlying payload exceeds 32 bytes.

TIP-403 Transfer Policies[](https://docs.tempo.xyz/protocol/tip20/spec#tip-403-transfer-policies)
-------------------------------------------------------------------------------------------------

All operations that move tokens: `transfer`, `transferFrom`, `transferWithMemo`, `transferFromWithMemo`, `mint`, `burn`, `mintWithMemo`, and `burnWithMemo` — enforce the token’s configured TIP-403 transfer policy.

Internally, this is implemented via a `transferAuthorized` modifier that:

*   Calls `TIP403_REGISTRY.isAuthorized(transferPolicyId, from)` for the sender.
*   Calls `TIP403_REGISTRY.isAuthorized(transferPolicyId, to)` for the recipient.

Both checks must return `true`, otherwise the call reverts with `PolicyForbids`. Reward operations (`distributeReward`, `setRewardRecipient`, `claimRewards`) also perform the same TIP-403 authorization checks before moving any funds.

Invalid Recipient Protection[](https://docs.tempo.xyz/protocol/tip20/spec#invalid-recipient-protection)
-------------------------------------------------------------------------------------------------------

TIP-20 tokens cannot be sent to other TIP-20 token contract addresses. The implementation uses a `validRecipient` guard that rejects recipients whose address is zero, or has the TIP-20 prefix (`0x20c000000000000000000000`). Any attempt to transfer to a TIP-20 token address must revert with `InvalidRecipient`. This prevents accidental token loss by sending funds to token contracts instead of user accounts.

Currencies and Quote Tokens[](https://docs.tempo.xyz/protocol/tip20/spec#currencies-and-quote-tokens)
-----------------------------------------------------------------------------------------------------

Each TIP-20 token declares a currency identifier and a corresponding `quoteToken` used for pricing and routing in the Stablecoin DEX. Tokens with `currency == "USD"` must pair with a USD-denominated TIP-20 token.

Updating the quote token occurs in two phases:

1.  `setNextQuoteToken` stages a new quote token.
2.  `completeQuoteTokenUpdate` finalizes the change.

The implementation must validate that the new quote token is a TIP-20 token, matches currency rules, and does not create circular quote-token chains.

Pause Controls[](https://docs.tempo.xyz/protocol/tip20/spec#pause-controls)
---------------------------------------------------------------------------

Pause controls `pause` and `unpause` govern all transfer operations and reward related flows. When paused, transfers and memo transfers halt, but administrative and configuration functions remain allowed. The `paused()` getter reflects the current state and must be checked by all affected entrypoints.

TIP-20 Roles[](https://docs.tempo.xyz/protocol/tip20/spec#tip-20-roles)
-----------------------------------------------------------------------

TIP-20 uses a role-based authorization system. The main roles are:

*   `ISSUER_ROLE`: controls minting and burning.
*   `PAUSE_ROLE` / `UNPAUSE_ROLE`: controls the token’s paused state.
*   `BURN_BLOCKED_ROLE`: allows burning balances belonging to addresses that fail TIP-403 authorization.

Roles are assigned and managed through `grantRole`, `revokeRole`, `renounceRole`, and `setRoleAdmin`, via the contract admin.

System Functions[](https://docs.tempo.xyz/protocol/tip20/spec#system-functions)
-------------------------------------------------------------------------------

System level functions `systemTransferFrom`, `transferFeePreTx`, and `transferFeePostTx` are only callable by other Tempo protocol precompiles. These entrypoints power transaction fee collection, refunds, and internal accounting within the Fee AMM and Stablecoin DEX. They must not be callable by general contracts or users.

`transferFeePreTx` respects the token's pause state and will revert if the token is paused. However, `transferFeePostTx` is intentionally allowed to execute even when the token is paused. This ensures that a transaction which pauses the token can still complete successfully and receive its fee refund. Apart from this specific refund transfer, no other token transfers can occur after a pause event.

Token Rewards Distribution[](https://docs.tempo.xyz/protocol/tip20/spec#token-rewards-distribution)
---------------------------------------------------------------------------------------------------

See [rewards distribution](https://docs.tempo.xyz/protocol/tip20-rewards/spec) for more information.

TIP20Factory[](https://docs.tempo.xyz/protocol/tip20/spec#tip20factory)
-----------------------------------------------------------------------

The `TIP20Factory` contract is the canonical entrypoint for creating new TIP-20 tokens on Tempo. The factory derives deterministic deployment addresses using a caller-provided salt, combined with the caller's address, under a fixed 12-byte TIP-20 prefix. This ensures that every TIP-20 token exists at a predictable, collision-free address. The `TIP20Factory` precompile is deployed at `0x20Fc000000000000000000000000000000000000`.

Newly created TIP-20 addresses are deployed to a deterministic address derived from `TIP20_PREFIX || lowerBytes`, where:

*   `TIP20_PREFIX` is the 12-byte prefix `20C000000000000000000000`
*   `lowerBytes` is the highest 64 bits of `keccak256(msg.sender, salt)`

The first 1000 addresses (where `lowerBytes < 1000`) are reserved for protocol use and cannot be deployed to via the factory.

When creating a token, the factory performs several checks to guarantee consistency across the TIP-20 ecosystem:

*   The specified Quote token must be a currently deployed TIP20.
*   Tokens that specify their currency as USD must also specify a quote token that is denoted in USD.
*   At deployment, the factory initializes defaults on the TIP-20:  
    `transferPolicyId = 1`, `supplyCap = type(uint128).max`, `paused = false`, and `totalSupply = 0`.
*   The provided `admin` address receives `DEFAULT_ADMIN_ROLE`, enabling it to manage roles and token configurations.

The complete `TIP20Factory` interface is defined below:

```
/// @title TIP-20 Factory Interface
/// @notice Deploys and initializes new TIP-20 tokens at deterministic addresses
interface ITIP20Factory {
    /// @notice Creates and deploys a new TIP-20 token
    /// @param name The token's ERC-20 name
    /// @param symbol The token's ERC-20 symbol
    /// @param currency The token's currency identifier (e.g. "USD")
    /// @param quoteToken The TIP-20 quote token used for exchange pricing
    /// @param admin The address to receive DEFAULT_ADMIN_ROLE on the new token
    /// @param salt A unique salt for deterministic address derivation
    ///
    /// @return token The deployed TIP-20 token address
    /// @dev
    ///  - Computes the TIP-20 deployment address as TIP20_PREFIX || lowerBytes,
    ///    where lowerBytes is the highest 64 bits of keccak256(msg.sender, salt)
    ///  - Reverts with AddressReserved if lowerBytes < 1000
    ///  - Ensures the provided quote token is itself a valid TIP-20
    ///  - Enforces USD-denomination rules (USD tokens must use USD quote tokens)
    ///  - Initializes the token with default settings:
    ///         transferPolicyId = 1 (always-allow)
    ///         supplyCap        = type(uint128).max
    ///         paused           = false
    ///         totalSupply      = 0
    ///  - Grants DEFAULT_ADMIN_ROLE on the new token to `admin`
    ///  - Emits a {TokenCreated} event
    function createToken(
        string memory name,
        string memory symbol,
        string memory currency,
        ITIP20 quoteToken,
        address admin,
        bytes32 salt
    ) external returns (address token);
 
    // =========================================================================
    //                                Helpers
    // =========================================================================
 
    /// @notice Returns true if `token` is a valid TIP-20 address
    /// @param token The address to check
    /// @return True if the address is a well-formed TIP-20
    /// @dev Checks the TIP-20 prefix and verifies the token has code deployed
    function isTIP20(address token) external view returns (bool);
 
    /// @notice Computes the deterministic TIP-20 address for a given sender and salt
    /// @param sender The address that will call {createToken}
    /// @param salt The salt that will be passed to {createToken}
    /// @return token The TIP-20 address that would be deployed
    /// @dev Computes the address as TIP20_PREFIX || lowerBytes, where lowerBytes is
    ///      the highest 64 bits of keccak256(sender, salt), matching the factory deployment scheme.
    function getTokenAddress(address sender, bytes32 salt) external pure returns (address token);
 
    // =========================================================================
    //                                Events
    // =========================================================================
 
    /// @notice Emitted when a new TIP-20 token is created
    /// @param token The newly deployed TIP-20 address
    /// @param name The token name
    /// @param symbol The token symbol
    /// @param currency The token currency
    /// @param quoteToken The token's assigned quote token
    /// @param admin The address receiving DEFAULT_ADMIN_ROLE
    /// @param salt The salt used for deterministic address derivation
    event TokenCreated(
        address indexed token,
        string name,
        string symbol,
        string currency,
        ITIP20 quoteToken,
        address admin,
        bytes32 salt
    );
 
    // =========================================================================
    //                                Errors
    // =========================================================================
 
    /// @notice The computed address falls within the reserved range (lowerBytes < 1000)
    error AddressReserved();
 
    /// @notice The provided quote token address is invalid or not a TIP-20
    error InvalidQuoteToken();
}
```


Invariants[](https://docs.tempo.xyz/protocol/tip20/spec#invariants)
-------------------------------------------------------------------

*   `totalSupply()` must always equal to the sum of all `balanceOf(account)` over all accounts.
*   `totalSupply()` must always be `<= supplyCap`
*   When `paused` is `true`, no functions that move tokens (`transfer`, `transferFrom`, memo variants, `systemTransferFrom`, `transferFeePreTx`, `distributeReward`, `setRewardRecipient`, `claimRewards`) can succeed.
*   TIP20 tokens cannot be transferred to another TIP20 token contract address.
*   `systemTransferFrom`, `transferFeePreTx`, and `transferFeePostTx` never change `totalSupply()`.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import { TIP20Factory } from "./TIP20Factory.sol";
import { TIP403Registry } from "./TIP403Registry.sol";
import { TempoUtilities } from "./TempoUtilities.sol";
import { TIP20RolesAuth } from "./abstracts/TIP20RolesAuth.sol";
import { ITIP20 } from "./interfaces/ITIP20.sol";

contract TIP20 is ITIP20, TIP20RolesAuth {

    TIP403Registry internal constant TIP403_REGISTRY =
        TIP403Registry(0x403c000000000000000000000000000000000000);

    address internal constant TIP_FEE_MANAGER_ADDRESS = 0xfeEC000000000000000000000000000000000000;
    address internal constant STABLECOIN_DEX_ADDRESS = 0xDEc0000000000000000000000000000000000000;

    address internal constant FACTORY = 0x20Fc000000000000000000000000000000000000;

    /*//////////////////////////////////////////////////////////////
                                METADATA
    //////////////////////////////////////////////////////////////*/

    string public name;
    string public symbol;
    string public currency;

    function decimals() public pure returns (uint8) {
        return 6;
    }

    /*//////////////////////////////////////////////////////////////
                             ADMINISTRATION
    //////////////////////////////////////////////////////////////*/

    ITIP20 public override quoteToken;
    ITIP20 public override nextQuoteToken;

    bytes32 public constant PAUSE_ROLE = keccak256("PAUSE_ROLE");
    bytes32 public constant UNPAUSE_ROLE = keccak256("UNPAUSE_ROLE");
    bytes32 public constant ISSUER_ROLE = keccak256("ISSUER_ROLE");
    bytes32 public constant BURN_BLOCKED_ROLE = keccak256("BURN_BLOCKED_ROLE");

    uint64 public transferPolicyId = 1; // "Always-allow" policy by default.

    constructor(
        string memory _name,
        string memory _symbol,
        string memory _currency,
        ITIP20 _quoteToken,
        address admin,
        address sender
    ) {
        name = _name;
        symbol = _symbol;
        currency = _currency;
        quoteToken = _quoteToken;
        nextQuoteToken = _quoteToken;
        // No currency registry; all tokens use 6 decimals by default

        hasRole[admin][DEFAULT_ADMIN_ROLE] = true; // Grant admin role to first admin.
        emit RoleMembershipUpdated(DEFAULT_ADMIN_ROLE, admin, sender, true);
    }

    /*//////////////////////////////////////////////////////////////
                              ERC20 STORAGE
    //////////////////////////////////////////////////////////////*/

    uint128 internal _totalSupply;
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    /*//////////////////////////////////////////////////////////////
                              TIP20 STORAGE
    //////////////////////////////////////////////////////////////*/

    bool public paused = false;
    uint256 public supplyCap = type(uint128).max; // Default to cap at uint128.max

    /*//////////////////////////////////////////////////////////////
                        REWARD DISTRIBUTION STORAGE
    //////////////////////////////////////////////////////////////*/

    uint256 internal constant ACC_PRECISION = 1e18;
    uint256 public globalRewardPerToken;
    uint128 public optedInSupply;

    struct UserRewardInfo {
        address rewardRecipient;
        uint256 rewardPerToken;
        uint256 rewardBalance;
    }

    mapping(address => UserRewardInfo) public userRewardInfo;

    /*//////////////////////////////////////////////////////////////
                              POLICY ADMINISTRATION
    //////////////////////////////////////////////////////////////*/

    function changeTransferPolicyId(uint64 newPolicyId) external onlyRole(DEFAULT_ADMIN_ROLE) {
        // Validate that the policy exists
        if (!TIP403_REGISTRY.policyExists(newPolicyId)) {
            revert InvalidTransferPolicyId();
        }

        emit TransferPolicyUpdate(msg.sender, transferPolicyId = newPolicyId);
    }

    /*//////////////////////////////////////////////////////////////
                          TOKEN ADMINISTRATION
    //////////////////////////////////////////////////////////////*/

    function setNextQuoteToken(ITIP20 newQuoteToken) external onlyRole(DEFAULT_ADMIN_ROLE) {
        // sets next quote token, to put the DEX for that pair into place-only mode
        // does not check for loops; that is checked in completeQuoteTokenUpdate
        if (!TempoUtilities.isTIP20(address(newQuoteToken))) {
            revert InvalidQuoteToken();
        }

        // If this token represents USD, enforce USD quote token
        if (keccak256(bytes(currency)) == keccak256(bytes("USD"))) {
            if (keccak256(bytes(newQuoteToken.currency())) != keccak256(bytes("USD"))) {
                revert InvalidQuoteToken();
            }
        }

        nextQuoteToken = newQuoteToken;
        emit NextQuoteTokenSet(msg.sender, newQuoteToken);
    }

    function completeQuoteTokenUpdate() external onlyRole(DEFAULT_ADMIN_ROLE) {
        // check that this does not create a loop, by looping through quote token until we reach the root
        ITIP20 current = nextQuoteToken;
        while (address(current) != address(0)) {
            if (current == this) revert InvalidQuoteToken();
            current = current.quoteToken();
        }

        quoteToken = nextQuoteToken;
        emit QuoteTokenUpdate(msg.sender, nextQuoteToken);
    }

    function setSupplyCap(uint256 newSupplyCap) external onlyRole(DEFAULT_ADMIN_ROLE) {
        if (newSupplyCap < _totalSupply) revert InvalidSupplyCap();
        if (newSupplyCap > type(uint128).max) revert SupplyCapExceeded();
        emit SupplyCapUpdate(msg.sender, supplyCap = newSupplyCap);
    }

    function pause() external onlyRole(PAUSE_ROLE) {
        emit PauseStateUpdate(msg.sender, paused = true);
    }

    function unpause() external onlyRole(UNPAUSE_ROLE) {
        emit PauseStateUpdate(msg.sender, paused = false);
    }

    function mint(address to, uint256 amount) external onlyRole(ISSUER_ROLE) {
        _mint(to, amount);
        emit Mint(to, amount);
    }

    function burn(uint256 amount) external onlyRole(ISSUER_ROLE) {
        _transfer(msg.sender, address(0), amount);
        unchecked {
            _totalSupply -= uint128(amount);
        }

        emit Burn(msg.sender, amount);
    }

    function burnBlocked(address from, uint256 amount) external onlyRole(BURN_BLOCKED_ROLE) {
        // Prevent burning from protected precompile addresses
        if (from == TIP_FEE_MANAGER_ADDRESS || from == STABLECOIN_DEX_ADDRESS) {
            revert ProtectedAddress();
        }

        // Only allow burning from addresses that are blocked from transferring.
        if (TIP403_REGISTRY.isAuthorized(transferPolicyId, from)) {
            revert PolicyForbids();
        }

        _transfer(from, address(0), amount);
        unchecked {
            _totalSupply -= uint128(amount);
        }

        emit BurnBlocked(from, amount);
    }

    function mintWithMemo(address to, uint256 amount, bytes32 memo) external onlyRole(ISSUER_ROLE) {
        _mint(to, amount);
        emit TransferWithMemo(address(0), to, amount, memo);
        emit Mint(to, amount);
    }

    function burnWithMemo(uint256 amount, bytes32 memo) external onlyRole(ISSUER_ROLE) {
        _transfer(msg.sender, address(0), amount);
        unchecked {
            _totalSupply -= uint128(amount);
        }

        emit TransferWithMemo(msg.sender, address(0), amount, memo);
        emit Burn(msg.sender, amount);
    }

    /*//////////////////////////////////////////////////////////////
                        STANDARD ERC20 FUNCTIONS
    //////////////////////////////////////////////////////////////*/

    modifier notPaused() {
        if (paused) revert ContractPaused();
        _;
    }

    modifier validRecipient(address to) {
        // Don't allow sending to the zero address not other precompiled tokens.
        if (to == address(0) || (uint160(to) >> 64) == 0x20c000000000000000000000) {
            revert InvalidRecipient();
        }
        _;
    }

    modifier transferAuthorized(address from, address to) {
        if (
            !TIP403_REGISTRY.isAuthorized(transferPolicyId, from)
                || !TIP403_REGISTRY.isAuthorized(transferPolicyId, to)
        ) revert PolicyForbids();
        _;
    }

    function transfer(address to, uint256 amount)
        public
        virtual
        notPaused
        validRecipient(to)
        transferAuthorized(msg.sender, to)
        returns (bool)
    {
        _transfer(msg.sender, to, amount);
        return true;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        emit Approval(msg.sender, spender, allowance[msg.sender][spender] = amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount)
        public
        virtual
        notPaused
        validRecipient(to)
        transferAuthorized(from, to)
        returns (bool)
    {
        _transferFrom(from, to, amount);
        return true;
    }

    function _transferFrom(address from, address to, uint256 amount) internal {
        // Allowance check and update.
        uint256 allowed = allowance[from][msg.sender];
        if (amount > allowed) revert InsufficientAllowance();
        unchecked {
            if (allowed != type(uint256).max) {
                allowance[from][msg.sender] = allowed - amount;
            }
        }

        _transfer(from, to, amount);
    }

    function totalSupply() public view returns (uint256) {
        return _totalSupply;
    }

    function _transfer(address from, address to, uint256 amount) internal {
        if (amount > balanceOf[from]) {
            revert InsufficientBalance(balanceOf[from], amount, address(this));
        }

        // Handle reward accounting for opted-in sender
        address fromsRewardRecipient = _updateRewardsAndGetRecipient(from);

        // Handle reward accounting for opted-in receiver (but not when burning)
        address tosRewardRecipient = _updateRewardsAndGetRecipient(to);

        if (fromsRewardRecipient != address(0)) {
            if (tosRewardRecipient == address(0)) {
                optedInSupply -= uint128(amount);
            }
        } else if (tosRewardRecipient != address(0)) {
            optedInSupply += uint128(amount);
        }

        unchecked {
            balanceOf[from] -= amount;
            if (to != address(0)) balanceOf[to] += amount;
        }

        emit Transfer(from, to, amount);
    }

    function _mint(address to, uint256 amount) internal {
        if (!TIP403_REGISTRY.isAuthorized(transferPolicyId, to)) {
            revert PolicyForbids();
        }
        if (_totalSupply + amount > supplyCap) revert SupplyCapExceeded(); // Catches overflow.

        // Handle reward accounting for opted-in receiver
        address tosRewardRecipient = _updateRewardsAndGetRecipient(to);
        if (tosRewardRecipient != address(0)) {
            optedInSupply += uint128(amount);
        }

        unchecked {
            _totalSupply += uint128(amount);
            balanceOf[to] += amount;
        }

        emit Transfer(address(0), to, amount);
    }

    /*//////////////////////////////////////////////////////////////
                        TIP20 EXTENSION FUNCTIONS
    //////////////////////////////////////////////////////////////*/

    function transferWithMemo(address to, uint256 amount, bytes32 memo)
        public
        virtual
        notPaused
        validRecipient(to)
        transferAuthorized(msg.sender, to)
    {
        _transfer(msg.sender, to, amount);
        emit TransferWithMemo(msg.sender, to, amount, memo);
    }

    function transferFromWithMemo(address from, address to, uint256 amount, bytes32 memo)
        public
        virtual
        notPaused
        validRecipient(to)
        transferAuthorized(from, to)
        returns (bool)
    {
        // Allowance check and update.
        uint256 allowed = allowance[from][msg.sender];
        if (amount > allowed) revert InsufficientAllowance();
        unchecked {
            if (allowed != type(uint256).max) {
                allowance[from][msg.sender] = allowed - amount;
            }
        }

        _transfer(from, to, amount);
        emit TransferWithMemo(from, to, amount, memo);
        return true;
    }

    /// @dev In the Tempo node implementation, this function is not exposed via the TIP20 interface
    /// and is not externally callable. It is only invoked internally by specific precompiles
    /// (like the fee manager precompile), avoiding the need to approve precompiles to spend tokens.
    function systemTransferFrom(address from, address to, uint256 amount)
        external
        virtual
        notPaused
        validRecipient(to)
        transferAuthorized(from, to)
        returns (bool)
    {
        require(msg.sender == TIP_FEE_MANAGER_ADDRESS);
        _transfer(from, to, amount);
        return true;
    }

    /*//////////////////////////////////////////////////////////////
                            FEE MANAGEMENT
    //////////////////////////////////////////////////////////////*/

    function transferFeePreTx(address from, uint256 amount) external notPaused {
        require(msg.sender == TIP_FEE_MANAGER_ADDRESS);
        require(from != address(0));

        if (amount > balanceOf[from]) {
            revert InsufficientBalance(balanceOf[from], amount, address(this));
        }

        address fromsRewardRecipient = _updateRewardsAndGetRecipient(from);
        if (fromsRewardRecipient != address(0)) {
            optedInSupply -= uint128(amount);
        }

        unchecked {
            balanceOf[from] -= amount;
            balanceOf[TIP_FEE_MANAGER_ADDRESS] += amount;
        }
    }

    function transferFeePostTx(address to, uint256 refund, uint256 actualUsed) external {
        require(msg.sender == TIP_FEE_MANAGER_ADDRESS);
        require(to != address(0));

        uint256 feeManagerBalance = balanceOf[TIP_FEE_MANAGER_ADDRESS];
        if (refund > feeManagerBalance) {
            revert InsufficientBalance(feeManagerBalance, refund, address(this));
        }

        address tosRewardRecipient = _updateRewardsAndGetRecipient(to);
        if (tosRewardRecipient != address(0)) {
            optedInSupply += uint128(refund);
        }

        unchecked {
            balanceOf[TIP_FEE_MANAGER_ADDRESS] -= refund;
            balanceOf[to] += refund;
        }

        emit Transfer(to, TIP_FEE_MANAGER_ADDRESS, actualUsed);
    }

    /*//////////////////////////////////////////////////////////////
                        REWARD DISTRIBUTION
    //////////////////////////////////////////////////////////////*/

    // Updates the rewards for `user` and their `rewardRecipient`
    function _updateRewardsAndGetRecipient(address user)
        internal
        returns (address rewardRecipient)
    {
        rewardRecipient = userRewardInfo[user].rewardRecipient;
        uint256 cachedGlobalRewardPerToken = globalRewardPerToken;
        uint256 rewardPerTokenDelta =
            cachedGlobalRewardPerToken - userRewardInfo[user].rewardPerToken;

        if (rewardPerTokenDelta != 0) {
            // No rewards to update if not opted-in
            if (rewardRecipient != address(0)) {
                // Balance to update
                uint256 reward = (uint256(balanceOf[user]) * (rewardPerTokenDelta)) / ACC_PRECISION;

                userRewardInfo[rewardRecipient].rewardBalance += reward;
            }
            userRewardInfo[user].rewardPerToken = cachedGlobalRewardPerToken;
        }
    }

    /// @notice Distributes rewards to opted-in token holders.
    function distributeReward(uint256 amount) external virtual notPaused {
        if (amount == 0) revert InvalidAmount();
        if (!TIP403_REGISTRY.isAuthorized(transferPolicyId, msg.sender)) {
            revert PolicyForbids();
        }

        // Transfer tokens from sender to this contract
        _transfer(msg.sender, address(this), amount);

        // Immediate payout
        if (optedInSupply == 0) {
            revert NoOptedInSupply();
        }
        uint256 deltaRPT = (amount * ACC_PRECISION) / optedInSupply;
        globalRewardPerToken += deltaRPT;
        emit RewardDistributed(msg.sender, amount);
    }

    function setRewardRecipient(address newRewardRecipient) external virtual notPaused {
        // Check TIP-403 authorization
        if (newRewardRecipient != address(0)) {
            if (
                !TIP403_REGISTRY.isAuthorized(transferPolicyId, msg.sender)
                    || !TIP403_REGISTRY.isAuthorized(transferPolicyId, newRewardRecipient)
            ) revert PolicyForbids();
        }

        address oldRewardRecipient = _updateRewardsAndGetRecipient(msg.sender);
        if (oldRewardRecipient != address(0)) {
            if (newRewardRecipient == address(0)) {
                optedInSupply -= uint128(balanceOf[msg.sender]);
            }
        } else if (newRewardRecipient != address(0)) {
            optedInSupply += uint128(balanceOf[msg.sender]);
        }
        userRewardInfo[msg.sender].rewardRecipient = newRewardRecipient;

        emit RewardRecipientSet(msg.sender, newRewardRecipient);
    }

    function claimRewards() external virtual notPaused returns (uint256 maxAmount) {
        if (
            !TIP403_REGISTRY.isAuthorized(transferPolicyId, address(this))
                || !TIP403_REGISTRY.isAuthorized(transferPolicyId, msg.sender)
        ) {
            revert PolicyForbids();
        }

        _updateRewardsAndGetRecipient(msg.sender);

        uint256 amount = userRewardInfo[msg.sender].rewardBalance;
        uint256 selfBalance = balanceOf[address(this)];
        maxAmount = (selfBalance > amount ? amount : selfBalance);
        userRewardInfo[msg.sender].rewardBalance -= maxAmount;

        balanceOf[address(this)] -= maxAmount;
        if (userRewardInfo[msg.sender].rewardRecipient != address(0)) {
            optedInSupply += uint128(maxAmount);
        }
        balanceOf[msg.sender] += maxAmount;

        emit Transfer(address(this), msg.sender, maxAmount);
    }

    /*//////////////////////////////////////////////////////////////
                        REWARD DISTRIBUTION VIEWS
    //////////////////////////////////////////////////////////////*/

    /// @notice Calculates the pending claimable rewards for an account without modifying state.
    /// @param account The address to query pending rewards for.
    /// @return pending The total pending claimable reward amount (stored balance + accrued pending rewards).
    function getPendingRewards(address account) external view returns (uint256 pending) {
        UserRewardInfo storage info = userRewardInfo[account];

        // Start with the stored reward balance
        pending = info.rewardBalance;

        // If this account is self-delegated, calculate pending rewards from their own holdings
        if (info.rewardRecipient == account) {
            uint256 holderBalance = balanceOf[account];
            if (holderBalance > 0) {
                uint256 rewardPerTokenDelta = globalRewardPerToken - info.rewardPerToken;
                if (rewardPerTokenDelta > 0) {
                    uint256 accrued = (holderBalance * rewardPerTokenDelta) / ACC_PRECISION;
                    pending += accrued;
                }
            }
        }
    }

    }

    RUST IMPLEMENTATION
    https://github.com/tempoxyz/tempo/tree/main
# Permit for TIP-20 ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/tips/tip-1004#abstract)
------------------------------------------------------------------

TIP-1004 adds EIP-2612 compatible `permit()` functionality to TIP-20 tokens, enabling gasless approvals via off-chain signatures. This allows users to approve token spending without submitting an on-chain transaction, with the approval being executed by any third party who submits the signed permit.

Motivation[](https://docs.tempo.xyz/protocol/tips/tip-1004#motivation)
----------------------------------------------------------------------

The standard ERC-20 approval flow requires users to submit a transaction to approve a spender before that spender can transfer tokens on their behalf. Among other things, this makes it difficult for a transaction to "sweep" tokens from multiple addresses that have never sent a transaction onchain.

EIP-2612 introduced the `permit()` function which allows approvals to be granted via a signed message rather than an on-chain transaction. This enables:

*   **Gasless approvals**: Users can sign a permit off-chain, and a relayer or the spender can submit the transaction
*   **Single-transaction flows**: DApps can batch the permit with the subsequent action (e.g., approve + swap) in one transaction
*   **Improved UX**: Users don't need to wait for or pay for a separate approval transaction

Since TIP-20 aims to be a superset of ERC-20 with additional functionality, adding EIP-2612 permit support ensures TIP-20 tokens work seamlessly with existing DeFi protocols and tooling that expect permit functionality.

### Alternatives[](https://docs.tempo.xyz/protocol/tips/tip-1004#alternatives)

While Tempo transactions provide solutions for most of the common problems that are solved by account abstraction, they do not provide a way to transfer tokens from an address that has never sent a transaction onchain, which means it does not provide an easy way for a batched transaction to "sweep" tokens from many addresses.

While we plan to have Permit2 deployed on the chain, it, too, requires an initial transaction from the address being transferred from.

Adding a function for `transferWithAuthorization`, which we are also considering, would also solve this problem. But `permit` is somewhat more flexible, and we think these functions are not mutually exclusive.

* * *

Specification
-------------

New functions[](https://docs.tempo.xyz/protocol/tips/tip-1004#new-functions)
----------------------------------------------------------------------------

The following functions are added to the TIP-20 interface:

```
interface ITIP20Permit {
    /// @notice Approves `spender` to spend `value` tokens on behalf of `owner` via a signed permit
    /// @param owner The address granting the approval
    /// @param spender The address being approved to spend tokens
    /// @param value The amount of tokens to approve
    /// @param deadline Unix timestamp after which the permit is no longer valid
    /// @param v The recovery byte of the signature
    /// @param r Half of the ECDSA signature pair
    /// @param s Half of the ECDSA signature pair
    /// @dev The permit is valid only if:
    ///      - The current block timestamp is <= deadline
    ///      - The signature is valid and was signed by `owner`
    ///      - The nonce in the signature matches the current nonce for `owner`
    ///      Upon successful execution, increments the nonce for `owner` by 1.
    ///      Emits an {Approval} event.
    function permit(
        address owner,
        address spender,
        uint256 value,
        uint256 deadline,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external;
 
    /// @notice Returns the current nonce for an address
    /// @param owner The address to query
    /// @return The current nonce, which must be included in any permit signature for this owner
    /// @dev The nonce starts at 0 and increments by 1 each time a permit is successfully used
    function nonces(address owner) external view returns (uint256);
 
    /// @notice Returns the EIP-712 domain separator for this token
    /// @return The domain separator bytes32 value
    /// @dev The domain separator is computed dynamically on each call as:
    ///      keccak256(abi.encode(
    ///          keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
    ///          keccak256(bytes(name())),
    ///          keccak256(bytes("1")),
    ///          block.chainid,
    ///          address(this)
    ///      ))
    ///      Dynamic computation ensures correct behavior after chain forks where chainId changes.
    function DOMAIN_SEPARATOR() external view returns (bytes32);
}
```


EIP-712 Typed Data[](https://docs.tempo.xyz/protocol/tips/tip-1004#eip-712-typed-data)
--------------------------------------------------------------------------------------

The permit signature must conform to EIP-712 typed structured data signing. The domain and message types are defined as follows:

### Domain Separator[](https://docs.tempo.xyz/protocol/tips/tip-1004#domain-separator)

The domain separator is computed using the following parameters:


|Parameter        |Value                                   |
|-----------------|----------------------------------------|
|name             |The token's name()                      |
|version          |"1"                                     |
|chainId          |The chain ID where the token is deployed|
|verifyingContract|The TIP-20 token contract address       |


```
bytes32 DOMAIN_SEPARATOR = keccak256(abi.encode(
    keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
    keccak256(bytes(name())),
    keccak256(bytes("1")),
    block.chainid,
    address(this)
));
```


### Permit Typehash[](https://docs.tempo.xyz/protocol/tips/tip-1004#permit-typehash)

The permit message type is:

```
bytes32 constant PERMIT_TYPEHASH = keccak256(
    "Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)"
);
```


### Signature Construction[](https://docs.tempo.xyz/protocol/tips/tip-1004#signature-construction)

To create a valid permit signature, the signer must sign the following EIP-712 digest:

```
bytes32 structHash = keccak256(abi.encode(
    PERMIT_TYPEHASH,
    owner,
    spender,
    value,
    nonces[owner],
    deadline
));
 
bytes32 digest = keccak256(abi.encodePacked(
    "\x19\x01",
    DOMAIN_SEPARATOR,
    structHash
));
```


The signature `(v, r, s)` must be produced by signing `digest` with the private key of `owner`.

Behavior[](https://docs.tempo.xyz/protocol/tips/tip-1004#behavior)
------------------------------------------------------------------

### Nonces[](https://docs.tempo.xyz/protocol/tips/tip-1004#nonces)

Each address has an associated nonce that:

*   Starts at `0` for all addresses
*   Increments by `1` each time a permit is successfully executed for that address
*   Must be included in the permit signature to prevent replay attacks

### Deadline[](https://docs.tempo.xyz/protocol/tips/tip-1004#deadline)

The `deadline` parameter is a Unix timestamp. The permit is only valid if `block.timestamp <= deadline`. This allows signers to limit the validity window of their permits.

### Pause State[](https://docs.tempo.xyz/protocol/tips/tip-1004#pause-state)

The `permit()` function follows the same pause behavior as `approve()`. Since setting an allowance does not move tokens, `permit()` is allowed to execute even when the token is paused.

### TIP-403 Transfer Policy[](https://docs.tempo.xyz/protocol/tips/tip-1004#tip-403-transfer-policy)

The `permit()` function does not perform TIP-403 authorization checks, consistent with the behavior of `approve()`. Transfer policy checks are only enforced when tokens are actually transferred.

### Signature Validation[](https://docs.tempo.xyz/protocol/tips/tip-1004#signature-validation)

The implementation must:

1.  Verify that `block.timestamp <= deadline`, otherwise revert with `PermitExpired`
2.  Attempt to validate the signature:
    *   First, use `ecrecover` to recover a signer address from the signature
    *   If `ecrecover` returns a non-zero address that equals `owner`, the signature is valid (EOA case)
    *   Otherwise, if `owner` has code, call `owner.isValidSignature(digest, signature)` per [EIP-1271](https://eips.ethereum.org/EIPS/eip-1271)
    *   If `isValidSignature` returns the magic value `0x1626ba7e`, the signature is valid (smart contract wallet case)
    *   Otherwise, revert with `InvalidSignature`
3.  Verify the nonce matches `nonces[owner]`
4.  Increment `nonces[owner]`
5.  Set `allowance[owner][spender] = value`
6.  Emit an `Approval(owner, spender, value)` event

### Smart Contract Wallet Support (EIP-1271)
[](https://docs.tempo.xyz/protocol/tips/tip-1004#smart-contract-wallet-support-eip-1271)

TIP-1004 supports permits signed by smart contract wallets via [EIP-1271](https://eips.ethereum.org/EIPS/eip-1271). When the `owner` address has code deployed, the implementation calls:

```
bytes4 constant EIP1271_MAGIC_VALUE = 0x1626ba7e;
 
// Pack signature for EIP-1271 call
bytes memory signature = abi.encodePacked(r, s, v);
 
// Call isValidSignature on the owner contract
(bool success, bytes memory result) = owner.staticcall(
    abi.encodeWithSelector(
        IERC1271.isValidSignature.selector,
        digest,
        signature
    )
);
 
// Signature is valid if call succeeds and returns magic value
bool isValid = success 
    && result.length == 32 
    && abi.decode(result, (bytes4)) == EIP1271_MAGIC_VALUE;
```


This enables multisigs, smart contract wallets (e.g., Safe, Argent), and account abstraction wallets to use gasless permits.

New errors[](https://docs.tempo.xyz/protocol/tips/tip-1004#new-errors)
----------------------------------------------------------------------

```
/// @notice The permit signature has expired (block.timestamp > deadline)
error PermitExpired();
 
/// @notice The permit signature is invalid (wrong signer, malformed, or zero address recovered)
error InvalidSignature();
```


New events[](https://docs.tempo.xyz/protocol/tips/tip-1004#new-events)
----------------------------------------------------------------------

None. Successful permit execution emits the existing `Approval` event from TIP-20.

* * *

Invariants
----------

*   `nonces(owner)` must only ever increase, never decrease
*   `nonces(owner)` must increment by exactly 1 on each successful `permit()` call for that owner
*   A permit signature can only be used once (enforced by nonce increment)
*   A permit with a deadline in the past must always revert
*   The recovered signer from a valid permit signature must exactly match the `owner` parameter
*   After a successful `permit(owner, spender, value, ...)`, `allowance(owner, spender)` must equal `value`
*   `DOMAIN_SEPARATOR()` must be computed dynamically and reflect the current `block.chainid`

Test Cases[](https://docs.tempo.xyz/protocol/tips/tip-1004#test-cases)
----------------------------------------------------------------------

The test suite must cover:

1.  **Happy path**: Valid permit sets allowance correctly
2.  **Expired permit**: Reverts with `PermitExpired` when `deadline < block.timestamp`
3.  **Invalid signature**: Reverts with `InvalidSignature` for malformed signatures
4.  **Wrong signer**: Reverts with `InvalidSignature` when signature is valid but signer ≠ owner
5.  **Replay protection**: Second use of same signature reverts (nonce already incremented)
6.  **Nonce tracking**: Verify nonce increments correctly after each permit
7.  **Zero address recovery**: Reverts with `InvalidSignature` if ecrecover returns zero address
8.  **Pause state**: Permit works when token is paused
9.  **Domain separator**: Verify correct EIP-712 domain separator computation
10.  **Domain separator chain ID**: Verify domain separator changes if chain ID changes
11.  **Max allowance**: Permit with `type(uint256).max` value works correctly
12.  **Allowance override**: Permit can override existing allowance (including to zero)
13.  **EIP-1271 smart contract wallet**: Permit works with smart contract wallet that implements `isValidSignature`
14.  **EIP-1271 rejection**: Reverts with `InvalidSignature` if smart contract wallet returns wrong magic value
15.  **EIP-1271 revert**: Reverts with `InvalidSignature` if `isValidSignature` call reverts
# Burn At for TIP-20 Tokens ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/tips/tip-1006#abstract)
------------------------------------------------------------------

This specification introduces a `burnAt` function to TIP-20 tokens, allowing holders of a new `BURN_AT_ROLE` to burn tokens from any address without transfer policy restrictions. This complements the existing `burnBlocked` function which is limited to burning from addresses blocked by the transfer policy.

Motivation[](https://docs.tempo.xyz/protocol/tips/tip-1006#motivation)
----------------------------------------------------------------------

The existing TIP-20 burn mechanisms have the following limitations:

1.  `burn()` - Only burns from the caller's own balance, requires `ISSUER_ROLE`
2.  `burnBlocked()` - Can burn from other addresses, but only if the target address is blocked by the transfer policy

There are legitimate use cases where token administrators may want a privileged caller to have the ability to burn tokens from any address regardless of their policy status, such as allowing a bridge contract to burn tokens that are being bridged out without requiring approval (as in the `crosschainBurn` function proposed in [ERC 7802](https://github.com/ethereum/ERCs/blob/master/ERCS/erc-7802.md)).

The `burnAt` function provides this capability with appropriate access controls via a dedicated role.

* * *

Specification
-------------

New Role[](https://docs.tempo.xyz/protocol/tips/tip-1006#new-role)
------------------------------------------------------------------

A new role constant is added to TIP-20:

```
bytes32 public constant BURN_AT_ROLE = keccak256("BURN_AT_ROLE");
```


This role is administered by the `DEFAULT_ADMIN_ROLE` (same as other TIP-20 roles).

New Event[](https://docs.tempo.xyz/protocol/tips/tip-1006#new-event)
--------------------------------------------------------------------

```
/// @notice Emitted when tokens are burned from any account.
/// @param from The address from which tokens were burned.
/// @param amount The amount of tokens burned.
event BurnAt(address indexed from, uint256 amount);
```


New Function[](https://docs.tempo.xyz/protocol/tips/tip-1006#new-function)
--------------------------------------------------------------------------

```
/// @notice Burns tokens from any account.
/// @dev Requires BURN_AT_ROLE. Cannot burn from protected precompile addresses.
/// @param from The address to burn tokens from.
/// @param amount The amount of tokens to burn.
function burnAt(address from, uint256 amount) external;
```


### Behavior[](https://docs.tempo.xyz/protocol/tips/tip-1006#behavior)

1.  **Access Control**: Reverts with `Unauthorized` if caller does not have `BURN_AT_ROLE`
2.  **Protected Addresses**: Reverts with `ProtectedAddress` if `from` is:
    *   `TIP_FEE_MANAGER_ADDRESS` (0xfeEC000000000000000000000000000000000000)
    *   `STABLECOIN_DEX_ADDRESS` (0xDEc0000000000000000000000000000000000000)
3.  **Balance Check**: Reverts with `InsufficientBalance` if `from` has insufficient balance
4.  **No Policy Check**: Unlike `burnBlocked`, this function does NOT check transfer policy authorization
5.  **State Changes**:
    *   Decrements `balanceOf[from]` by `amount`
    *   Decrements `_totalSupply` by `amount`
    *   Updates reward accounting if `from` is opted into rewards
6.  **Events**: Emits `Transfer(from, address(0), amount)` and `BurnAt(from, amount)`

### Interface Addition[](https://docs.tempo.xyz/protocol/tips/tip-1006#interface-addition)

The `ITIP20` interface is extended with:

```
/// @notice Returns the role identifier for burning tokens from any account.
/// @return The burn-at role identifier.
function BURN_AT_ROLE() external view returns (bytes32);
 
/// @notice Burns tokens from any account.
/// @param from The address to burn tokens from.
/// @param amount The amount of tokens to burn.
function burnAt(address from, uint256 amount) external;
```


Invariants
----------

1.  **Role Required**: `burnAt` must always revert if caller lacks `BURN_AT_ROLE`
2.  **Protected Addresses**: `burnAt` must never succeed when `from` is a protected precompile address
3.  **Supply Conservation**: After `burnAt(from, amount)`:
    *   `totalSupply` decreases by exactly `amount`
    *   `balanceOf[from]` decreases by exactly `amount`
4.  **Balance Constraint**: `burnAt` must revert if `amount > balanceOf[from]`
5.  **Reward Accounting**: If `from` is opted into rewards, `optedInSupply` must decrease by `amount`
6.  **Policy Independence**: `burnAt` must succeed regardless of transfer policy status of `from`

Test Cases[](https://docs.tempo.xyz/protocol/tips/tip-1006#test-cases)
----------------------------------------------------------------------

The test suite must verify:

1.  Successful burn with `BURN_AT_ROLE`
2.  Revert without `BURN_AT_ROLE` (Unauthorized)
3.  Revert when burning from `TIP_FEE_MANAGER_ADDRESS` (ProtectedAddress)
4.  Revert when burning from `STABLECOIN_DEX_ADDRESS` (ProtectedAddress)
5.  Successful burn from policy-blocked address (differs from `burnBlocked`)
6.  Revert on insufficient balance
7.  Correct event emissions (`Transfer` and `BurnAt`)
8.  Correct reward accounting updates
