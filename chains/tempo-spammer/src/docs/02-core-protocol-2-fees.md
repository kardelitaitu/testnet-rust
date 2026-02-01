Tempo has no native token. Instead, transaction fees—including both gas fees and priority fees—can be paid directly in stablecoins. When you send a transaction, you can choose which supported stablecoin to use for fees.

For a stablecoin to be accepted, it must be USD-denominated, issued as a native TIP-20 contract, and have sufficient liquidity on the native Fee AMM.

Tempo uses a fixed base fee (rather than a variable base fee as in EIP-1559), set so that a TIP-20 transfer costs less than $0.001. All fees accrue to the validator who proposes the block.

# Fees ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/fees/spec-fee#abstract)
------------------------------------------------------------------

This spec lays out how fees work on Tempo, including how fees are calculated, who pays them, and how the default fee token for a transaction is determined.

Motivation[](https://docs.tempo.xyz/protocol/fees/spec-fee#motivation)
----------------------------------------------------------------------

On Tempo, users can pay gas fees in any [TIP-20](https://docs.tempo.xyz/protocol/tip20/spec) token whose currency is USD, as long as that stablecoin has sufficient liquidity on the enshrined [fee AMM](https://docs.tempo.xyz/protocol/fees/spec-fee-amm) against the token that the current validator wants to receive.

In determining _which_ token a user pays fees in, we want to maximize customizability (so that wallets or users can implement more sophisticated UX than is possible at the protocol layer), minimize surprise (particularly surprises in which a user pays fees in a stablecoin they did not expect to), and have sane default behavior so that users can begin using basic functions like payments even using wallets that are not customized for Tempo support.

Fee units[](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-units)
--------------------------------------------------------------------

Fees in the `max_base_fee_per_gas` and `max_fee_per_gas` fields of transactions, as well as in the block's `base_fee_per_gas` field, are specified in units of **USD per 10^18 gas**. Since TIP-20 tokens have 6 decimal places, that means the fee for a transaction can be calculated as `ceil(base_fee * gas_used / 10^12)`.

This unit is chosen to provide sufficient precision for low-fee transactions. Since TIP-20 tokens have only 6 decimal places (as opposed to the 18 decimal places of ETH), expressing fees directly in tokens per gas would not provide enough precision for transactions with very low gas costs. By scaling the fee paid by 10^-12, the protocol ensures that even small fee amounts can be accurately represented and calculated.

Fee payment[](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-payment)
------------------------------------------------------------------------

Before the execution of each transaction, the protocol takes the following steps:

*   Determine the `[fee_payer](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-payer)` of the transaction.
*   Determine the `fee_token` of the transaction, according to the [rules for fee token preferences](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-token-preferences). If the fee token cannot be determined, the transaction is invalid.
*   Compute the `max_fee` of the transaction as `gas_limit * gas_price`.
*   Deduct `max_fee` from the `fee_payer`'s balance of `fee_token`. If `fee_payer` does not have sufficient balance in `fee_token`, the transaction is invalid.
*   Reserve `max_fee` of liquidity on the [fee AMM](https://docs.tempo.xyz/protocol/fees/spec-fee-amm) between the `fee_token` and the validator's preferred fee token. If there is insufficient liquidity, the transaction is invalid.

After the execution of each transaction:

*   Compute the `refund_amount` as `(gas_limit - gas_used) * gas_price`.
*   Credit the `fee_payer`'s address with `refund_amount` of `fee_token`.
*   Log a `Transfer` event from the user to the [fee manager contract](https://docs.tempo.xyz/protocol/fees/spec-fee-amm) for the net amount of the fee payment.

Fee payer[](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-payer)
--------------------------------------------------------------------

Tempo supports _sponsored transactions_ in which the `fee_payer` is a different address from the `tx.origin` of the transaction. This is supported by Tempo's [new transaction type](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction), which has a `fee_payer_signature` field.

If no `fee_payer_signature` is provided, then the `fee_payer` of the transaction is its sender (`tx.origin`).

If the `fee_payer_signature` field is set, then it is used to derive the `fee_payer` for the transaction, as described in the [transaction spec](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction).

For purposes of [fee token preferences](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-token-preferences), the `fee_payer` is the account that chooses the fee token.

Presence of the `fee_payer_signature` field authorizes a third party to pay the transaction's gas costs while the original sender executes the transaction logic.

#### Sender signs the transaction[](https://docs.tempo.xyz/protocol/fees/spec-fee#sender-signs-the-transaction)

The sender signs the transaction with their private key, signing over a blank fee token field. This means the sender delegates the choice of which fee token to use to the fee payer.

#### Fee payer selects and signs[](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-payer-selects-and-signs)

The fee payer selects which fee token to use, then signs over the transaction.

#### Transaction submission[](https://docs.tempo.xyz/protocol/fees/spec-fee#transaction-submission)

The fee token and fee payer signature is added to the transaction using the `fee_payer_signature` field and is then submitted.

#### Network validation[](https://docs.tempo.xyz/protocol/fees/spec-fee#network-validation)

The network validates both signatures and executes the transaction.

#### Validation[](https://docs.tempo.xyz/protocol/fees/spec-fee#validation)

When `feePayerSignature` is present:

*   Both sender and fee payer signatures must be valid
*   Fee payer must have sufficient balance in the fee token
*   Transaction is rejected if either signature fails or fee payer's balance is insufficient

Fee token preferences[](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-token-preferences)
--------------------------------------------------------------------------------------------

The protocol checks for token preferences in five ways, with this order of precedence:

1.  Transaction (set by the `fee_token` field of the transaction)
2.  Account (set on the FeeManager contract by the `fee_payer` of the transaction)
3.  TIP-20 contract (if the transaction is calling `transfer`, `transferWithMemo`, or `startReward` on a TIP-20 token contract, the transaction uses that token as its fee token)
4.  Stablecoin DEX (for certain swap calls, the transaction uses the `tokenIn` argument as its fee token)
5.  PathUSD (as a fallback)

The protocol checks preferences at each of these levels, stopping at the first one at which a preference is specified. At that level, the protocol performs the following checks. If any of the checks fail, the transaction is invalid (without looking at any further levels):

*   The token must be a TIP-20 token whose currency is USD.
*   The user must have sufficient balance in that token to pay the `gasLimit` on the transaction at the transaction's `gasPrice`.
*   There must be sufficient liquidity on the [fee AMM](https://docs.tempo.xyz/protocol/fees/spec-fee-amm), as discussed in that specification.

If no preference is specified at the transaction, account, or contract level, the protocol falls back to [pathUSD](https://docs.tempo.xyz/protocol/fees/spec-fee#pathusd).

### Transaction level[](https://docs.tempo.xyz/protocol/fees/spec-fee#transaction-level)

Tempo's [new transaction type](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction), allows transactions to specify a `fee_token` on the transaction. This overrides any preferences set at the account, contract, or validator level.

For [sponsored transactions](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-payer), the `tx.origin` address does not sign over the `fee_token` field (allowing the `fee_payer` to choose the fee token).

### Account level[](https://docs.tempo.xyz/protocol/fees/spec-fee#account-level)

An account can specify a fee token preference for all transactions for which it is the `fee_payer` (including both transactions it sponsors as well as non-sponsored transactions for which it is the `tx.origin`). This overrides any preference set at the contract or validator level.

To set its preference, the account can call the `setUserToken` function on the FeeManager precompile.

At this step, the protocol does one more check:

*   If the transaction is not a [Tempo transaction](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction) _and_ the transaction is a top-level call to the `setUserToken` function on the FeeManager, then the protocol checks the `token` argument to the function:
    *   If that token is a TIP-20 whose currency is USD, that token is used as the fee token (unless the transaction specifies a `fee_token` at the [transaction level](https://docs.tempo.xyz/protocol/fees/spec-fee#transaction-level)).
    *   If that token is not a TIP-20 or its currency is not USD, the transaction is invalid.

### TIP-20 contracts[](https://docs.tempo.xyz/protocol/fees/spec-fee#tip-20-contracts)

If the top-level call of a transaction is to one of the following functions on a TIP-20 token whose currency is USD:

*   `transfer(address to, uint256 amount)`
*   `transferWithMemo(address to, uint256 amount, bytes32 memo)`
*   `startReward(uint256 amount, uint32 seconds_)`

then that TIP-20 token is used as the user's fee token for that transaction (unless there is a preference specified at the [transaction](https://docs.tempo.xyz/protocol/fees/spec-fee#transaction-level) or [account](https://docs.tempo.xyz/protocol/fees/spec-fee#account-level) level).

For [Tempo transactions](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction), this rule applies only if _all_ top-level calls are to the same TIP-20 contract, and each such call is to one of the functions listed above, with `fee_payer == tx.origin`.

### Stablecoin DEX contract[](https://docs.tempo.xyz/protocol/fees/spec-fee#stablecoin-dex-contract)

If the top-level call of a transaction is to the [Stablecoin DEX](https://docs.tempo.xyz/protocol/exchange/spec) contract, the function being called is either `swapExactAmountIn` or `swapExactAmountOut`, and the `tokenIn` argument to that function is the address of a TIP-20 token for which the currency is USD, then the `tokenIn` argument is used as the user's fee token for the transaction (unless there is a preference specified at the [transaction](https://docs.tempo.xyz/protocol/fees/spec-fee#transaction-level) or [account](https://docs.tempo.xyz/protocol/fees/spec-fee#account-level) level).

For [Tempo transactions](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction), this rule applies only if there is only one top-level call in the transaction.

### pathUSD[](https://docs.tempo.xyz/protocol/fees/spec-fee#pathusd)

If no fee preference is set at the transaction, account, or contract level, the protocol falls back to [pathUSD](https://docs.tempo.xyz/protocol/exchange/pathUSD) as the user's fee token preference.

Validator preferences[](https://docs.tempo.xyz/protocol/fees/spec-fee#validator-preferences)
--------------------------------------------------------------------------------------------

Validators can set a default fee token preference that determines which stablecoin they receive for transaction fees. When users pay in different tokens, the Fee AMM automatically converts to the validator's preferred token.

### Setting validator preference[](https://docs.tempo.xyz/protocol/fees/spec-fee#setting-validator-preference)

To set their preference, validators call the `setValidatorToken` function on the FeeManager precompile:

```
// Set your preferred fee token
feeManager.setValidatorToken(preferredTokenAddress);
```


After setting a validator token preference, all fees collected in blocks the validator proposes will be automatically converted to the chosen token (if needed) and transferred to the validator's account.

On the Moderato testnet, validators currently expect alphaUSD (one of the tokens distributed by the faucet) as their fee token.

If validators have not specified a fee token preference, the protocol falls back to expecting pathUSD as their fee token.

### Removing validator preference[](https://docs.tempo.xyz/protocol/fees/spec-fee#removing-validator-preference)

To remove a validator token preference, set it to the zero address:

```
// Remove validator token preference
feeManager.setValidatorToken(address(0));
```


Fee lifecycle[](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-lifecycle)
----------------------------------------------------------------------------

This section describes the complete flow of how fees are collected, converted, and distributed from user to validator.

### Fee flow steps[](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-flow-steps)

When a user submits a transaction on Tempo, fees are paid in their chosen stablecoin (determined by the [fee token preferences](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-token-preferences) hierarchy). If the validator prefers a different stablecoin, the Fee AMM automatically converts the user's payment to the validator's preferred token.

#### 1\. User submits transaction[](https://docs.tempo.xyz/protocol/fees/spec-fee#1-user-submits-transaction)

The transaction is submitted with the fee token determined by the preference hierarchy.

#### 2\. Pre-transaction collection[](https://docs.tempo.xyz/protocol/fees/spec-fee#2-pre-transaction-collection)

Before the transaction executes, the `FeeManager` contract collects the maximum possible fee amount from the user:

*   Verifies the user has sufficient balance in their chosen fee token
*   Checks if the Fee AMM has enough liquidity (if conversion is needed)
*   Collects the maximum fee amount based on the transaction's gas limit

If either check fails, the transaction is rejected before execution.

#### 3\. Transaction execution[](https://docs.tempo.xyz/protocol/fees/spec-fee#3-transaction-execution)

The transaction executes normally. The actual gas consumed may be less than the maximum that was collected.

#### 4\. Post-transaction refund[](https://docs.tempo.xyz/protocol/fees/spec-fee#4-post-transaction-refund)

After execution, the `FeeManager`:

*   Calculates the actual fee owed based on gas used
*   Refunds any unused tokens to the user
*   Queues the actual fee amount for conversion (if needed)

#### 5\. Fee swap execution[](https://docs.tempo.xyz/protocol/fees/spec-fee#5-fee-swap-execution)

If the user's fee token differs from the validator's preferred token, the fee swap executes immediately during the post-transaction step at a fixed rate of **0.9970** (validator receives 0.9970 of their token per 1.0 user token paid).

If the user's fee token matches the validator's preference, no conversion is needed.

Fees accumulate in the FeeManager contract. Validators can claim their accumulated fees at any time by calling `distributeFees()`.

### Fee swap mechanics[](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-swap-mechanics)

Fee swaps always execute at a fixed rate of **0.9970**:

```
validatorTokenOut = userTokenIn × 0.9970

```


This means:

*   User pays 1.0 USDC for fees
*   Validator receives 0.9970 USDT (if that's their preferred token)
*   The 0.003 (0.3%) difference goes to liquidity providers as a fee

### Example flow[](https://docs.tempo.xyz/protocol/fees/spec-fee#example-flow)

Here's a complete example of the fee lifecycle:

1.  **Alice** wants to send a transaction and pays fees in **USDC** (her preferred token)
2.  **Validator** prefers to receive fees in **USDT**
3.  Alice's transaction has a max fee of 1.0 USDC
4.  The FeeManager collects 1.0 USDC from Alice before execution
5.  Transaction executes and uses 0.8 USDC worth of gas
6.  The FeeManager refunds 0.2 USDC to Alice
7.  The Fee AMM immediately swaps 0.8 USDC → 0.7976 USDT (0.8 × 0.9970)
8.  The 0.7976 USDT is added to the validator's accumulated fees
9.  Validator calls `distributeFees()` to claim their accumulated fees
10.  Liquidity providers earn 0.0024 USDT from the 0.3% fee

### Gas costs[](https://docs.tempo.xyz/protocol/fees/spec-fee#gas-costs)

The fee conversion process adds minimal overhead to transactions:

*   **Pre-transaction**: ~5,000 gas for balance and liquidity checks
*   **Post-transaction**: ~3,000 gas for refund and queue operations
*   **Block settlement**: Amortized across all transactions in the block

For complete technical specifications on the Fee AMM mechanism, see the [Fee AMM Protocol Specification](https://docs.tempo.xyz/protocol/fees/spec-fee-amm).


# Fee AMM Specification ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#abstract)
----------------------------------------------------------------------

This specification defines a system of one-way Automated Market Makers (AMMs) designed to facilitate gas fee payments from a user using one stablecoin (the `userToken`) to a validator who prefers a different stablecoin (the `validatorToken`). Each AMM handles fee swaps from a `userToken` to a `validatorToken` at one price (0.9970 `validatorToken` per `userToken`), and allows rebalancing in the other direction at another fixed price (1.0015 `userToken` per `validatorToken`).

Motivation[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#motivation)
--------------------------------------------------------------------------

Current blockchain fee systems typically require users to hold native tokens for gas payments. This creates friction for users who prefer to transact in stablecoins.

The Fee AMM is a dedicated AMM for trading between stablecoins, which can only be used by the protocol (and by arbitrageurs rebalancing it to keep it balanced). The protocol automatically collects fees in many different coins and immediately swaps them (paying a constant price) into the token preferred by the validator. Fees accumulate in the FeeManager, and validators can claim them on-demand.

The system is designed to minimize several forms of MEV:

*   **No Probabilistic MEV**: The fixed fee swap rate prevents profitable backrunning of fee swaps. There is no way to profitably spam the chain with transactions hoping an opportunity might arise.
*   **No Sandwich Attacks**: Fee swaps execute at a fixed rate, eliminating sandwich attack vectors.
*   **Top-of-Block Auction**: The main MEV in the AMM (from rebalancing) occurs as a single race at the top of the next block rather than creating probabilistic spam throughout.

Specification[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#specification)
--------------------------------------------------------------------------------

### Overview[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#overview)

The Fee AMM implements two distinct swap mechanisms:

1.  **Fee Swaps**: Fixed-rate swaps at a price of `0.9970` (validator token per user token) from `userToken` to `validatorToken`
2.  **Rebalancing Swaps**: Fixed-rate swaps at a price of `1.0015` (user token per validator token) from `validatorToken` to `userToken`

### Core Components[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#core-components)

#### 1\. FeeAMM Contract[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#1-feeamm-contract)

The primary AMM contract managing liquidity pools and swap operations.

##### Pool Structure[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#pool-structure)

```
struct Pool {
    uint128 reserveUserToken;           // Reserve of userToken
    uint128 reserveValidatorToken;      // Reserve of validatorToken
}
```


Each pool is directional: `userToken` → `validatorToken`. For a pair of tokens A and B, there are two separate pools:

*   Pool(A, B): for swapping A to B at fixed rate of 0.997 (fee swaps) and B to A at fixed rate of 0.9985 (rebalancing)
*   Pool(B, A): for swapping B to A at fixed rate of 0.997 (fee swaps) and A to B at fixed rate of 0.9985 (rebalancing)

##### Constants[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#constants)

*   `M = 9970` (scaled by 10000, representing 0.9970)
*   `N = 9985` (scaled by 10000, representing 0.9985)
*   `SCALE = 10000`
*   `MIN_LIQUIDITY = 1000`

##### Key Functions[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#key-functions)

```
function getPool(
    address userToken,
    address validatorToken
) external view returns (Pool memory)
```


Returns the pool structure for a given token pair.

```
function getPoolId(
    address userToken,
    address validatorToken
) external pure returns (bytes32)
```


Returns the pool ID for a given token pair (used internally for pool lookup).

```
function rebalanceSwap(
    address userToken,
    address validatorToken,
    uint256 amountOut,
    address to
) external returns (uint256 amountIn)
```


Executes rebalancing swaps from `validatorToken` to `userToken` at fixed rate of 1.0015 (user token per validator token). Can be executed by anyone. Calculates `amountIn = (amountOut * N) / SCALE + 1` (rounds up). Updates reserves immediately. Emits `RebalanceSwap` event.

```
function mint(
    address userToken,
    address validatorToken,
    uint256 amountUserToken,
    uint256 amountValidatorToken,
    address to
) external returns (uint256 liquidity)
```


Adds liquidity to a pool with both tokens. First provider sets initial reserves and must burn `MIN_LIQUIDITY` tokens. Subsequent providers must provide proportional amounts. Receives fungible LP tokens representing pro-rata share of pool reserves.

```
function mint(
    address userToken,
    address validatorToken,
    uint256 amountValidatorToken,
    address to
) external returns (uint256 liquidity)
```


Single-sided liquidity provision with validator token only. Treats the deposit as equivalent to performing a hypothetical `rebalanceSwap` first at rate `n = 0.9985` until the ratio of reserves match, then minting liquidity by depositing both. Formula: `liquidity = amountValidatorToken * _totalSupply / (V + n * U)`, where `n = N / SCALE`. Rounds down to avoid over-issuing LP tokens. Updates reserves by increasing only `validatorToken` by `amountValidatorToken`. Emits `Mint` event with `amountUserToken = 0`.

```
function burn(
    address userToken,
    address validatorToken,
    uint256 liquidity,
    address to
) external returns (uint256 amountUserToken, uint256 amountValidatorToken)
```


Burns LP tokens and receives pro-rata share of reserves. Emits `Burn` event.

```
function executeFeeSwap(
    address userToken,
    address validatorToken,
    uint256 amountIn
) internal returns (uint256 amountOut)
```


Executes a fee swap immediately. Calculates `amountOut = (amountIn * M) / SCALE`. Only executed by the protocol during transaction execution. Emits `FeeSwap` event. Note: `FeeSwap` events are not emitted for immediate swaps.

```
function checkSufficientLiquidity(
    address userToken,
    address validatorToken,
    uint256 maxAmount
) internal view
```


Verifies sufficient validator token reserves for the fee swap. Calculates `maxAmountOut = (maxAmount * M) / SCALE`. Reverts if insufficient liquidity.

#### 2\. FeeManager Contract[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#2-feemanager-contract)

Tempo introduces a precompiled contract, the `FeeManager`, at the address `0xfeec000000000000000000000000000000000000`.

The `FeeManager` is a singleton contract that implements all the functions of the Fee AMM for every pool. It handles the collection and refunding of fees during each transaction, executes fee swaps immediately, stores fee token preferences for users and validators, and accumulates fees for validators to claim via `distributeFees()`.

##### Key Functions[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#key-functions-1)

```
function setUserToken(address token) external
```


Sets the default fee token preference for the caller (user). Requires token to be a USD TIP-20 token. Emits `UserTokenSet` event. Access: Direct calls only (not via delegatecall).

```
function setValidatorToken(address token) external
```


Sets the fee token preference for the caller (validator). Requires token to be a USD TIP-20 token. Cannot be called during a block built by that validator. Emits `ValidatorTokenSet` event. Access: Direct calls only (not via delegatecall).

```
function collectFeePreTx(
    address user,
    address userToken,
    uint256 maxAmount
) external
```


Called by the protocol before transaction execution. The fee token (`userToken`) is determined by the protocol before calling using logic that considers: explicit tx fee token, setUserToken calls, stored user preference, tx.to if TIP-20. Reserves AMM liquidity if user token differs from validator token. Collects maximum possible fee from user. Access: Protocol only (`msg.sender == address(0)`).

```
function collectFeePostTx(
    address user,
    uint256 maxAmount,
    uint256 actualUsed,
    address userToken
) external
```


Called by the protocol after transaction execution. The validator token and fee recipient are inferred from `block.coinbase`. Calculates refund amount: `refundAmount = maxAmount - actualUsed`. Refunds unused tokens to user. If user token differs from validator token, executes the fee swap immediately and accumulates the output for the validator. Access: Protocol only (`msg.sender == address(0)`).

```
function distributeFees(address validator, address token) external
```


Allows anyone to trigger distribution of accumulated fees for a specific token to a validator. Transfers all accumulated fees in the specified token to the validator address. If no fees have accumulated for that token, this is a no-op.

```
function collectedFees(address validator, address token) external view returns (uint256)
```


Returns the amount of accumulated fees for a validator and specific token that can be claimed via `distributeFees()`.

### Swap Mechanisms[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#swap-mechanisms)

#### Fee Swaps[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#fee-swaps)

*   **Rate**: Fixed at m=0.9970 (validator receives 0.9970 of their preferred token per 1 user token that user pays)
*   **Direction**: User token to validator token
*   **Purpose**: Convert tokens paid by users as fees to tokens preferred by validators
*   **Settlement**: Immediate during transaction execution
*   **Access**: Protocol only

#### Rebalancing Swaps[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#rebalancing-swaps)

*   **Rate**: Fixed at n=0.9985 (swapper receives 1 of the user token for every 0.9985 that they put in of the validator's preferred token)
*   **Direction**: Validator token to user token
*   **Purpose**: Refill reserves of validator token in the pool
*   **Settlement**: Immediate
*   **Access**: Anyone

### Fee Collection Flow[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#fee-collection-flow)

1.  **Pre-Transaction**:
    
    *   Protocol determines user's fee token using logic that considers: explicit tx fee token, setUserToken calls, stored user preference, tx.to if TIP-20
    *   Protocol calculates maximum gas needed (`maxAmount = gasLimit * maxFeePerGas`)
    *   `FeeManager.collectFeePreTx(user, userToken, maxAmount)` is called:
        *   If user token differs from validator token, checks AMM has sufficient liquidity via `checkSufficientLiquidity()`
        *   Collects maximum fee from user using `transferFeePreTx()`
    *   If any check fails (insufficient balance, insufficient liquidity), transaction is invalid
2.  **Post-Transaction**:
    
    *   Calculate actual gas used (`actualUsed = gasUsed * gasPrice`)
    *   `FeeManager.collectFeePostTx(user, maxAmount, actualUsed, userToken)` is called:
        *   Validator token and fee recipient are inferred from `block.coinbase`
        *   Calculates refund: `refundAmount = maxAmount - actualUsed`
        *   Refunds unused tokens to user via `transferFeePostTx()`
        *   If user token differs from validator token and `actualUsed > 0`, executes fee swap immediately via `executeFeeSwap()`
        *   Accumulates swapped fees for the validator
3.  **Fee Distribution**:
    
    *   Validators (or anyone) can call `distributeFees(validator)` at any time to transfer accumulated fees to the validator

### Events[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#events)

```
event RebalanceSwap(
    address indexed userToken,
    address indexed validatorToken,
    address indexed swapper,
    uint256 amountIn,
    uint256 amountOut
)
event FeeSwap(
    address indexed userToken,
    address indexed validatorToken,
    uint256 amountIn,
    uint256 amountOut
)
event Mint(
    address indexed sender,
    address indexed userToken,
    address indexed validatorToken,
    uint256 amountUserToken,
    uint256 amountValidatorToken,
    uint256 liquidity
)
event Burn(
    address indexed sender,
    address indexed userToken,
    address indexed validatorToken,
    uint256 amountUserToken,
    uint256 amountValidatorToken,
    uint256 liquidity,
    address to
)
event UserTokenSet(address indexed user, address indexed token)
event ValidatorTokenSet(address indexed validator, address indexed token)
```


`Transfer` events are emitted as usual for transactions, with the exception of paying gas fees via TIP20 tokens. For fee payments, a single `Transfer` event is emitted post execution to represent the actual fee amount consumed (i.e. `gasUsed * gasPrice`).

### Gas[](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#gas)

Fee swaps are designed to be gas-free from the user perspective. The pre-tx and post-tx steps in each transaction do not cost any gas.

https://github.com/tempoxyz/tempo/tree/main/crates/precompiles/src/tip_fee_manager
# Pay Fees in Any Stablecoin ⋅ Tempo
Configure users to pay transaction fees in any supported stablecoin. Tempo's flexible fee system allows users to pay fees with the same token they're using, eliminating the need to hold a separate gas token.

Demo[](https://docs.tempo.xyz/guide/payments/pay-fees-in-any-stablecoin#demo)
-----------------------------------------------------------------------------

By the end of this guide you will be able to pay fees in any stablecoin on Tempo.

#### Pay Fees in Any Stablecoin

demo

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Send 100 AlphaUSD and pay fees in another token.

Quick Snippet[](https://docs.tempo.xyz/guide/payments/pay-fees-in-any-stablecoin#quick-snippet)
-----------------------------------------------------------------------------------------------

Using a custom fee token is as simple as passing a `feeToken` attribute to mutable actions like `useTransferSync`, `useSendTransactionSync`, and more.

Steps[](https://docs.tempo.xyz/guide/payments/pay-fees-in-any-stablecoin#steps)
-------------------------------------------------------------------------------

Recipes
-------

Set user fee token[](https://docs.tempo.xyz/guide/payments/pay-fees-in-any-stablecoin#set-user-fee-token)
---------------------------------------------------------------------------------------------------------

You can also set a persistent default fee token for an account, so users don't need to specify `feeToken` on every transaction. Learn more about fee token preferences [here](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-token-preferences).

Learning Resources[](https://docs.tempo.xyz/guide/payments/pay-fees-in-any-stablecoin#learning-resources)
---------------------------------------------------------------------------------------------------------
# Sponsor User Fees ⋅ Tempo
Enable gasless transactions by sponsoring transaction fees for your users. Tempo's native fee sponsorship allows applications to pay fees on behalf of users, improving UX and removing friction from payment flows.

Demo[](https://docs.tempo.xyz/guide/payments/sponsor-user-fees#demo)
--------------------------------------------------------------------

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Send 100 AlphaUSD with fees sponsored by the testnet fee payer.

Steps[](https://docs.tempo.xyz/guide/payments/sponsor-user-fees#steps)
----------------------------------------------------------------------

### Set up the fee payer service[](https://docs.tempo.xyz/guide/payments/sponsor-user-fees#set-up-the-fee-payer-service)

You can stand up a minimal fee payer service using the `Handler.feePayer` handler provided by the Tempo TypeScript SDK ([link](https://docs.tempo.xyz/sdk/typescript/server/handler.feePayer)). To sponsor transactions, you need a funded account that will act as the fee payer.

server.ts

```
import { ,  } from 'viem'
import {  } from 'viem/accounts'
import {  } from 'viem/chains'
 
const  = ({
  : .({
    : '0x20c0000000000000000000000000000000000001',
  }),
  : (),
})
 
const  = Handler.feePayer({
  : ('0x...'), 
  , 
}) 
 
const  = createServer(.listener)
.listen(3000)
```


### Configure your client to use the fee payer service[](https://docs.tempo.xyz/guide/payments/sponsor-user-fees#configure-your-client-to-use-the-fee-payer-service)

Use the `withFeePayer` transport provided by Viem ([link](https://viem.sh/tempo/transports/withFeePayer)). It routes transactions to the configured fee payer service for sponsorship when `feePayer: true` is requested on a transaction.

wagmi.config.ts

```
import {  } from 'viem/chains'
import {  } from 'viem/tempo'
import { ,  } from 'wagmi'
import { ,  } from 'wagmi/tempo'
 
export const  = ({
  : [
    ({
      : .(),
    }),
  ],
  : [],
  : false,
  : {
    [.]: ((), ('https://sponsor.moderato.tempo.xyz')),
  },
})
```


Now you can sponsor transactions by passing `feePayer: true` in the transaction parameters. For more details on how to send a transaction, see the [Send a payment](https://docs.tempo.xyz/guide/payments/send-a-payment) guide.

```
import {  } from 'wagmi/tempo'
import {  } from 'viem'
import {  } from 'viem/accounts'
 
const  = '0x20c0000000000000000000000000000000000001'
 
function () {
  const  = ('0x...')
  const  = ..() 
  const  = ..({
    : ,
  })
 
  return (
    < ={
      () => {
        .()
        const  = new (. as HTMLFormElement)
 
        const  = (.('recipient') ||
          '0x0000000000000000000000000000000000000000') as `0x${string}`
 
        .({ 
          : ('100', ..), 
          : true, 
          : , 
          : , 
        }) 
      }
    }>
      <>
        < ="recipient"> Recipient address </>
        < ="text" ="0x..." />
      </>
      < ="submit" ={.}>
        Send Payment
      </>
    </>
  )
}
```


Next Steps[](https://docs.tempo.xyz/guide/payments/sponsor-user-fees#next-steps)
--------------------------------------------------------------------------------

Now that you've implemented fee sponsorship, you can:

*   Learn more about the [Tempo Transaction](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#fee-payer-signature-details) type and fee payer signature details
*   Explore [Batch Transactions](https://docs.tempo.xyz/guide/use-accounts/batch-transactions) to sponsor multiple operations at once
*   Learn how to [Pay Fees in Any Stablecoin](https://docs.tempo.xyz/guide/payments/pay-fees-in-any-stablecoin)

Recipes[](https://docs.tempo.xyz/guide/payments/sponsor-user-fees#recipes)
--------------------------------------------------------------------------

The example above uses a fee payer server to sign and sponsor transactions. If you want to sponsor transactions locally, you can easily do so by passing a local account to the `feePayer` parameter.

client.ts

```
import { ,  } from 'viem'
import {  } from 'viem/accounts'
import {  } from 'viem/chains'
 
const  = ({
  : ,
  : (),
})
 
const {  } = await .token.transferSync({
  : parseUnits('10.5', 6),
  : '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEbb',
  : '0x20c0000000000000000000000000000000000000',
  : ('0x...'), 
})
```


:::

Best practices[](https://docs.tempo.xyz/guide/payments/sponsor-user-fees#best-practices)
----------------------------------------------------------------------------------------

1.  **Set sponsorship limits**: Implement daily or per-user limits to control costs
2.  **Monitor expenses**: Track sponsorship costs regularly to stay within budget
3.  **Consider selective sponsorship**: Only sponsor fees for specific operations or user segments
4.  **Educate users**: Clearly communicate when fees are being sponsored

### Security Considerations[](https://docs.tempo.xyz/guide/payments/sponsor-user-fees#security-considerations)

*   **Transaction-specific**: Fee payer signatures are tied to specific transactions
*   **No delegation risk**: Fee payer can't execute arbitrary transactions
*   **Balance checks**: Network verifies fee payer has sufficient balance
*   **Signature validation**: Both signatures must be valid

Learning Resources[](https://docs.tempo.xyz/guide/payments/sponsor-user-fees#learning-resources)
------------------------------------------------------------------------------------------------
# Use Your Stablecoin for Fees ⋅ Tempo
Enable users to pay transaction fees using your stablecoin. Tempo supports flexible fee payment options, allowing users to pay fees in any stablecoin they hold.

Demo[](https://docs.tempo.xyz/guide/issuance/use-for-fees#demo)
---------------------------------------------------------------

#### Use Your Stablecoin for Fees

demo

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Create & deploy a token to testnet.

4

Grant issuer role on token.

5

Mint 100 tokens to yourself.

6

Add fee liquidity for your token.

7

Send 100 AlphaUSD and pay fees in your token.

Steps[](https://docs.tempo.xyz/guide/issuance/use-for-fees#steps)
-----------------------------------------------------------------

### Create your stablecoin[](https://docs.tempo.xyz/guide/issuance/use-for-fees#create-your-stablecoin)

First, create and mint your stablecoin by following the [Create a Stablecoin](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin) guide.

### Add fee pool liquidity[](https://docs.tempo.xyz/guide/issuance/use-for-fees#add-fee-pool-liquidity)

Before users can pay fees with your token, you need to provide liquidity in the Fee AMM between your token and each of the tokens accepted by validators.

To determine which validator tokens are needed, sample recent blocks and check the miner's preferred fee token using `getValidatorToken` on the FeeManager contract. For example, on Moderato testnet, validators accept fees in pathUSD and AlphaUSD. On mainnet, this token mix is different and subject to change.

Add liquidity to your token's fee pool for each validator token:

```
import {  } from 'wagmi/tempo'
import {  } from 'viem'
import {  } from 'wagmi'
 
const {  } = ()
const  = '0x...' // Your issued token address
const  = '0x20c0000000000000000000000000000000000001' // AlphaUSD on testnet
 
const  = ..() 
 
// Add 100 AlphaUSD of liquidity to the fee pool
.({ 
  : ,
  : ,
  : ,
  : ,
  : ('100', 6),
}) 
```


You can also check your token's fee pool liquidity at any time:

```
import {  } from 'wagmi/tempo'
 
const { :  } = ..({
  : yourToken,
  : '0x20c0000000000000000000000000000000000001', // AlphaUSD on testnet
})
 
const  =  && . > 0n
```


If the pool has no liquidity (`reserveValidatorToken == 0`), you'll need to add liquidity to the fee pool before users can pay fees with your token. See the [Create a Stablecoin](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin) guide for instructions on minting fee AMM liquidity.

### Send payment with your token as fee[](https://docs.tempo.xyz/guide/issuance/use-for-fees#send-payment-with-your-token-as-fee)

Your users can send payments using your issued stablecoin as the fee token:

```
import React from 'react'
import {  } from 'wagmi/tempo'
import {  } from 'wagmi'
import { , , ,  } from 'viem'
 
export function () {
  const {  } = ()
  const [, ] = React.<string>('')
  const [, ] = React.<string>('')
 
  const  = '0x...' // Your issued token address
  const  = '0x20c0000000000000000000000000000000000001' // AlphaUSD
 
  const { : , :  } =
    ..({ 
      : , 
      : , 
    }) 
 
  const { : , :  } =
    ..({ 
      : , 
      : , 
    }) 
 
  const  = ..({ 
    : { 
      () { 
        () 
        () 
      }, 
    }, 
  }) 
 
  const  =  && ()
 
  const  = () => { 
    if (!) return
    .({ 
      : ('100', 6), 
      :  as `0x${string}`, 
      : , 
      :  ? ((), { : 32 }) : , 
      , // Pay fees with your issued token
    }) 
  } 
 
  return (
    <>
      <>
        <>Recipient address</>
        <
          ="text"
          ={}
          ={() => (..)}
          ="0x..."
        />
      </>
      <>
        <>Memo (optional)</>
        <
          ="text"
          ={}
          ={() => (..)}
          ="INV-12345"
        />
      </>
      <
        ={! || ! || .}
        ={}
        ="button"
      > 
        {. ? 'Sending...' : 'Send'}
      </> 
    </>
  )
}
```


Users can set your stablecoin as their default fee token at the account level, or specify it for individual transactions. Learn more about [how users pay fees in different stablecoins](https://docs.tempo.xyz/guide/payments/pay-fees-in-any-stablecoin).

How It Works[](https://docs.tempo.xyz/guide/issuance/use-for-fees#how-it-works)
-------------------------------------------------------------------------------

When users pay transaction fees with your stablecoin, Tempo's fee system automatically handles the conversion if validators prefer a different token. The [Fee AMM](https://docs.tempo.xyz/protocol/fees/spec-fee-amm) ensures seamless fee payments across all supported stablecoins.

Users can select your stablecoin as their fee token through:

*   **Account-level preference**: Set as default for all transactions
*   **Transaction-level preference**: Specify for individual transactions
*   **Automatic selection**: When directly interacting with your token contract

Learn more about [how users pay fees in different stablecoins](https://docs.tempo.xyz/guide/payments/pay-fees-in-any-stablecoin) and the complete [fee token preference hierarchy](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-token-preferences).

Benefits[](https://docs.tempo.xyz/guide/issuance/use-for-fees#benefits)
-----------------------------------------------------------------------

*   **User convenience**: Users can pay fees with the same token they're using
*   **Liquidity**: Encourages users to hold your stablecoin
*   **Flexibility**: Works seamlessly with Tempo's fee system

Best Practices[](https://docs.tempo.xyz/guide/issuance/use-for-fees#best-practices)
-----------------------------------------------------------------------------------

### Monitor pool liquidity[](https://docs.tempo.xyz/guide/issuance/use-for-fees#monitor-pool-liquidity)

Regularly check your token's fee pool reserves to ensure users can consistently pay fees with your stablecoin. Low liquidity can prevent transactions from being processed.

### Maintain adequate reserves[](https://docs.tempo.xyz/guide/issuance/use-for-fees#maintain-adequate-reserves)

Keep sufficient validator token reserves in your fee pool to handle expected transaction volume. Consider your user base size and typical transaction frequency when determining reserve levels.

As fees accrue in your token, the pool will run low on validator tokens and need to be rebalanced. Use `[rebalanceSwap](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#rebalance-liquidity)` to replenish validator token reserves when they become depleted.

### Test before launch[](https://docs.tempo.xyz/guide/issuance/use-for-fees#test-before-launch)

Before promoting fee payments with your token, thoroughly test the flow on testnet:

1.  Add liquidity to the fee pool
2.  Verify users can set your token as their fee preference
3.  Execute test transactions with various gas costs
4.  Monitor that fee conversions work correctly

Next Steps[](https://docs.tempo.xyz/guide/issuance/use-for-fees#next-steps)
---------------------------------------------------------------------------
# Managing Fee Liquidity ⋅ Tempo
The Fee AMM converts transaction fees between stablecoins when users pay in a different token than the validator prefers. This guide shows you how to add and remove liquidity to enable fee conversions.

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Create & deploy a token to testnet.

4

Add fee liquidity for your token.

5

View Fee AMM pool for your token.

6

Burn 10 LP tokens from your token pool.

Steps[](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#steps)
---------------------------------------------------------------------------------

### Check pool reserves[](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#check-pool-reserves)

Before adding liquidity, check the current pool reserves to understand the pool state.

```
import {  } from 'wagmi/tempo'
import {  } from 'viem'
 
const  = '0x20c0000000000000000000000000000000000002' // BetaUSD
const  = '0x20c0000000000000000000000000000000000001' // AlphaUSD
 
function () {
  const { :  } = ..({ 
    , 
    , 
  }) 
 
  return (
    <>
      <>User token reserves: {(?. ?? 0n, 6)}</>
      <>Validator token reserves: {(?. ?? 0n, 6)}</>
    </>
  )
}
```


### Add liquidity[](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#add-liquidity)

Add validator token to the pool to receive LP tokens representing your share. The first liquidity provider to a new pool must burn 1,000 units of liquidity. This costs approximately 0.002 USD and prevents attacks on pool reserves. Learn more in the [Fee AMM specification](https://docs.tempo.xyz/protocol/fees/spec-fee-amm).

```
import {  } from 'wagmi/tempo'
import { ,  } from 'viem'
import {  } from 'wagmi'
 
const  = '0x20c0000000000000000000000000000000000002' // BetaUSD
const  = '0x20c0000000000000000000000000000000000001' // AlphaUSD
 
function () {
  const {  } = ()
 
  const { :  } = ..({
    ,
    ,
  })
 
  const  = ..() 
 
  return (
    <>
      <>User token reserves: {(?. ?? 0n, 6)}</>
      <>Validator token reserves: {(?. ?? 0n, 6)}</>
      < ="button" ={() => { 
        if (!) return
        .({ 
          : , 
          : , 
          : ('100', 6), 
          : , 
          : , 
        }) 
      }}> 
        Add Liquidity 
      </> 
    </>
  )
}
```


### Check your LP balance[](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#check-your-lp-balance)

View your LP token balance to see your share of the pool.

```
import {  } from 'wagmi/tempo'
import { ,  } from 'viem'
import {  } from 'wagmi'
 
const  = '0x20c0000000000000000000000000000000000002' // BetaUSD
const  = '0x20c0000000000000000000000000000000000001' // AlphaUSD
 
function () {
  const {  } = ()
 
  const { :  } = ..({
    ,
    ,
  })
 
  const { :  } = ..({ 
    , 
    , 
    , 
  }) 
 
  const  = ..()
 
  return (
    <>
      <>LP token balance: {( ?? 0n, 6)}</>  
      <>User token reserves: {(?. ?? 0n, 6)}</>
      <>Validator token reserves: {(?. ?? 0n, 6)}</>
      < ="button" ={() => {
        if (!) return
        .({
          : ,
          : ,
          : ('100', 6),
          : ,
          : ,
        })
      }}>
        Add Liquidity
      </>
    </>
  )
}
```


### Remove liquidity[](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#remove-liquidity)

Burn LP tokens to withdraw your share of pool reserves plus accumulated fees.

```
import {  } from 'wagmi/tempo'
import { ,  } from 'viem'
import {  } from 'wagmi'
 
const  = '0x20c0000000000000000000000000000000000002' // BetaUSD
const  = '0x20c0000000000000000000000000000000000001' // AlphaUSD
 
function () {
  const {  } = ()
 
  const { :  } = ..({
    ,
    ,
  })
 
  const { :  } = ..({
    ,
    ,
    ,
  })
 
  const  = ..()
  const  = ..() 
 
  return (
    <>
      <>LP token balance: {( ?? 0n, 6)}</>
      <>User token reserves: {(?. ?? 0n, 6)}</>
      <>Validator token reserves: {(?. ?? 0n, 6)}</>
      < ="button" ={() => {
        if (!) return
        .({
          : ,
          : ,
          : ('100', 6),
          : ,
          : ,
        })
      }}>
        Add Liquidity
      </>
      < ="button" ={() => { 
        if (!) return
        .({ 
          , 
          , 
          : ('10', 6), // Burn 10 LP tokens
          : , 
        }) 
      }}> 
        Remove Liquidity 
      </> 
    </>
  )
}
```


Recipes[](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#recipes)
-------------------------------------------------------------------------------------

### Monitor pool utilization[](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#monitor-pool-utilization)

Track fee swap activity to understand pool utilization and revenue.

```
import * as React from 'react'
import {  } from 'wagmi/tempo'
import {  } from 'viem'
 
const  = '0x20c0000000000000000000000000000000000002' // BetaUSD
const  = '0x20c0000000000000000000000000000000000001' // AlphaUSD
 
function () {
  const [, ] = React.<any[]>([])
 
  ..useWatchFeeSwap({ 
    , 
    , 
    () { 
      for (const  of ) { 
        (() => [..., { 
          : (.args.amountIn, 6), 
          : (.args.amountOut, 6), 
          : (.args.amountIn * 30n / 10000n, 6), 
        }]) 
      } 
    }, 
  }) 
 
  return (
    <>
      {.((, ) => (
        < ={}>
          Swap: {.amountIn} → {.amountOut} (LP revenue: {.revenue})
        </>
      ))}
    </>
  )
}
```


### Rebalance pools[](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#rebalance-pools)

You can rebalance pools by swapping validator tokens for accumulated user tokens at a fixed rate. Rebalancing restores validator token reserves and enables continued fee conversions. Learn more [here](https://docs.tempo.xyz/protocol/fees/spec-fee-amm#swap-mechanisms).

```
import {  } from 'wagmi/tempo'
import { ,  } from 'viem'
import {  } from 'wagmi'
 
const  = '0x20c0000000000000000000000000000000000002' // BetaUSD
const  = '0x20c0000000000000000000000000000000000001' // AlphaUSD
 
function () {
  const {  } = ()
 
  const { :  } = ..({
    ,
    ,
  })
 
  const  = ..() 
 
  return (
    <>
      <>User token reserves: {(?. ?? 0n, 6)}</>
      <>Validator token reserves: {(?. ?? 0n, 6)}</>
      < ="button" ={() => { 
        if (! || !) return
        // Swap validator token for user token at 0.9985 rate
        .({ 
          , 
          , 
          : ., // Amount of user token to receive
          : , 
        }) 
      }}> 
        Rebalance 
      </> 
    </>
  )
}
```


Best Practices[](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#best-practices)
---------------------------------------------------------------------------------------------------

### Monitor pool reserves[](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#monitor-pool-reserves)

Regularly check pool reserves to ensure sufficient liquidity for fee conversions. Low reserves can prevent transactions from being processed.

Add liquidity when:

*   Transaction rates increase for a given `userToken`
*   Reserve levels drop below expected daily volume
*   Multiple validators begin preferring the same token

### Maintain adequate reserves[](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#maintain-adequate-reserves)

As an issuer, keep sufficient validator token reserves to handle expected transaction volume. Consider your anticipated fee conversion volume when determining reserve levels.

For new token pairs, provide the entire initial amount in the validator token. The pool naturally accumulates user tokens as fees are paid.

### Deploy liquidity strategically[](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#deploy-liquidity-strategically)

Focus liquidity on pools with:

*   High transaction volume and frequent fee conversions
*   New stablecoins that need initial bootstrapping
*   Validator tokens preferred by multiple validators

Learning Resources[](https://docs.tempo.xyz/guide/stablecoin-dex/managing-fee-liquidity#learning-resources)
-----------------------------------------------------------------------------------------------------------
# Fee Token Introspection ⋅ Tempo
TIP-1007: Fee Token Introspection
---------------------------------

Abstract[](https://docs.tempo.xyz/protocol/tips/tip-1007#abstract)
------------------------------------------------------------------

TIP-1007 adds a `getFeeToken()` view function to the FeeManager precompile that returns the fee token address being used for the current transaction. This enables smart contracts to introspect which TIP-20 token is paying for gas fees during execution, allowing for dynamic logic based on the fee token choice.

Motivation[](https://docs.tempo.xyz/protocol/tips/tip-1007#motivation)
----------------------------------------------------------------------

Tempo transactions support paying gas fees in any USD-denominated TIP-20 token via the fee token preference system. However, prior to this TIP, there was no way for a smart contract to determine which fee token is being used for the current transaction during execution.

This capability was requested by a partner. It could be useful for contracts that want to:

*   Adjust their internal logic based on which fee token is being used
*   Provide fee token-aware pricing or routing decisions
*   Emit events or logs that include the fee token for off-chain indexing
*   Implement fee token-specific behavior in cross-chain messaging

* * *

Specification
-------------

New Function[](https://docs.tempo.xyz/protocol/tips/tip-1007#new-function)
--------------------------------------------------------------------------

The following function is added to the `IFeeManager` interface:

```
interface IFeeManager {
    // ... existing functions ...
 
    /// @notice Returns the fee token being used for the current transaction
    /// @return The address of the TIP-20 token paying for gas fees
    /// @dev This value is set by the protocol before transaction execution begins.
    ///      Returns address(0) if no fee token has been set (e.g., in eth_call
    ///      simulations where the transaction handler does not run).
    function getFeeToken() external view returns (address);
}
```


Behavior[](https://docs.tempo.xyz/protocol/tips/tip-1007#behavior)
------------------------------------------------------------------

### Fee Token Resolution[](https://docs.tempo.xyz/protocol/tips/tip-1007#fee-token-resolution)

The fee token returned by `getFeeToken()` is the same token that was resolved by the protocol during transaction validation, following the [fee token preference rules](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-token-resolution).

### Storage[](https://docs.tempo.xyz/protocol/tips/tip-1007#storage)

The fee token is stored in **transient storage** (EIP-1153) within the FeeManager precompile. This means:

*   The value is automatically cleared at the end of each transaction
*   No persistent storage writes occur, minimizing gas costs
*   The value is consistent across all calls within a transaction (including internal calls and subcalls)

### Timing[](https://docs.tempo.xyz/protocol/tips/tip-1007#timing)

The fee token is set by the protocol in the `validate_against_state_and_deduct_caller` handler phase, before any user code executes. This ensures the value is available throughout the entire transaction execution.

### Gas Cost[](https://docs.tempo.xyz/protocol/tips/tip-1007#gas-cost)

Reading the fee token costs the standard warm transient storage read cost (100 gas for TLOAD). This is the cost of calling `getFeeToken()` itself; callers should account for additional gas used by the CALL opcode to invoke the precompile.

### Edge Cases[](https://docs.tempo.xyz/protocol/tips/tip-1007#edge-cases)


|Scenario                         |Return Value                             |
|---------------------------------|-----------------------------------------|
|Normal transaction               |The resolved fee token address           |
|Free transaction (zero gas price)|The resolved fee token (may still be set)|
|eth_call simulation              |address(0) (no transaction context)      |


The only case where `address(0)` is returned is in simulation contexts (e.g., `eth_call`) where the protocol handler does not execute.

Example Usage[](https://docs.tempo.xyz/protocol/tips/tip-1007#example-usage)
----------------------------------------------------------------------------

```
import { IFeeManager } from "./interfaces/IFeeManager.sol";
 
contract FeeTokenAware {
    IFeeManager constant FEE_MANAGER = IFeeManager(0xfeeC000000000000000000000000000000000000);
    address constant PATH_USD = 0x20C0000000000000000000000000000000000000;
 
    function doSomething() external {
        address feeToken = FEE_MANAGER.getFeeToken();
 
        if (feeToken == PATH_USD) {
            // User is paying fees in pathUSD
        } else if (feeToken != address(0)) {
            // User is paying fees in a different USD stablecoin
        } else {
            // No fee token context (e.g., eth_call simulation)
        }
    }
}
```


Interface Addition[](https://docs.tempo.xyz/protocol/tips/tip-1007#interface-addition)
--------------------------------------------------------------------------------------

The following function is added to `IFeeManager`:

```
/// @notice Returns the fee token being used for the current transaction
/// @return The address of the TIP-20 token paying for gas fees
function getFeeToken() external view returns (address);
```


* * *

Invariants
----------

*   `getFeeToken()` must return a consistent value across all calls within the same transaction
*   `getFeeToken()` must return `address(0)` in simulation contexts (e.g., `eth_call`) where no transaction handler runs
*   `getFeeToken()` must be callable from `staticcall` contexts without reverting
*   The fee token returned must match the token used for actual fee deduction in `collectFeePreTx` and `collectFeePostTx`
*   Reading the fee token must not modify any state (view function)

Test Cases[](https://docs.tempo.xyz/protocol/tips/tip-1007#test-cases)
----------------------------------------------------------------------

The test suite must cover:

1.  **Basic functionality**: `getFeeToken()` returns the correct fee token address
2.  **Zero when unset**: Returns `address(0)` when no fee token is set
3.  **Consistency**: Same value returned from nested calls within a transaction
4.  **Static call safety**: Works correctly when called via `staticcall`
5.  **Transient storage**: Value is cleared between transactions
6.  **Different fee tokens**: Works with various TIP-20 fee tokens (pathUSD, USDC, etc.)
7.  **Dispatch coverage**: Function selector is correctly dispatched by the precompile
