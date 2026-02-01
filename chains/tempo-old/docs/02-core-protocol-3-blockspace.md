# Blockspace Overview ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/blockspace/overview#abstract)
------------------------------------------------------------------------

This specification defines the structure of valid blocks in the Tempo blockchain.

Motivation[](https://docs.tempo.xyz/protocol/blockspace/overview#motivation)
----------------------------------------------------------------------------

Tempo blocks extend the Ethereum block format in multiple ways: there are new header fields to account for payment lanes and sub-blocks, and system transactions are added to the block body for the fee AMM and other protocol operations. This specification contains all the modifications to the block format.

Specification[](https://docs.tempo.xyz/protocol/blockspace/overview#specification)
----------------------------------------------------------------------------------

Tempo extends an Ethereum header with three extra scalars.

Header struct

```
pub struct Header {
    pub general_gas_limit: u64,
    pub shared_gas_limit: u64,
    pub timestamp_millis_part: u64,
    pub inner: Header,
}
```


*   `inner` is the canonical Ethereum header (parent\_hash, state\_root, gas\_limit, etc.).
*   `general_gas_limit` and `shared_gas_limit` carve up the canonical `gas_limit` for payment and sub-block gas (see [payment lane specification](https://docs.tempo.xyz/protocol/blockspace/payment-lane-specification) and [sub-block specification](https://docs.tempo.xyz/protocol/blockspace/sub-block-specification)).
*   `timestamp_millis_part` stores the sub‑second component; the full timestamp is `inner.timestamp * 1000 + timestamp_millis_part` .

### Block body[](https://docs.tempo.xyz/protocol/blockspace/overview#block-body)

The block body in Tempo retains the canonical Ethereum block body structure, with the addition of system transactions. Transactions are ordered in the following sections:

1.  Start-of-block system transaction(s) (must begin with the rewards registry call).
2.  Proposer lane transactions, subject to `general_gas_limit` on non-payment transactions.
3.  Sub-block transactions, grouped by proposer and prefixed with the reserved nonce key.
4.  Gas incentive transactions that consume leftover shared gas.
5.  End-of-block system transactions (see below).

### System transactions[](https://docs.tempo.xyz/protocol/blockspace/overview#system-transactions)

A valid tempo block must contain the following system transactions:

*   **Rewards Registry (start-of-block)** — must be the first transaction in the block body; refreshes validator rewards metadata before user transactions begin. Detailed specification [here](https://docs.tempo.xyz/protocol/tip20-rewards/spec).
*   **Subblock Metadata (end-of-block)** — contains metadata about the sub-blocks included in the block. Detailed specification [here](https://docs.tempo.xyz/protocol/blockspace/sub-block-specification).


# Payment Lane Specification ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/blockspace/payment-lane-specification#abstract)
------------------------------------------------------------------------------------------

This specification introduces a second consensus gas constraint for **non-payment** transactions. Transactions are classified as either payments or non-payments based solely on their transaction data, without requiring any access to blockchain state. For a block to be valid, total `gas_used` by the block must be less than the `gas_limit`. Non-payment transactions executed in the proposer's lane (i.e. before the gas incentive section) must consume at most `general_gas_limit`, a new field added to the header. Once that budget is exhausted, any additional inclusion must come via the gas incentive lane defined in the [sub-blocks specification](https://docs.tempo.xyz/protocol/blockspace/sub-block-specification).

Motivation[](https://docs.tempo.xyz/protocol/blockspace/payment-lane-specification#motivation)
----------------------------------------------------------------------------------------------

Tempo ensures that payment transactions always have available blockspace, even during periods of high network congestion from DeFi activity or complex smart contracts. No action is required by the user to take advantage of this feature.

This is achieved through **separate gas limits** for payment and non-payment transactions. When blocks are constructed, validators enforce two separate gas constraints:

1.  **`gas_limit`** — The total gas available for all transactions (standard Ethereum behavior)
2.  **`general_gas_limit`** — The maximum gas that non-payment transactions can consume

Non-payment transactions in the proposer's lane can only fill up to `general_gas_limit`, payment transactions can still use the remaining capacity up.

Terminology[](https://docs.tempo.xyz/protocol/blockspace/payment-lane-specification#terminology)
------------------------------------------------------------------------------------------------

*   **Payment transaction:** Determined by a function, `is_payment(tx) -> bool`. This function returns true if the transaction is a payment transaction, false otherwise.
*   **Non-payment transaction:** `!is_payment(tx)`.

Specification[](https://docs.tempo.xyz/protocol/blockspace/payment-lane-specification#specification)
----------------------------------------------------------------------------------------------------

### 1\. Transaction classification[](https://docs.tempo.xyz/protocol/blockspace/payment-lane-specification#1-transaction-classification)

A transaction is classified as a **payment transaction** when:

1.  the recipient address (`tx.to`) starts with the TIP-20 payment prefix `0x20c0000000000000000000000000`, or,
2.  for TempoTransactions, every entry in `tx.calls` targets an address starting with the TIP-20 payment prefix `0x20c0000000000000000000000000`.

This classification is performed entirely on the transaction payload, no account state is consulted.

### 2\. Ordering of Transactions[](https://docs.tempo.xyz/protocol/blockspace/payment-lane-specification#2-ordering-of-transactions)

The specification does not require any specific ordering of transactions: payment and non-payment transactions can be intermixed.

### 3\. Gas accounting & validity (consensus)
[](https://docs.tempo.xyz/protocol/blockspace/payment-lane-specification#3-gas-accounting--validity-consensus)

Validity of a block requires that,

```
general_gas_limit >= Σ gas_consumed(tx[i])   
for all i such that !is_payment(tx[i]) and tx[i] is in the proposer's lane

```


Where `gas_consumed` includes intrinsic gas and gas burned by reverts, as in the existing protocol.


Sub-block Specification
Abstract
This proposal allows non-proposing validators to propose a limited set of transactions in each block through signed sub-blocks. Sub-blocks are sent directly to the main proposer and their transactions are included in the block as described below. Consensus does not enforce inclusion. The proposer is incentivized to include sub-blocks by provisioning additional gas upon sub-block inclusion, which permits them to include additional transactions at the bottom of the block as described below.

Motivation
In traditional blockchains, only the current block proposer can include transactions. This means validators must wait for their scheduled slot to provide fast inclusion for their users. Tempo changes this by letting all validators contribute transactions to every block through sub-blocks.

For validators, this is good as they no longer have to wait for their turn as proposer to provide low-latency inclusion for their users. They have guaranteed access to blockspace in every block, allowing them to include transactions whenever needed. Validators can also ensure a specific transaction execution order within their sub-block, giving them controlled ordering of their transactions.

For users, this is good because transactions can be included faster since they can go through any validator, not just the current proposer. Access to blockspace becomes more predictable as it is smoothed across all validators rather than being concentrated with a single proposer. Time-sensitive transactions benefit from lower latency and can be included more quickly.

This proposal smooths access to blockspace across validators. It enables every validator to provide low-latency inclusion for themselves, their users, or their partners, without waiting for their turn as proposer.

Specification
This specification describes the process in temporal order.

0. Definitions
The gas limit of the whole block is G. There are n validators: 1 proposer and n-1 other non-proposers.
f fraction of the gas limit of the block, 0 < f < 1 is reserved for the main proposer.
1. Sub-blocks
Each validator can construct a sub-block. Sub-blocks follow this structure:
sub-block = rlp([version, parent_hash, fee_recipient, [transactions], signature])

where:

version = 1,
parent_hash is the parent hash of the previous block.
fee_recipient is the EOA at which this validator wants to receive the fees included in this block.
[transactions] is an ordered list of transactions. Transactions in a sub-block must satisfy additional conditions described below in Section 1.1. We explicitly allow for this list to be empty: a validator with no transactions to propose may still send a sub-block so that the proposer gets extra gas for the gas incentive region, described below.
The signature field is the validator signing over a hash computed as
keccak256(magic_byte || rlp([version, parent_hash, fee_recipient, [transactions]])), where magic_byte = 0x78, The signature ensures that this sub-block is valid only for the declared slot, and that the proposer cannot alter the order or set of transactions included in a sub-block.
The validator sends this sub-block directly to the next proposer.
For each validator i, define

unreservedGas[i] = (1 - f) * G / n - Σ(gasLimit of transactions in sub-block[i])

1.1 Sub-block Transactions
We use the two-dimensional nonce sequence to simplify transaction validity. In this construction, the nonce_key is a u256 value.

Let validatorPubKey be the public key of the validator proposing a given sub-block. Let validatorPubKey120 be the most significant 120 bits of the validator's public key.

We reserve sequence keys to each validator by requiring that the first (most significant) byte of the sequence key is the constant byte 0x5b, and the next 15 bytes (120 bits) encode validatorPubKey120.

Formally, we require that:

The sequence key of any transaction in the sub-block is of the form (0x5b << 248) + (validatorPubKey120 << 128) + x, where x is a value between 0 and 2**128 - 1. In other words, the most significant byte of the sequence key is always 0x5b, the next 15 bytes are the most significant 120 bits of the validator's public key, and the final 32 bytes (128 bits) still allow for 2D-nonces.

No two validators share the same validatorPubKey120; each validator's reserved space is distinct.

This explicit prefixing with 0x5b ensures the reserved sequence key space is unambiguous and disjoint across validators. Sub-block proposers control all sequence keys of the form above, and can ensure that nonces are sequential within their space.

Reserved Nonce Space

To prevent transaction conflicts, each validator has a reserved nonce space. Transactions in sub-blocks use special nonce sequence keys that identify which validator proposed them. This ensures that validators can't interfere with each other's transactions.

Further, we require that sub-block transactions are signed solely by the root EOA key of the address sending the transaction.

2. Block Construction
The proposer collects sub-blocks from other validators. It now constructs a block with the following contents:

transactions = [list of own transactions] | [sub-block transactions] | [gas incentive transactions]

list of own transactions are regular transactions from the proposer with f * G gas limit.
sub-block transactions are transactions from the included sub-blocks. This includes a sub-block from the proposer itself if the proposer desires. Nonce sequence keys with prefix 0x5b should only appear in this section.
gas incentive transactions are additional transactions that the proposer can include after the sub-block transactions, with additional gas defined below.
We have the following new header field:

shared_gas_limit  // The total gas limit allocated for the sub-blocks and gas incentive transactions

2.1 System transaction
The block includes a new system transaction, whose call data contains, for each included sub-block, the public key of the validator proposing, the feeRecipient for that sub-block and the signature of the sub-block. It is a no-op, and is there for execution layer blocks to be self-contained/carry all context.

Field	Value / Requirement	Notes / Validation
Type	Legacy transaction	
Position in Block	Last transaction	Block is invalid if absent.
From (sender)	0x0000000000000000000000000000000000000000	Zero address
To (recipient)	0x0000000000000000000000000000000000000000	No-op
Calldata	rlp([[version, validator_pubkey, fee_recipient, signature], ...])	Sub-block version (currently = 1), each included sub-block's validator public key, fee_recipient, and signature.
Value	0	No native token transfer.
Nonce	0	
Gas Limit	0	Does not contribute to block gas accounting.
Gas Price	0	Independent of block base fee; does not pay fees.
Signature	r = 0, s = 0, yParity = false	Empty signature designates system transaction.
3. Proposer Behavior
Construct Main Block in the usual way.
Collect sub-blocks from validators, including from self. Verify signatures and gas bounds of sub-blocks. Skip (i.e., do not include) invalid or missing sub-blocks; include valid ones. Transactions from a sub-block must be contiguous in the block, but sub-blocks can be included in any order.
Compute proposer Gas Incentive section limit:
gasIncentiveLimit =  Σ(unreservedGas[i]) for all included sub-blocks [i]

Append transactions at the bottom up to this gas limit.
Construct and include the system transaction at the bottom of the block.
3.1 Proposer Incentives
We do not enforce censorship-resistance for the transactions at consensus layer.
Proposer is incentivized by additional gas from sub-blocks included and reciprocity.
Additional gas is unrestricted so it could include backruns etc from sub-block transactions.
4. Block Validity Rules:
We can now define what a valid block is:

Gas Limits:
[list of own transactions] uses gas at most f * G.
[sub-block transactions]: the sum of gas_limits of all transactions in each sub-block is lower than the per-sub block gas limit: Σ(gasLimit of transactions in sub-block[i]) <= (1-f) * G / n.
[gas incentive transactions] use total gas <= gasIncentiveLimit.
General transactions gas limit from payments lane spec applies to [list of own transactions].
Transactions with nonce sequence key prefix 0x5b appear only in the [sub-block transactions]. Transactions are contiguous by validator. The [list of own transactions] and [gas incentive transactions] can use any un-reserved sequence key.
System transaction is present, in the correct position, and valid (matches contents of the block).
Transactions in the main proposers's section and the gas incentive section are valid in the standard way (signature, nonce, can pay fees).
4.1 Failures for Sub-block Transactions
Even if a transaction can pay its fees when the sub-block is created (i.e., when the sub-block is sent to the proposer), it may not be able to pay its fees when the sub-block is included and the block is processed. Here is a list of possible scenarios:

Fee Failure Scenarios:
The Fee AMM liquidity for the user's fee_token is insufficient (e.g., it was used up by previous transactions in the block).
The user's balance of the fee_token is insufficient (e.g., the user spent that balance in previous transactions in the block).
The user or validator changed their fee_token preference in the block and the transaction cannot pay its fees because of the new preference.
In all these scenarios, transaction is considered valid, increments the nonce, skips fee payment and execution, and results in an exceptional halt.

5. Transaction Fee Accounting
The fee manager is updated to handle fee accounting across sub-blocks:

For the main proposer transactions, fees are paid to the main proposer's fee_recipient as usual.
For the sub-block transactions, fees are paid to the fee_recipient of the sub-block (available from the system transaction).
For the gas incentive transactions, fees are paid to the proposer's fee_recipient.
In all cases, the fee is paid in the preferred fee_token of the fee_recipient, using liquidity from the fee AMM as necessary (i.e., validatorTokens[fee_recipient] from the FeeManager contract). If the fee_recipient has not set a preferred fee_token, then we use pathUSD as a fallback.
# State Creation Cost Increase ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/tips/tip-1000#abstract)
------------------------------------------------------------------

This TIP increases the gas cost for creating new state elements, accounts, and contract code to provide economic protection against state growth spam attacks. The proposal increases the cost of writing a new state element from 20,000 gas to 250,000 gas, introduces a 250,000 gas charge for account creation (when the account's nonce is first written), and implements a new contract creation cost model: 1,000 gas per byte of contract code plus 500,000 gas for keccak hash and codesize fields.

Motivation[](https://docs.tempo.xyz/protocol/tips/tip-1000#motivation)
----------------------------------------------------------------------

Tempo's high throughput capability (approximately 20,000 transactions per second) creates a vulnerability where an adversary could create a massive amount of state with the intent of permanently slowing the chain down. If each transaction is used to create a new account, and each account requires approximately 200 bytes of storage, then over 120 TB of storage could be created in a single year. Even if this storage is technically feasible, the database performance implications are unknown and would likely require significant R&D on state management much earlier than needed for business requirements.

The current EVM gas schedule charges 20,000 gas for writing a new state element and has no cost for creating an account. This makes state creation attacks economically viable for adversaries. By increasing these costs to 250,000 gas each, we create a meaningful economic barrier: creating 1 TB of state would cost approximately $50 million, and creating 10 TB would cost approximately $500 million (based on the assumption that a TIP-20 transfer costs 50,000 gas = 0.1 cent, implying 1 cent per 500,000 gas).

### Alternatives Considered[](https://docs.tempo.xyz/protocol/tips/tip-1000#alternatives-considered)

1.  **Storage rent**: Implementing a periodic fee for holding state. This was rejected due to complexity and poor user experience.
2.  **State expiry**: Automatically removing unused state after a time period. This was rejected due to technical complexity and breaking changes to existing applications.
3.  **Lower cost increases**: Using smaller multipliers (e.g., 50,000 gas instead of 250,000 gas). This was rejected as it would not provide sufficient economic deterrent against well-funded attackers.

* * *

Specification
-------------

Gas Cost Changes[](https://docs.tempo.xyz/protocol/tips/tip-1000#gas-cost-changes)
----------------------------------------------------------------------------------

### New State Element Creation[](https://docs.tempo.xyz/protocol/tips/tip-1000#new-state-element-creation)

**Current Behavior:**

*   Writing a new state element (SSTORE to a zero slot) costs 20,000 gas

**Proposed Behavior:**

*   Writing a new state element (SSTORE to a zero slot) costs 250,000 gas

This applies to all storage slot writes that transition from zero to non-zero, including:

*   Contract storage slots
*   TIP-20 token balances
*   Nonce key storage in the Nonce precompile (when a new nonce key is first used)
*   Rewards-related storage (userRewardInfo mappings, reward balances)
*   Active key count tracking in the Nonce precompile
*   Any other state elements stored in the EVM state trie

**Note:** Since Tempo-specific operations (nonce keys, rewards processing, etc.) ultimately use EVM storage operations (SSTORE), they are automatically subject to the new state creation pricing. The implementation must ensure all new state element creation is correctly charged at 250,000 gas, regardless of which precompile or contract creates the state.

### Account Creation[](https://docs.tempo.xyz/protocol/tips/tip-1000#account-creation)

**Current Behavior:**

*   Account creation has no explicit gas cost
*   The account is created implicitly when its nonce is first written

**Proposed Behavior:**

*   Account creation incurs a 250,000 gas charge when the account's nonce is first written
*   This charge applies when the account is first used (e.g., sends its first transaction), not when it first receives tokens

**Implementation Details:**

*   The charge is applied when `account.nonce` transitions from 0 to 1
*   The charge also applies to other nonces with [nonce keys](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#specification) (2D nonces)
*   Transactions with a nonce value of 0 need to supply at least 271,000 gas and are otherwise invalid
*   For EOA accounts: charged on the first transaction sent from that address (when the account is first used)
*   For contract accounts: charged when the contract is deployed (CREATE or CREATE2)
*   **Important:** When tokens are transferred TO a new address, the recipient's nonce remains 0, so no account creation cost is charged. The account creation cost only applies when the account is first used (sends a transaction).
*   The charge is in addition to any other gas costs for the transaction

### Contract Creation[](https://docs.tempo.xyz/protocol/tips/tip-1000#contract-creation)

**Current Behavior:**

*   Contract creation (CREATE/CREATE2) has a base cost of 32,000 gas plus 200 gas per byte of contract code
*   Total cost formula: `32,000 + (code_size × 200)` gas
*   Example: A 1,000 byte contract costs 32,000 + (1,000 × 200) = 232,000 gas

**Proposed Behavior:**

*   Contract creation replaces the existing EVM per-byte cost with a new pricing model:
    *   Each byte: 1,000 gas per byte (linear pricing)
    *   TX create cost: 2 × 250,000 gas = 500,000 gas for keccak hash and nonce fields
*   This pricing applies to the contract code size (the bytecode being deployed)

**Implementation Details:**

*   The code storage cost is calculated as: `code_size × 1,000`
*   Additional state creation costs: 2 × 250,000 gas = 500,000 gas for keccak hash and codesize fields
*   Total contract creation cost: `(code_size × 1,000) + 500,000` gas
*   This replaces the existing EVM per-byte cost for contract creation (not an additional charge)
*   Applies to both CREATE and CREATE2 operations
*   The account creation cost (250,000 gas) is separate and still applies when the contract account's nonce transitions from 0 to 1

### Intrinsic transaction gas[](https://docs.tempo.xyz/protocol/tips/tip-1000#intrinsic-transaction-gas)

A transaction is invalid if the minimal costs of a (reverting) transaction can't be covered by caller's balance. Those checks are done in the transaction pool as a DOS prevention measure as well as when a transaction is first executed as part of a block.

*   Transaction with `nonce == 0` require an additional 250,000 gas
*   Tempo transactions with any `nonce_key` and `nonce == 0` require an additional 250,000 gas
*   Changes to EIP-7702 authorization lists:
    *   EIP-7702 authorisation list entries with `auth_list.nonce == 0` require an additional 250,000 gas.
    *   The base cost per authorization is reduced to 12,500 gas
    *   There is no refund if the account already exists
*   The additional initial cost for CREATE transactions that deploy a contract is increased to 500,000 from currently 32,000 (to reflect the upfront cost in contract creation)
    *   If the first transaction in a batch is a CREATE transaction, the additional cost of 500,000 needs to be charged

### Other changes[](https://docs.tempo.xyz/protocol/tips/tip-1000#other-changes)

The transaction gas cap is changed from 16M to 30M to accommodate the deployment of 24kb contracts.

Tempo transaction key authorisations can't determine whether it is going to create new storage or not. If the transaction cannot pay for the key authorization storage costs, the transaction reverts any authorization key that has been set.

Gas Schedule Summary[](https://docs.tempo.xyz/protocol/tips/tip-1000#gas-schedule-summary)
------------------------------------------------------------------------------------------


|Operation                                          |Current Gas Cost|Proposed Gas Cost|Change   |
|---------------------------------------------------|----------------|-----------------|---------|
|New state element (SSTORE zero → non-zero)         |20,000          |250,000          |+230,000 |
|Account creation (first nonce write)               |0               |250,000          |+250,000 |
|Contract creation (per byte)                       |200             |1,000            |+800     |
|Contract creation (keccak + codesize fields)       |Included in base|500,000          |+500,000 |
|Existing state element (SSTORE non-zero → non-zero)|5,000           |5,000            |No change|
|Existing state element (SSTORE non-zero → zero)    |-15,000 (refund)|-15,000 (refund) |No change|


Economic Impact Analysis[](https://docs.tempo.xyz/protocol/tips/tip-1000#economic-impact-analysis)
--------------------------------------------------------------------------------------------------

### Cost Calculations[](https://docs.tempo.xyz/protocol/tips/tip-1000#cost-calculations)

Based on the assumptions:

*   TIP-20 transfer cost (to existing address, including base transaction and state update): 50,000 gas = 0.1 cent
*   Implied gas price: 1 cent per 500,000 gas

**New State Element Creation:**

*   Gas cost: 250,000 gas
*   Dollar cost: 250,000 / 500,000 = **0.5 cents per state element**

**Account Creation:**

*   Gas cost: 250,000 gas
*   Dollar cost: 250,000 / 500,000 = **0.5 cents per account**

**Contract Creation:**

*   Per byte: 1,000 gas = **0.002 cents per byte**
*   Keccak + codesize fields: 500,000 gas (2 × 250,000) = **1.0 cent**
*   Example: 1,000 byte contract = (1,000 × 1,000) + 500,000 = 1,500,000 gas = **3.0 cents**

### Attack Cost Analysis[](https://docs.tempo.xyz/protocol/tips/tip-1000#attack-cost-analysis)

**Creating 1 TB of state:**

*   1 TB = 1,000,000,000,000 bytes
*   Assuming ~100 bytes per state element: 10,000,000,000 state elements
*   Cost: 10,000,000,000 × 0.5 cents = **$50,000,000**

**Creating 10 TB of state:**

*   10 TB = 10,000,000,000,000 bytes
*   Assuming ~100 bytes per state element: 100,000,000,000 state elements
*   Cost: 100,000,000,000 × 0.5 cents = **$500,000,000**

These costs serve as a significant economic deterrent against state growth spam attacks.

Impact on Normal Operations[](https://docs.tempo.xyz/protocol/tips/tip-1000#impact-on-normal-operations)
--------------------------------------------------------------------------------------------------------

### Transfer to New Address[](https://docs.tempo.xyz/protocol/tips/tip-1000#transfer-to-new-address)

**Current Cost:**

*   TIP-20 transfer (base + operation): 50,000 gas
*   New state element (balance): 20,000 gas
*   **Total: ~70,000 gas ≈ 0.14 cents**
*   Note: Account creation cost does not apply here because the recipient's nonce remains 0

**Proposed Cost:**

*   TIP-20 transfer (base + operation): 50,000 gas
*   New state element (balance): 250,000 gas
*   **Total: ~300,000 gas ≈ 0.6 cents**
*   Note: Account creation cost does not apply here because the recipient's nonce remains 0

**Impact:** A transfer to a new address increases from 0.14 cents to 0.6 cents, representing a 4.3x increase. The account creation cost (0.5 cents) will be charged separately when the recipient first uses their account.

### First Use of New Account[](https://docs.tempo.xyz/protocol/tips/tip-1000#first-use-of-new-account)

**Current Cost:**

*   TIP-20 transfer (base + operation + state update): 50,000 gas
*   Account creation: 0 gas
*   **Total: 50,000 gas ≈ 0.1 cents**

**Proposed Cost:**

*   TIP-20 transfer (base + operation + state update): 50,000 gas
*   Account creation (nonce 0 → 1): 250,000 gas
*   **Total: ~300,000 gas ≈ 0.6 cents**

**Impact:** The first transaction from a new account increases from 0.1 cents to 0.6 cents, representing a 6x increase. Combined with the initial transfer cost (0.6 cents), the total onboarding cost for a new user is approximately 1.2 cents.

### Transfer to Existing Address[](https://docs.tempo.xyz/protocol/tips/tip-1000#transfer-to-existing-address)

**Current Cost:**

*   TIP-20 transfer (base + operation + state update): 50,000 gas
*   **Total: 50,000 gas ≈ 0.1 cents**

**Proposed Cost:**

*   TIP-20 transfer (base + operation + state update): 50,000 gas
*   **Total: 50,000 gas ≈ 0.1 cents**

**Impact:** No change for transfers to existing addresses.

### Contract Deployment[](https://docs.tempo.xyz/protocol/tips/tip-1000#contract-deployment)

**Current Cost:**

*   Contract code storage: 32,000 gas base + 200 gas per byte
*   Example for 1,000 byte contract: 32,000 + (1,000 × 200) = 232,000 gas ≈ 0.46 cents

**Proposed Cost:**

*   Account creation: 250,000 gas
*   Contract code storage: code\_size × 1,000 gas
*   Keccak + codesize fields: 500,000 gas (2 × 250,000)
*   Example for 1,000 byte contract: 250,000 + (1,000 × 1,000) + 500,000 = 1,750,000 gas ≈ **3.5 cents**

**Impact:** Contract deployment costs increase significantly, especially for larger contracts. A 100 byte contract costs (100 × 1,000) + 500,000 + 250,000 = 850,000 gas = 1.7 cents total.

Implementation Requirements[](https://docs.tempo.xyz/protocol/tips/tip-1000#implementation-requirements)
--------------------------------------------------------------------------------------------------------

### Node Implementation[](https://docs.tempo.xyz/protocol/tips/tip-1000#node-implementation)

The node implementation must:

1.  **Detect new state element creation:**
    
    *   Track SSTORE operations that write to a zero slot
    *   Charge 250,000 gas instead of 20,000 gas for these operations
2.  **Detect account creation:**
    
    *   Track when an account's nonce transitions from 0 to 1
    *   Charge 250,000 gas for this transition
    *   Apply to both EOA and contract account creation
3.  **Implement contract creation pricing:**
    
    *   Replace existing EVM per-byte cost for contract code storage
    *   Charge 1,000 gas per byte of contract code (linear pricing)
    *   Charge 500,000 gas (2 × 250,000) for keccak hash and codesize fields
    *   Total formula: `(code_size × 1,000) + 500,000`
    *   Apply to both CREATE and CREATE2 operations
4.  **Maintain backward compatibility:**
    
    *   Existing state operations (non-zero to non-zero, non-zero to zero) remain unchanged
    *   Gas refunds for storage clearing remain unchanged

### Test Suite Requirements[](https://docs.tempo.xyz/protocol/tips/tip-1000#test-suite-requirements)

The test suite must verify:

1.  **New state element creation:**
    
    *   SSTORE to zero slot charges 250,000 gas
    *   Multiple new state elements in one transaction are each charged 250,000 gas
    *   Existing state element updates (non-zero to non-zero) remain at 5,000 gas
2.  **Account creation:**
    
    *   First transaction from EOA charges 250,000 gas for account creation (when nonce transitions 0 → 1)
    *   Contract deployment (CREATE) charges 250,000 gas for account creation
    *   Contract deployment (CREATE2) charges 250,000 gas for account creation
    *   Transfer TO a new address does NOT charge account creation fee (recipient's nonce remains 0)
    *   Subsequent transactions from the same account do not charge account creation fee
3.  **Contract creation:**
    
    *   Contract code storage replaces EVM per-byte cost with new pricing model
    *   Each byte of contract code costs 1,000 gas (linear pricing)
    *   Keccak hash and codesize fields cost 500,000 gas (2 × 250,000) total
    *   Total cost formula: `(code_size × 1,000) + 500,000` gas
    *   Example: 100 byte contract costs (100 × 1,000) + 500,000 = 600,000 gas
    *   Both CREATE and CREATE2 use the same pricing
4.  **Tempo-specific state creation operations:**
    
    *   Nonce key creation: First use of a new nonce key (nonce key > 0) creates storage in Nonce precompile
    *   Active key count tracking: First nonce key for an account creates active key count storage
    *   Rewards opt-in: `setRewardRecipient` creates new `userRewardInfo` mapping entry
    *   Rewards recipient delegation: Setting reward recipient for a new recipient creates storage
    *   Rewards balance creation: First reward accrual to a recipient creates storage if needed
    *   All Tempo-specific operations that create new state elements must charge 250,000 gas per new storage slot
5.  **Edge cases:**
    
    *   Self-destruct and recreation of account
    *   Contracts that create accounts via CREATE/CREATE2
    *   Batch operations creating multiple accounts/state elements
    *   Contract deployment with various code sizes (small, medium, large)
    *   Multiple Tempo-specific operations in a single transaction
6.  **Economic calculations:**
    
    *   Verify gas costs match expected dollar amounts
    *   Verify attack cost calculations for large-scale state creation
    *   Verify contract creation costs match formula: `(code_size × 1,000) + 500,000 + 250,000` (including account creation)
    *   Verify Tempo-specific operations charge correctly for new state creation

* * *

Invariants
----------

The following invariants must always hold:

1.  **State Creation Cost Invariant:** Any SSTORE operation that writes a non-zero value to a zero slot MUST charge exactly 250,000 gas (not 20,000 gas).
    
2.  **Account Creation Cost Invariant:** The first transaction that causes an account's nonce to transition from 0 to 1 MUST charge exactly 250,000 gas for account creation.
    
3.  **Existing State Invariant:** SSTORE operations that modify existing non-zero state (non-zero to non-zero) MUST continue to charge 5,000 gas and MUST NOT be affected by this change.
    
4.  **Storage Clearing Invariant:** SSTORE operations that clear storage (non-zero to zero) MUST continue to provide a 15,000 gas refund and MUST NOT be affected by this change.
    
5.  **Gas Accounting Invariant:** The total gas charged for a transaction creating N new state elements and M new accounts (where M is the number of accounts whose nonce transitions from 0 to 1 in this transaction) MUST equal: base\_transaction\_gas + operation\_gas + (N × 250,000) + (M × 250,000). Note: Transferring tokens TO a new address does not create the account (nonce remains 0), so M = 0 in that case.
    
6.  **Contract Creation Cost Invariant:** Contract creation (CREATE/CREATE2) MUST charge exactly `(code_size × 1,000) + 500,000` gas for code storage, replacing the existing EVM per-byte cost. This includes: 1,000 gas per byte of contract code (linear pricing) and 500,000 gas (2 × 250,000) for keccak hash and codesize fields. The account creation cost (250,000 gas) is charged separately.
    
7.  **Economic Deterrent Invariant:** The cost to create 1 TB of state MUST be at least $50 million, and the cost to create 10 TB of state MUST be at least $500 million, based on the assumed gas price of 1 cent per 500,000 gas.
    

Critical Test Cases[](https://docs.tempo.xyz/protocol/tips/tip-1000#critical-test-cases)
----------------------------------------------------------------------------------------

The test suite must cover:

1.  **Basic state creation:** Single SSTORE to zero slot charges 250,000 gas
2.  **Multiple state creation:** Multiple SSTORE operations to zero slots each charge 250,000 gas
3.  **Account creation (EOA):** First transaction from new EOA charges 250,000 gas
4.  **Account creation (CREATE):** Contract deployment via CREATE charges 250,000 gas for account creation
5.  **Account creation (CREATE2):** Contract deployment via CREATE2 charges 250,000 gas for account creation
6.  **Contract creation (small):** Contract with 100 bytes charges (100 × 1,000) + 500,000 = 600,000 gas for code storage
7.  **Contract creation (medium):** Contract with 1,000 bytes charges (1,000 × 1,000) + 500,000 = 1,500,000 gas for code storage
8.  **Contract creation (large):** Contract with 10,000 bytes charges (10,000 × 1,000) + 500,000 = 10,500,000 gas for code storage
9.  **Existing state updates:** SSTORE to existing non-zero slot charges 5,000 gas (unchanged)
10.  **Storage clearing:** SSTORE clearing storage provides 15,000 gas refund (unchanged)
11.  **Mixed operations:** Transaction creating both new accounts and new state elements charges correctly for both
12.  **Transfer to new address:** Complete transaction cost matches expected ~300,000 gas (no account creation cost, only new state element cost)
13.  **First use of new account:** Complete transaction cost matches expected ~300,000 gas (account creation cost applies)
14.  **Transfer to existing address:** Complete transaction cost matches expected 50,000 gas (unchanged)
15.  **Batch operations:** Multiple account creations in one transaction each charge 250,000 gas
16.  **Self-destruct and recreate:** Account that self-destructs and is recreated charges account creation fee again
17.  **Transfer to new address does not create account:** Transferring tokens to a new address does not charge account creation fee (only new state element fee applies)
18.  **Nonce key creation:** First use of a new nonce key creates a new storage slot and charges 250,000 gas
19.  **Active key count tracking:** First nonce key for an account creates storage for active key count and charges 250,000 gas
20.  **Rewards opt-in:** First call to `setRewardRecipient` creates a new entry and charges 250,000 gas
21.  **Rewards recipient delegation:** Setting a new reward recipient creates storage and charges 250,000 gas
22.  **Rewards balance creation:** First reward accrual creates storage and charges 250,000 gas (if needed)
23.  **Multiple nonce keys:** Creating multiple nonce keys in one transaction each charges 250,000 gas
24.  **Nonce key and rewards combined:** Transaction creating both nonce key and rewards storage charges 250,000 gas for each new state element
