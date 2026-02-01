# Tempo Transactions
Tempo Transactions are a new [EIP-2718](https://github.com/ethereum/EIPs/blob/master/EIPS/eip-2718.md) transaction type, exclusively available on Tempo.

If you're integrating with Tempo, we **strongly recommend** using Tempo Transactions, and not regular Ethereum transactions. Learn more about the benefits below, or follow the guide on issuance [here](https://docs.tempo.xyz/guide/issuance).

Integration Guides[](https://docs.tempo.xyz/protocol/transactions#integration-guides)
-------------------------------------------------------------------------------------

Integrating Tempo Transactions is easy and can be done quickly by a developer in multiple languages. See below for quick links to some of our guides.

If you are an EVM smart contract developer, see the [Tempo extension for Foundry](https://docs.tempo.xyz/sdk/foundry).

Properties[](https://docs.tempo.xyz/protocol/transactions#properties)
---------------------------------------------------------------------

### Configurable Fee Tokens[](https://docs.tempo.xyz/protocol/transactions#configurable-fee-tokens)

A fee token is a permissionless [TIP-20 token](https://docs.tempo.xyz/protocol/tip20/overview) that can be used to pay fees on Tempo.

When a TIP-20 token is passed as the `fee_token` parameter in a transaction, Tempo's [Fee AMM](https://docs.tempo.xyz/protocol/fees/spec-fee-amm) automatically facilitates conversion between the user's preferred fee token and the validator's preferred token.

Fee sponsorship enables a third party (the fee payer) to pay transaction fees on behalf of the transaction sender.

The process uses dual signature domains: the sender signs their transaction, and then the fee payer signs over the transaction with a special "fee payer envelope" to commit to paying fees for that specific sender.

### Batch Calls[](https://docs.tempo.xyz/protocol/transactions#batch-calls)

Batch calls enable multiple operations to be executed atomically within a single transaction. Instead of sending separate transactions for each operation, you can bundle multiple calls together using the `calls` parameter.

### Access Keys[](https://docs.tempo.xyz/protocol/transactions#access-keys)

Access keys enable you to delegate signing authority from a primary account to a secondary key, such as device-bound non-extractable [WebCrypto key](https://developer.mozilla.org/en-US/docs/Web/API/CryptoKeyPair). The primary account signs a key authorization that grants the access key permission to sign transactions on its behalf.

This authorization is then attached to the next transaction (that can be signed by either the primary or the access key), then all transactions thereafter can be signed by the access key.

### Concurrent Transactions[](https://docs.tempo.xyz/protocol/transactions#concurrent-transactions)

Concurrent transactions enable higher throughput by allowing multiple transactions from the same account to be sent in parallel without waiting for sequential nonce confirmation.

By using different nonce keys, you can submit multiple transactions simultaneously that don't conflict with each other, enabling parallel execution and significantly improved transaction throughput for high-activity accounts.

### Scheduled Transactions[](https://docs.tempo.xyz/protocol/transactions#scheduled-transactions)

Scheduled transactions allow you to sign a transaction in advance and specify a time window for when it can be executed onchain. By setting `validAfter` and `validBefore` timestamps, you define the earliest and latest times the transaction can be included in a block.

Learn more[](https://docs.tempo.xyz/protocol/transactions#learn-more)
---------------------------------------------------------------------

# Tempo Transaction
Abstract[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#abstract)
----------------------------------------------------------------------------------------

This spec introduces native protocol support for the following features, using a new Tempo transaction type:

*   WebAuthn/P256 signature validation - enables passkey accounts
*   Parallelizable nonces - allows higher tx throughput for each account
*   Gas sponsorship - allows apps to pay for their users' transactions
*   Call Batching - allows users to multicall efficiently and atomically
*   Scheduled Txs - allow users to specify a time window in which their tx can be executed
*   Access Keys - allow a sender's key to provision scoped access keys with spending limits

Motivation[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#motivation)
--------------------------------------------------------------------------------------------

Current accounts are limited to secp256k1 signatures and sequential nonces, creating UX and scalability challenges.  
Users cannot leverage modern authentication methods like passkeys, applications face throughput limitations due to sequential nonces.

Specification[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#specification)
--------------------------------------------------------------------------------------------------

### Transaction Type[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#transaction-type)

A new EIP-2718 transaction type is introduced with type byte `0x76`:

```
pub struct TempoTransaction {
    // Standard EIP-1559 fields
    chain_id: ChainId,                          // EIP-155 replay protection
    max_priority_fee_per_gas: u128,
    max_fee_per_gas: u128,
    gas_limit: u64,
    calls: Vec<Call>,                           // Batch of calls to execute atomically
    access_list: AccessList,                    // EIP-2930 access list
 
    // nonce-related fields
    nonce_key: U256,                            // 2D nonce key (0 = protocol nonce, >0 = user nonces)
    nonce: u64,                                 // Current nonce value for the nonce key
 
    // Optional features
    fee_token: Option<Address>,                 // Optional fee token preference
    fee_payer_signature: Option<Signature>,     // Sponsored transactions (secp256k1 only)
    valid_before: Option<u64>,                  // Transaction expiration timestamp
    valid_after: Option<u64>,                   // Transaction can only be included after this timestamp
    key_authorization: Option<SignedKeyAuthorization>, // Access key authorization (optional)
    aa_authorization_list: Vec<TempoSignedAuthorization>, // EIP-7702 style authorizations with AA signatures
}
 
// Call structure for batching
pub struct Call {
    to: TxKind,      // Can be Address or Create
    value: U256,
    input: Bytes     // Calldata for the call
}
 
// Key authorization for provisioning access keys
// RLP encoding: [chain_id, key_type, key_id, expiry?, limits?]
pub struct KeyAuthorization {
    chain_id: u64,                              // Chain ID for replay protection (0 = valid on any chain)
    key_type: SignatureType,                    // Type of key: Secp256k1 (0), P256 (1), or WebAuthn (2)
    key_id: Address,                            // Key identifier (address derived from public key)
    expiry: Option<u64>,                        // Unix timestamp when key expires (None = never expires)
    limits: Option<Vec<TokenLimit>>,            // TIP20 spending limits (None = unlimited spending)
}
 
// Signed key authorization (authorization + root key signature)
pub struct SignedKeyAuthorization {
    authorization: KeyAuthorization,
    signature: PrimitiveSignature,              // Root key's signature over keccak256(rlp(authorization))
}
 
// TIP20 spending limits for access keys
pub struct TokenLimit {
    token: Address,                             // TIP20 token address
    limit: U256,                                // Maximum spending amount for this token
}
```


### Signature Types[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#signature-types)

Four signature schemes are supported. The signature type is determined by length and type identifier:

#### secp256k1 (65 bytes)
[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#secp256k1-65-bytes)

```
pub struct Signature {
    r: B256,        // 32 bytes
    s: B256,        // 32 bytes
    v: u8           // 1 byte (recovery id)
}
```


**Format**: No type identifier prefix (backward compatible). Total length: 65 bytes. **Detection**: Exactly 65 bytes with no type identifier.

#### P256 (130 bytes)
[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#p256-130-bytes)

```
pub struct P256SignatureWithPreHash {
    typeId: u8,         // 0x01
    r: B256,            // 32 bytes
    s: B256,            // 32 bytes
    pub_key_x: B256,    // 32 bytes
    pub_key_y: B256,    // 32 bytes
    pre_hash: bool      // 1 byte
}
```


**Format**: Type identifier `0x01` + 129 bytes of signature data. Total length: 130 bytes. The `typeId` is a wire format prefix (not a struct field) prepended during encoding.

Note: Some P256 implementers (like Web Crypto) require the digests to be pre-hashed before verification. If `pre_hash` is set to `true`, then before verification: `digest = sha256(digest)`.

#### WebAuthn (Variable length, max 2KB)
[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#webauthn-variable-length-max-2kb)

```
pub struct WebAuthnSignature {
    typeId: u8,                 // 0x02
    webauthn_data: Bytes,       // Variable length (authenticatorData || clientDataJSON)
    r: B256,                    // 32 bytes
    s: B256,                    // 32 bytes
    pub_key_x: B256,            // 32 bytes
    pub_key_y: B256             // 32 bytes
}
```


**Format**: Type identifier `0x02` + variable webauthn\_data + 128 bytes (r, s, pub\_key\_x, pub\_key\_y). Total length: variable (minimum 129 bytes, maximum 2049 bytes). The `typeId` is a wire format prefix prepended during encoding. Parse by working backwards: last 128 bytes are r, s, pub\_key\_x, pub\_key\_y.

#### Keychain (Variable length)
[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#keychain-variable-length)

```
pub struct KeychainSignature {
    typeId: u8,                     // 0x03
    user_address: Address,          // 20 bytes - root account address
    signature: PrimitiveSignature   // Inner signature (Secp256k1, P256, or WebAuthn)
}
```


**Format**: Type identifier `0x03` + user\_address (20 bytes) + inner signature. The `typeId` is a wire format prefix prepended during encoding. **Purpose**: Allows an access key to sign on behalf of a root account. The handler validates that `user_address` has authorized the access key in the AccountKeychain precompile.

### Address Derivation[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#address-derivation)

#### secp256k1[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#secp256k1)

```
address(uint160(uint256(keccak256(abi.encode(x, y)))))
```


#### P256 and WebAuthn[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#p256-and-webauthn)

```
function deriveAddressFromP256(bytes32 pubKeyX, bytes32 pubKeyY) public pure returns (address) {    
    // Hash 
    bytes32 hash = keccak256(abi.encodePacked(
        pubKeyX,
        pubKeyY
    ));
    
    // Take last 20 bytes as address
    return address(uint160(uint256(hash)));
}
```


The `aa_authorization_list` field enables EIP-7702 style delegation with support for all three AA signature types (secp256k1, P256, and WebAuthn), not just secp256k1.

#### Structure[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#structure)

```
pub struct TempoSignedAuthorization {
    inner: Authorization,      // Standard EIP-7702 authorization
    signature: TempoSignature,    // Can be Secp256k1, P256, or WebAuthn
}
```


Each authorization in the list:

*   Delegates an account to a specified implementation contract
*   Is signed by the account's authority using any supported signature type
*   Follows EIP-7702 semantics for delegation and execution

#### Validation[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#validation)

*   Cannot have `Create` calls when `aa_authorization_list` is non-empty (follows EIP-7702 semantics)
*   Authority address is recovered from the signature and matched against the authorization

### Parallelizable Nonces[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#parallelizable-nonces)

*   **Protocol nonce (key 0)**: Existing account nonce, incremented for regular txs, 7702 authorization, or `CREATE`
*   **User nonces (keys 1-N)**: Enable parallel execution with special gas schedule
*   **Reserved sequence keys**: Nonce sequence keys with the most significant byte `0x5b` are reserved for [sub-block transactions](https://docs.tempo.xyz/protocol/blockspace/sub-block-specification#11-sub-block-transactions).

#### Account State Changes[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#account-state-changes)

*   `nonces: mapping(uint256 => uint64)` - 2D nonce tracking

**Implementation Note:** Nonces are stored in the storage of a designated precompile at address `0x4E4F4E4345000000000000000000000000000000` (ASCII hex for "NONCE"), as there is currently no clean way to extend account state in Reth.

**Storage Layout at 0x4E4F4E4345:**

*   Storage key: `keccak256(abi.encode(account_address, nonce_key))`
*   Storage value: `nonce` (uint64)

Note: Protocol Nonce key (0), is directly stored in the account state, just like normal transaction types.

#### Nonce Precompile[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#nonce-precompile)

The nonce precompile implements the following interface for managing 2D nonces:

```
/// @title INonce - Nonce Precompile Interface
/// @notice Interface for managing 2D nonces as per the Tempo Transaction spec
/// @dev This precompile manages user nonce keys (1-N) while protocol nonces (key 0)
///      are handled directly by account state. Each account can have multiple
///      independent nonce sequences identified by a nonce key.
interface INonce {
    /// @notice Emitted when a nonce is incremented for an account and nonce key
    /// @param account The account whose nonce was incremented
    /// @param nonceKey The nonce key that was incremented
    /// @param newNonce The new nonce value after incrementing
    event NonceIncremented(address indexed account, uint256 indexed nonceKey, uint64 newNonce);
    /// @notice Thrown when trying to access protocol nonce (key 0) through the precompile
    /// @dev Protocol nonce should be accessed through account state, not this precompile
    error ProtocolNonceNotSupported();
    /// @notice Thrown when an invalid nonce key is provided
    error InvalidNonceKey();
    /// @notice Thrown when a nonce value would overflow
    error NonceOverflow();
    /// @notice Get the current nonce for a specific account and nonce key
    /// @param account The account address
    /// @param nonceKey The nonce key (must be > 0, protocol nonce key 0 not supported)
    /// @return nonce The current nonce value
    function getNonce(address account, uint256 nonceKey) external view returns (uint64 nonce);
}
```


#### Precompile Implementation[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#precompile-implementation)

The precompile contract maintains a single storage mapping:

```
contract Nonce is INonce {
    /// @dev Mapping from account -> nonce key -> nonce value
    mapping(address => mapping(uint256 => uint64)) private nonces;
}
```


#### Gas Schedule[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#gas-schedule)

For transactions using nonce keys:

1.  **Protocol nonce (key 0)**: No additional gas cost
    
    *   Uses the standard account nonce stored in account state
2.  **Existing user key (nonce > 0)**: Add 5,000 gas to base cost
    
    *   Rationale: Cold SLOAD (2,100) + warm SSTORE reset (2,900)
3.  **New user key (nonce == 0)**: Add 22,100 gas to base cost
    
    *   Rationale: Cold SLOAD (2,100) + SSTORE set for 0 → non-zero (20,000)

We specify the complete gas schedule in more detail in the [gas costs section](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#gas-costs)

### Transaction Validation[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#transaction-validation)

#### Signature Validation[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#signature-validation)

1.  Determine type from signature format:
    *   65 bytes (no type identifier) = secp256k1
    *   First byte `0x01` + 129 bytes = P256 (total 130 bytes)
    *   First byte `0x02` + variable data = WebAuthn (total 129-2049 bytes)
    *   First byte `0x03` + 20 bytes + inner signature = Keychain
    *   Otherwise invalid
2.  Apply appropriate verification:
    *   secp256k1: Standard `ecrecover`
    *   P256: P256 curve verification with provided public key (sha256 pre-hash if flag set)
    *   WebAuthn: Parse clientDataJSON, verify challenge and type, then P256 verify
    *   Keychain: Verify inner signature, then validate access key authorization via AccountKeychain precompile

#### Nonce Validation[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#nonce-validation)

1.  Fetch sequence for given nonce key
2.  Verify sequence matches transaction
3.  Increment sequence

#### Fee Payer Validation (if present)
[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#fee-payer-validation-if-present)

1.  Verify fee payer signature (K1 only initially)
2.  Recover payer address via `ecrecover`
3.  Deduct fees from payer instead of sender

### Fee Payer Signature Details[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#fee-payer-signature-details)

The Tempo Transaction Type (0x76) supports **gas sponsorship** where a third party (fee payer) can pay transaction fees on behalf of the sender. This is achieved through dual signature domains—the sender signs with transaction type byte `0x76`, while the fee payer signs with magic byte `0x78` to ensure domain separation and prevent signature reuse attacks.

#### Signing Domains[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#signing-domains)

##### Sender Signature[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#sender-signature)

For computing the transaction hash that the sender signs:

*   Fields are preceded by transaction type byte `0x76`
*   Field 11 (`fee_token`) is encoded as empty string (`0x80`) **if and only if** `fee_payer_signature` is present. This allows the fee payer to specify the fee token.
*   Field 12 (`fee_payer_signature`) is encoded as:
    *   Single byte `0x00` if fee payer signature will be present (placeholder)
    *   Empty string `0x80` if no fee payer

**Sender Signature Hash:**

```
// When fee_payer_signature is present:
sender_hash = keccak256(0x76 || rlp([
    chain_id,
    max_priority_fee_per_gas,
    max_fee_per_gas,
    gas_limit,
    calls,
    access_list,
    nonce_key,
    nonce,
    valid_before,
    valid_after,
    0x80,  // fee_token encoded as EMPTY (skipped)
    0x00   // placeholder byte for fee_payer_signature
]))
 
// When no fee_payer_signature:
sender_hash = keccak256(0x76 || rlp([
    chain_id,
    max_priority_fee_per_gas,
    max_fee_per_gas,
    gas_limit,
    calls,
    access_list,
    nonce_key,
    nonce,
    valid_before,
    valid_after,
    fee_token,  // fee_token is INCLUDED
    0x80        // empty for no fee_payer_signature
]))
```


##### Fee Payer Signature[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#fee-payer-signature)

Only included for sponsored transactions. For computing the fee payer's signature hash:

*   Fields are preceded by **magic byte `0x78`** (different from transaction type `0x76`)
*   Field 11 (`fee_token`) is **always included** (20-byte address or `0x80` for None)
*   Field 12 is serialized as the **sender address** (20 bytes). This commits the fee payer to sponsoring a specific sender.

**Fee Payer Signature Hash:**

```
fee_payer_hash = keccak256(0x78 || rlp([  // Note: 0x78 magic byte
    chain_id,
    max_priority_fee_per_gas,
    max_fee_per_gas,
    gas_limit,
    calls,
    access_list,
    nonce_key,
    nonce,
    valid_before,
    valid_after,
    fee_token,      // fee_token ALWAYS included
    sender_address  // 20-byte sender address
    key_authorization,
]))
```


#### Key Properties[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#key-properties)

1.  **Sender Flexibility**: By omitting `fee_token` from sender signature when fee payer is present, the fee payer can specify which token to use for payment without invalidating the sender's signature
2.  **Fee Payer Commitment**: Fee payer's signature includes `fee_token` and `sender_address`, ensuring they agree to:
    *   Pay for the specific sender
    *   Use the specific fee token
3.  **Domain Separation**: Different magic bytes (`0x76` vs `0x78`) prevent signature reuse attacks between sender and fee payer roles
4.  **Deterministic Fee Payer**: The fee payer address is statically recoverable from the transaction via secp256k1 signature recovery

#### Validation Rules[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#validation-rules)

**Signature Requirements:**

*   Sender signature MUST be valid (secp256k1, P256, or WebAuthn depending on signature length)
*   If `fee_payer_signature` present:
    *   MUST be recoverable via secp256k1 (only secp256k1 supported for fee payers)
    *   Recovery MUST succeed, otherwise transaction is invalid
*   If `fee_payer_signature` absent:
    *   Fee payer defaults to sender address (self-paid transaction)

**Token Preference:**

*   When `fee_token` is `Some(address)`, this overrides any account/validator-level preferences
*   Validation ensures the token is a valid TIP-20 token with sufficient balance/liquidity
*   Failures reject the transaction before execution (see Token Preferences spec)

**Fee Payer Resolution:**

*   Fee payer signature present → recovered address via `ecrecover`
*   Fee payer signature absent → sender address
*   This address is used for all fee accounting (pre-charge, refund) via TIP Fee Manager precompile

#### Transaction Flow[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#transaction-flow)

1.  **User prepares transaction**: Sets `fee_payer_signature` to placeholder (`Some(Signature::default())`)
2.  **User signs**: Computes sender hash (with fee\_token skipped) and signs
3.  **Fee payer receives** user-signed transaction
4.  **Fee payer verifies** user signature is valid
5.  **Fee payer signs**: Computes fee payer hash (with fee\_token and sender\_address) and signs
6.  **Complete transaction**: Replace placeholder with actual fee payer signature
7.  **Broadcast**: Transaction is sent to network with both signatures

#### Error Cases[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#error-cases)

*   `fee_payer_signature` present but unrecoverable → invalid transaction
*   Fee payer balance insufficient for `gas_limit * max_fee_per_gas` in fee token → invalid
*   Any sender signature failure → invalid
*   Malformed RLP → invalid

### RLP Encoding[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#rlp-encoding)

The transaction is RLP encoded as follows:

**Signed Transaction Envelope:**

```
0x76 || rlp([
    chain_id,
    max_priority_fee_per_gas,
    max_fee_per_gas,
    gas_limit,
    calls,                   // RLP list of Call structs
    access_list,
    nonce_key,
    nonce,
    valid_before,            // 0x80 if None
    valid_after,             // 0x80 if None
    fee_token,               // 0x80 if None
    fee_payer_signature,     // 0x80 if None, RLP list [v, r, s] if Some
    aa_authorization_list,   // EIP-7702 style authorization list with AA signatures
    key_authorization?,      // Only encoded if present (backwards compatible)
    sender_signature         // TempoSignature bytes (secp256k1, P256, WebAuthn, or Keychain)
])

```


**Call Encoding:**

**Key Authorization Encoding:**

```
rlp([
    chain_id,
    key_type,
    key_id,
    expiry?,         // Optional trailing field (omitted or 0x80 if None)
    limits?,         // Optional trailing field (omitted or 0x80 if None)
    signature        // PrimitiveSignature bytes
])

```


**Notes:**

*   Optional fields encode as `0x80` (EMPTY\_STRING\_CODE) when `None`
*   The `key_authorization` field is truly optional - when `None`, no bytes are encoded (backwards compatible)
*   The `calls` field is a list that must contain at least one Call (empty calls list is invalid)
*   The `sender_signature` field is the final field and contains the TempoSignature bytes (secp256k1, P256, WebAuthn, or Keychain)
*   KeyAuthorization uses RLP trailing field semantics for optional `expiry` and `limits`

### WebAuthn Signature Verification[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#webauthn-signature-verification)

WebAuthn verification follows the [Daimo P256 verifier approach](https://github.com/daimo-eth/p256-verifier/blob/master/src/WebAuthn.sol).

#### Signature Format[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#signature-format)

```
signature = authenticatorData || clientDataJSON || r (32) || s (32) || pubKeyX (32) || pubKeyY (32)

```


Parse by working backwards:

*   Last 32 bytes: `pubKeyY`
*   Previous 32 bytes: `pubKeyX`
*   Previous 32 bytes: `s`
*   Previous 32 bytes: `r`
*   Remaining bytes: `authenticatorData || clientDataJSON` (requires parsing to split)

#### Authenticator Data Structure (minimum 37 bytes)
[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#authenticator-data-structure-minimum-37-bytes)

```
Bytes 0-31:   rpIdHash (32 bytes)
Byte 32:      flags (1 byte)
              - Bit 0 (0x01): User Presence (UP) - must be set
Bytes 33-36:  signCount (4 bytes)

```


#### Verification Steps[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#verification-steps)

```
def verify_webauthn(tx_hash: bytes32, signature: bytes, require_uv: bool) -> bool:
    # 1. Parse signature
    pubKeyY = signature[-32:]
    pubKeyX = signature[-64:-32]
    s = signature[-96:-64]
    r = signature[-128:-96]
    webauthn_data = signature[:-128]
 
    # Parse authenticatorData and clientDataJSON
    # Minimum authenticatorData is 37 bytes
    # Simple approach: try to decode clientDataJSON from different split points
    authenticatorData, clientDataJSON = split_webauthn_data(webauthn_data)
 
    # 2. Validate authenticator data
    if len(authenticatorData) < 37:
        return False
 
    flags = authenticatorData[32]
    if not (flags & 0x01):  # UP bit must be set
        return False
 
    # 3. Validate client data JSON
    if not contains(clientDataJSON, '"type":"webauthn.get"'):
        return False
 
    challenge_b64url = base64url_encode(tx_hash)
    challenge_property = '"challenge":"' + challenge_b64url + '"'
    if not contains(clientDataJSON, challenge_property):
        return False
 
    # 4. Compute message hash
    clientDataHash = sha256(clientDataJSON)
    messageHash = sha256(authenticatorData || clientDataHash)
 
    # 5. Verify P256 signature
    return p256_verify(messageHash, r, s, pubKeyX, pubKeyY)
```


#### What We Verify[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#what-we-verify)

*   Authenticator data minimum length (37 bytes)
*   User Presence (UP) flag is set
*   `"type":"webauthn.get"` in clientDataJSON
*   Challenge matches tx\_hash (Base64URL encoded)
*   P256 signature validity

#### What We Skip[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#what-we-skip)

*   Origin verification (not applicable to blockchain)
*   RP ID hash validation (no central RP in decentralized context)
*   Signature counter (anti-cloning left to application layer)
*   Backup flags (account policy decision)

#### Parsing authenticatorData and clientDataJSON[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#parsing-authenticatordata-and-clientdatajson)

Since authenticatorData has variable length, finding the split point requires:

1.  Check if AT flag (bit 6) is set at byte 32
2.  If not set, authenticatorData is exactly 37 bytes
3.  If set, need to parse CBOR credential data (complex, see implementation)
4.  Everything after authenticatorData is clientDataJSON (valid UTF-8 JSON)

**Simplified approach:** For TempoTransactions, wallets should send minimal authenticatorData (37 bytes, no AT/ED flags) to minimize gas costs and simplify parsing.

### Access Keys[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#access-keys)

A sender can choose to authorize an Access Key to sign transactions on the sender's behalf. This is useful to enable flows where a root key (e.g. a passkey) would provision a short-lived (scoped) Access Key to be able to sign transactions on the sender's behalf without inducing another passkey prompt.

More information about Access Keys can be found in the [Account Keychain Specification](https://docs.tempo.xyz/AccountKeychain).

A sender can authorize a key by signing over a "key authorization" item that contains the following information:

*   **Chain ID** for replay protection (0 = valid on any chain)
*   **Key type** (Secp256k1, P256, or WebAuthn)
*   **Key ID** (address derived from the public key)
*   **Expiration** timestamp of when the key should expire (optional - None means never expires)
*   TIP20 token **spending limits** for the key (optional - None means unlimited spending):
    *   Limits deplete as tokens are spent
    *   Root key can update limits via `updateSpendingLimit()` without revoking the key
    *   Note: Spending limits only apply to TIP20 token transfers, not ETH or other asset transfers

#### RLP Encoding[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#rlp-encoding-1)

**Unsigned Format:**

The root key signs over the keccak256 hash of the RLP encoded `KeyAuthorization`:

```
key_authorization_digest = keccak256(rlp([chain_id, key_type, key_id, expiry?, limits?]))

chain_id = u64 (0 = valid on any chain)
key_type = 0 (Secp256k1) | 1 (P256) | 2 (WebAuthn)
key_id = Address (derived from the public key)
expiry = Option<u64> (unix timestamp, None = never expires, stored as u64::MAX in precompile)
limits = Option<Vec<[token, limit]>> (None = unlimited spending)

```


**Signed Format:**

The signed format (`SignedKeyAuthorization`) includes all fields with the `signature` appended:

```
signed_key_authorization = rlp([chain_id, key_type, key_id, expiry?, limits?, signature])

```


The `signature` is a `PrimitiveSignature` (secp256k1, P256, or WebAuthn) signed by the root key.

Note: `expiry` and `limits` use RLP trailing field semantics - they can be omitted entirely when None.

#### Keychain Precompile[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#keychain-precompile)

The Account Keychain precompile (deployed at address `0xAAAAAAAA00000000000000000000000000000000`) manages authorized access keys for accounts. It enables root keys to provision scoped access keys with expiry timestamps and per-TIP20 token spending limits.

**See the [Account Keychain Specification](https://docs.tempo.xyz/AccountKeychain) for complete interface details, storage layout, and implementation.**

#### Protocol Behavior[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#protocol-behavior)

The protocol enforces Access Key authorization and spending limits natively.

##### Transaction Validation[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#transaction-validation-1)

When a TempoTransaction is received, the protocol:

1.  **Identifies the signing key** from the transaction signature
    
    *   If signature is a `Keychain` variant: extracts the `keyId` (address) of the Access Key
    *   Otherwise: treats it as the Root Key (keyId = address(0))
2.  **Validates KeyAuthorization** (if present in transaction)
    
    *   The `key_authorization` field in `TempoTransaction` provisions a NEW Access Key
    *   Root Key MUST sign:
        *   The `key_authorization` digest: `keccak256(rlp([key_type, key_id, expiry, limits]))`
    *   Access Key (being authorized) CAN sign the same tx which it is authorized in.
    *   This enables "authorize and use" in a single transaction
3.  **Sets transaction context**
    
    *   Stores `transactionKey[account] = keyId` in protocol state
    *   Used to enforce authorization hierarchy during execution, can also be used by DApps to see which key authorized the current tx.
4.  **Validates Key Authorization** (for Access Keys)
    
    *   Queries precompile: `getKey(account, keyId)` returns `KeyInfo`
    *   Checks key is active (not revoked)
    *   Checks expiry: `current_timestamp < expiry` (or `expiry == 0` for never expires)
    *   Rejects transaction if validation fails

##### Authorization Hierarchy Enforcement[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#authorization-hierarchy-enforcement)

The protocol enforces a strict two-tier hierarchy:

**Root Key** (keyId = address(0)):

*   The account's primary key (address matches account address)
*   Can call ALL precompile functions
*   No spending limits
*   Can authorize, revoke, and update Access Keys

**Access Keys** (keyId != address(0)):

*   Secondary keys authorized by Root Key
*   CANNOT call mutable precompile functions (`authorizeKey`, `revokeKey`, `updateSpendingLimit`)
*   Precompile functions check: `transactionKey[msg.sender] == 0` before allowing mutations
*   Subject to per-TIP20 token spending limits
*   Can have expiry timestamps

When an Access Key attempts to call `authorizeKey()`, `revokeKey()`, or `updateSpendingLimit()`:

1.  Transaction executes normally until the precompile call
2.  Precompile checks `getTransactionKey()` returns non-zero (Access Key)
3.  Call reverts with `UnauthorizedCaller` error
4.  Entire transaction is reverted

##### Spending Limit Enforcement[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#spending-limit-enforcement)

The protocol tracks and enforces spending limits for TIP20 token transfers:

**Scope:** Only TIP20 `transfer()`, `transferWithMemo()`, `approve()`, and `startReward()` calls are tracked

*   Spending limits only apply when `msg.sender == tx.origin` (direct EOA calls)
*   When a contract makes transfers on behalf of the user, spending limits do NOT apply (e.g., `transferFrom()`)
*   Native value transfers are NOT limited
*   NFT transfers are NOT limited
*   Other asset types are NOT limited

**Tracking:** During transaction execution, when an Access Key's transaction directly calls TIP20 methods:

1.  Protocol intercepts `transfer(to, amount)`, `transferWithMemo()`, `approve(spender, amount)`, and `startReward()` calls
2.  For `transfer`/`transferWithMemo`, the full `amount` is checked against the remaining limit
3.  For `approve`, only **increases** in approval (new approval minus previous allowance) are checked and counted against the limit
4.  Queries: `getRemainingLimit(account, keyId, token)`
5.  Checks: relevant amount (transfer amount or approval increase) `<= remaining_limit`
6.  If check fails: reverts with `SpendingLimitExceeded`
7.  If check passes: decrements the limit by the relevant amount
8.  Updates are stored in precompile state

**Root Key Behavior:** Spending limit checks are skipped entirely (no limits apply)

**Limit Updates:**

*   Limits deplete as tokens are spent
*   Root Key can call `updateSpendingLimit(keyId, token, newLimit)` to set new limits
*   Setting a new limit REPLACES the current remaining amount (does not add to it)
*   Limits do not reset automatically (no time-based periods)

##### Creating and Using KeyAuthorization[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#creating-and-using-keyauthorization)

**First-Time Authorization Flow:**

1.  **Generate Access Key**
    
    ```
// Generate a new P256 or secp256k1 key pair
const accessKey = generateKeyPair("p256"); // or "secp256k1"
const keyId = deriveAddress(accessKey.publicKey);
```

    
2.  **Create Authorization Message**
    
    ```
// Define key parameters
const keyAuth = {
  key_type: SignatureType.P256,      // 1
  key_id: keyId,                     // address derived from public key
  expiry: timestamp + 86400,         // 24 hours from now (or 0 for never)
  limits: [
    { token: USDC_ADDRESS, amount: 1000000000 }, // 1000 USDC (6 decimals)
    { token: DAI_ADDRESS, amount: 500000000000000000000 }  // 500 DAI (18 decimals)
  ]
};
 
// Compute digest: keccak256(rlp([key_type, key_id, expiry, limits]))
const authDigest = computeAuthorizationDigest(keyAuth);
```

    
3.  **Root Key Signs Authorization**
    
    ```
// Sign with Root Key (e.g., passkey prompt)
const rootSignature = await signWithRootKey(authDigest);
```

    
4.  **Build TempoTransaction**
    
    ```
const tx = {
  chain_id: 1,
  nonce: await getNonce(account),
  nonce_key: 0,
  calls: [{ to: recipient, value: 0, input: "0x" }],
  gas_limit: 200000,
  max_fee_per_gas: 1000000000,
  max_priority_fee_per_gas: 1000000000,
  key_authorization: {
    key_type: keyAuth.key_type,
    expiry: keyAuth.expiry,
    limits: keyAuth.limits,
    key_id: keyAuth.key_id,
    signature: rootSignature  // Root Key's signature on authDigest
  },
  // ... other fields
};
```

    
5.  **Access Key Signs Transaction**
    
    ```
// Sign transaction with the NEW Access Key being authorized
const txHash = computeTxSignatureHash(tx);
const accessSignature = await signWithAccessKey(txHash, accessKey);
 
// Wrap in Keychain signature
const finalSignature = {
  Keychain: {
    user_address: account,
    signature: { P256: accessSignature }  // or Secp256k1
  }
};
```

    
6.  **Submit Transaction**
    
    *   Protocol validates Root Key signed the `key_authorization`
    *   Protocol calls `authorizeKey()` on the precompile to store the key
    *   Protocol validates Access Key signature on transaction
    *   Transaction executes with spending limits enforced

**Subsequent Usage (Key Already Authorized):**

```
// Access Key is already authorized, just sign transactions directly
const tx = {
  chain_id: 1,
  nonce: await getNonce(account),
  calls: [{ to: recipient, value: 0, input: calldata }],
  key_authorization: null,  // No authorization needed
  // ... other fields
};
 
const txHash = computeTxSignatureHash(tx);
const accessSignature = await signWithAccessKey(txHash, accessKey);
 
const finalSignature = {
  Keychain: {
    user_address: account,
    signature: { P256: accessSignature }
  }
};
 
// Submit - protocol validates key is authorized and not expired
```


##### Key Management Operations[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#key-management-operations)

**Revoking an Access Key:**

```
// Must be signed by Root Key
const tx = {
  chain_id: 1,
  nonce: await getNonce(account),
  calls: [{
    to: ACCOUNT_KEYCHAIN_ADDRESS,
    value: 0,
    input: encodeCall("revokeKey", [keyId])
  }],
  // ... sign with Root Key
};
```


**Updating Spending Limits:**

```
// Must be signed by Root Key
const tx = {
  chain_id: 1,
  nonce: await getNonce(account),
  calls: [{
    to: ACCOUNT_KEYCHAIN_ADDRESS,
    value: 0,
    input: encodeCall("updateSpendingLimit", [
      keyId,
      USDC_ADDRESS,
      2000000000  // New limit: 2000 USDC
    ])
  }],
  // ... sign with Root Key
};
```


**Note:** After updating, the remaining limit is set to the `newLimit` value, not added to the current remaining amount.

##### Querying Key State[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#querying-key-state)

Applications can query key information and spending limits:

```
// Check if key is authorized and get info
const keyInfo = await precompile.getKey(account, keyId);
// Returns: { signatureType, keyId, expiry }
 
// Check remaining spending limit for a token
const remaining = await precompile.getRemainingLimit(account, keyId, USDC_ADDRESS);
// Returns: uint256 amount remaining
 
// Get which key signed current transaction (callable from contracts)
const currentKey = await precompile.getTransactionKey();
// Returns: address (0x0 for Root Key, keyId for Access Key)
```


Rationale[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#rationale)
------------------------------------------------------------------------------------------

### Signature Type Detection by Length[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#signature-type-detection-by-length)

Using signature length for type detection avoids adding explicit type fields while maintaining deterministic parsing. The chosen lengths (65, 129, variable) are naturally distinct.

### Linear Gas Scaling for Nonce Keys[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#linear-gas-scaling-for-nonce-keys)

The progressive pricing model prevents state bloat while keeping initial keys affordable. The 20,000 gas increment approximates the long-term state cost of maintaining each additional nonce mapping.

### No Nonce Expiry[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#no-nonce-expiry)

Avoiding expiry simplifies the protocol and prevents edge cases where in-flight transactions become invalid. Wallets handle nonce key allocation to prevent conflicts.

### Backwards Compatibility[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#backwards-compatibility)

This spec introduces a new transaction type and does not modify existing transaction processing. Legacy transactions continue to work unchanged. We special case `nonce key = 0` (also referred to as the protocol nonce key) to maintain compatibility with existing nonce behavior.

Gas Costs[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#gas-costs)
------------------------------------------------------------------------------------------

### Signature Verification Gas Schedule[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#signature-verification-gas-schedule)

Different signature types incur different base transaction costs to reflect their computational complexity:



* Signature Type: secp256k1
  * Base Gas Cost: 21,000
  * Calculation: Standard
  * Rationale: Includes 3,000 gas for ecrecover precompile
* Signature Type: P256
  * Base Gas Cost: 26,000
  * Calculation: 21,000 + 5,000
  * Rationale: Base 21k + additional 5k for P256 verification
* Signature Type: WebAuthn
  * Base Gas Cost: 26,000 + variable data cost
  * Calculation: 26,000 + (calldata gas for clientDataJSON)
  * Rationale: Base P256 cost plus variable cost for clientDataJSON based on size
* Signature Type: Keychain
  * Base Gas Cost: Inner signature + 3,000
  * Calculation: primitive_sig_cost + 3,000
  * Rationale: Inner signature cost + key validation overhead (2,100 SLOAD + 900 buffer)


**Rationale:**

*   The base 21,000 gas for standard transactions already includes the cost of secp256k1 signature verification via ecrecover (3,000 gas)
*   [EIP 7951](https://eips.ethereum.org/EIPS/eip-7951) sets P256 verification cost at 6,900 gas. We add 1,100 gas to account for the additional 65 bytes of signature size (129 bytes total vs 64 bytes for secp256k1), giving 8,000 gas total. Since the base 21k already includes 3,000 gas for ecrecover (which P256 doesn't use), the net additional cost is 8,000 - 3,000 = **5,000 gas**.
*   WebAuthn signatures require additional computation to parse and validate the clientDataJSON structure. We cap the total signature size at 2kb. The signature is also charged using the same gas schedule as calldata (16 gas per non-zero byte, 4 gas per zero byte) to prevent the use of this signature space from spam.
*   Keychain signatures wrap a primitive signature and are used by access keys. They add 3,000 gas to cover key validation during transaction validation (cold SLOAD to verify key exists + processing overhead).
*   Individual per-signature-type gas costs allow us to add more advanced verification methods in the future like multisigs, which could have dynamic gas pricing.

### Nonce Key Gas Schedule[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#nonce-key-gas-schedule)

Transactions using parallelizable nonces incur additional costs based on the nonce key usage pattern:

#### Case 1: Protocol Nonce (Key 0)
[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#case-1-protocol-nonce-key-0)

*   **Additional Cost:** 0 gas
*   **Total:** 21,000 gas (base transaction cost)
*   **Rationale:** Maintains backward compatibility with existing transaction flow

#### Case 2: Existing User Nonce Key (nonce > 0)
[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#case-2-existing-user-nonce-key-nonce--0)

*   **Additional Cost:** 5,000 gas
*   **Total:** 26,000 gas
*   **Rationale:** Cold SLOAD (2,100) + warm SSTORE reset (2,900) for incrementing an existing nonce

#### Case 3: New User Nonce Key (nonce == 0)
[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#case-3-new-user-nonce-key-nonce--0)

*   **Additional Cost:** 22,100 gas
*   **Total:** 43,100 gas
*   **Rationale:** Cold SLOAD (2,100) + SSTORE set (20,000) for writing to a new storage slot

**Rationale for Fixed Pricing:**

1.  **Simplicity:** Fixed costs based on actual EVM storage operations are straightforward to reason about
2.  **Storage Pattern Alignment:** Costs directly mirror EVM cold SSTORE costs for new vs existing slots
3.  **State Growth:** Creating new nonce keys incurs the higher cost naturally through SSTORE set pricing

### Key Authorization Gas Schedule[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#key-authorization-gas-schedule)

When a transaction includes a `key_authorization` field to provision a new access key, additional intrinsic gas is charged to cover signature verification and storage operations. This gas is charged **before execution** as part of the transaction's intrinsic gas cost.

#### Gas Components[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#gas-components)



* Component: Signature verification
  * Gas Cost: 3,000 (secp256k1) / 8,000 (P256) / 8,000 + calldata (WebAuthn)
  * Notes: Verifying the root key's signature on the authorization
* Component: Key storage
  * Gas Cost: 22,000
  * Notes: Cold SSTORE to store new key (0→non-zero)
* Component: Overhead buffer
  * Gas Cost: 5,000
  * Notes: Buffer for event emission, storage reads, and other overhead
* Component: Per spending limit
  * Gas Cost: 22,000 each
  * Notes: Cold SSTORE per token limit (0→non-zero)


**Signature verification rationale:** KeyAuthorization requires an _additional_ signature verification beyond the transaction signature. Unlike the transaction signature (where ecrecover cost is included in the base 21k), KeyAuthorization must pay the full verification cost:

*   **secp256k1**: 3,000 gas (ecrecover precompile cost)
*   **P256**: 8,000 gas (6,900 from EIP-7951 + 1,100 for signature size). Note: the transaction signature schedule charges only 5,000 additional gas for P256 because it subtracts the 3,000 ecrecover "savings" already in base 21k. KeyAuthorization pays the full 8,000.
*   **WebAuthn**: 8,000 + calldata gas for webauthn\_data

#### Gas Formula[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#gas-formula)

```
KEY_AUTH_BASE_GAS = 30,000  # For secp256k1 signature (3,000 + 22,000 + 5,000)
KEY_AUTH_BASE_GAS = 35,000  # For P256 signature (5,000 + 3,000 + 22,000 + 5,000)
KEY_AUTH_BASE_GAS = 35,000 + webauthn_calldata_gas  # For WebAuthn signature

PER_LIMIT_GAS = 22,000  # Per spending limit entry

total_key_auth_gas = KEY_AUTH_BASE_GAS + (num_limits * PER_LIMIT_GAS)

```


#### Examples[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#examples)


|Configuration       |Gas Cost|Calculation                |
|--------------------|--------|---------------------------|
|secp256k1, no limits|30,000  |Base only                  |
|secp256k1, 1 limit  |52,000  |30,000 + 22,000            |
|secp256k1, 3 limits |96,000  |30,000 + (3 × 22,000)      |
|P256, no limits     |35,000  |Base with P256 verification|
|P256, 2 limits      |79,000  |35,000 + (2 × 22,000)      |


#### Rationale[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#rationale-1)

1.  **Pre-execution charging**: KeyAuthorization is validated and executed during transaction validation (before the EVM runs), so its gas must be included in intrinsic gas
2.  **Storage cost alignment**: The 22,000 gas per storage slot approximates EVM cold SSTORE costs for new slots
3.  **DoS prevention**: Progressive cost based on number of limits prevents abuse through excessive limit creation

### Reference Pseudocode[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#reference-pseudocode)

```
def calculate_calldata_gas(data: bytes) -> uint256:
    """
    Calculate gas cost for calldata based on zero and non-zero bytes
 
    Args:
        data: bytes to calculate cost for
 
    Returns:
        gas_cost: uint256
    """
    CALLDATA_ZERO_BYTE_GAS = 4
    CALLDATA_NONZERO_BYTE_GAS = 16
 
    gas = 0
    for byte in data:
        if byte == 0:
            gas += CALLDATA_ZERO_BYTE_GAS
        else:
            gas += CALLDATA_NONZERO_BYTE_GAS
 
    return gas
 
 
def calculate_signature_verification_gas(signature: PrimitiveSignature) -> uint256:
    """
    Calculate gas cost for verifying a primitive signature.
 
    Returns the ADDITIONAL gas beyond the base 21k transaction cost.
    - secp256k1: 0 (already included in base 21k via ecrecover)
    - P256: 5,000 (8,000 full cost - 3,000 ecrecover already in base 21k)
    - WebAuthn: 5,000 + calldata gas for webauthn_data
    """
    # P256 full verification cost is 8,000 (6,900 from EIP-7951 + 1,100 for signature size)
    # But base 21k already includes 3,000 for ecrecover, so additional cost is 5,000
    P256_ADDITIONAL_GAS = 5_000
 
    if signature.type == Secp256k1:
        return 0  # Already included in base 21k
    elif signature.type == P256:
        return P256_ADDITIONAL_GAS
    elif signature.type == WebAuthn:
        webauthn_data_gas = calculate_calldata_gas(signature.webauthn_data)
        return P256_ADDITIONAL_GAS + webauthn_data_gas
    else:
        revert("Invalid signature type")
 
 
def calculate_key_authorization_gas(key_auth: SignedKeyAuthorization) -> uint256:
    """
    Calculate the intrinsic gas cost for a KeyAuthorization.
 
    This is charged BEFORE execution as part of transaction validation.
 
    Args:
        key_auth: SignedKeyAuthorization with fields:
            - signature: PrimitiveSignature (root key's signature)
            - limits: Optional[List[TokenLimit]]
 
    Returns:
        gas_cost: uint256
    """
    # Constants - KeyAuthorization pays FULL signature verification costs
    # (not the "additional" costs used for transaction signatures)
    ECRECOVER_GAS = 3_000   # Full ecrecover cost
    P256_FULL_GAS = 8_000   # Full P256 cost (6,900 + 1,100)
    COLD_SSTORE_SET_GAS = 22_000  # Storage cost for new slot
    OVERHEAD_BUFFER = 5_000  # Buffer for event emission, storage reads, etc.
 
    gas = 0
 
    # Step 1: Signature verification cost (full cost, not additional)
    if key_auth.signature.type == Secp256k1:
        gas += ECRECOVER_GAS  # 3,000
    elif key_auth.signature.type == P256:
        gas += P256_FULL_GAS  # 8,000
    elif key_auth.signature.type == WebAuthn:
        webauthn_data_gas = calculate_calldata_gas(key_auth.signature.webauthn_data)
        gas += P256_FULL_GAS + webauthn_data_gas  # 8,000 + calldata
 
    # Step 2: Key storage
    gas += COLD_SSTORE_SET_GAS  # 22,000 - store new key (0 → non-zero)
 
    # Step 3: Overhead buffer
    gas += OVERHEAD_BUFFER  # 5,000
 
    # Step 4: Per-limit storage cost
    num_limits = len(key_auth.limits) if key_auth.limits else 0
    gas += num_limits * COLD_SSTORE_SET_GAS  # 22,000 per limit
 
    return gas
 
 
def calculate_tempo_tx_base_gas(tx):
    """
    Calculate the base gas cost for a TempoTransaction
 
    Args:
        tx: TempoTransaction object with fields:
            - signature: TempoSignature (variable length)
            - nonce_key: uint192
            - nonce: uint64
            - sender_address: address
            - key_authorization: Optional[SignedKeyAuthorization]
 
    Returns:
        total_gas: uint256
    """
 
    # Constants
    BASE_TX_GAS = 21_000
    EXISTING_NONCE_KEY_GAS = 5_000   # Cold SLOAD (2,100) + warm SSTORE reset (2,900)
    NEW_NONCE_KEY_GAS = 22_100       # Cold SLOAD (2,100) + SSTORE set (20,000)
    KEYCHAIN_VALIDATION_GAS = 3_000  # 2,100 SLOAD + 900 processing buffer
 
    # Step 1: Determine signature verification cost
    # For Keychain signatures, use the inner primitive signature
    if tx.signature.type == Keychain:
        inner_sig = tx.signature.inner_signature
    else:
        inner_sig = tx.signature
 
    signature_gas = BASE_TX_GAS + calculate_signature_verification_gas(inner_sig)
 
    # Add keychain validation overhead if using access key
    if tx.signature.type == Keychain:
        signature_gas += KEYCHAIN_VALIDATION_GAS
 
    # Step 2: Calculate nonce key cost
    if tx.nonce_key == 0:
        # Protocol nonce (backward compatible)
        nonce_gas = 0
    else:
        # User nonce key
        current_nonce = get_nonce(tx.sender_address, tx.nonce_key)
 
        if current_nonce > 0:
            # Existing nonce key - cold SLOAD + warm SSTORE reset
            nonce_gas = EXISTING_NONCE_KEY_GAS
        else:
            # New nonce key - cold SLOAD + SSTORE set
            nonce_gas = NEW_NONCE_KEY_GAS
 
    # Step 3: Calculate key authorization cost (if present)
    if tx.key_authorization is not None:
        key_auth_gas = calculate_key_authorization_gas(tx.key_authorization)
    else:
        key_auth_gas = 0
 
    # Step 4: Calculate total base gas
    total_gas = signature_gas + nonce_gas + key_auth_gas
 
    return total_gas
```


Security Considerations[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#security-considerations)
----------------------------------------------------------------------------------------------------------------------

### Mempool DOS Protection[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#mempool-dos-protection)

Transaction pools perform pre-execution validation checks before accepting transactions. These checks are performed for free by the nodes, making them potential DOS vectors. The three primary validation checks are:

1.  **Signature verification** - Must be valid
2.  **Nonce verification** - Must match current account nonce
3.  **Balance check** - Account must have sufficient balance to pay for transaction

This transaction type impacts all three areas:

#### Signature Verification Impact[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#signature-verification-impact)

*   **P256 signatures**: Fixed computational cost similar to ecrecover.
*   **WebAuthn signatures**: Variable cost due to clientDataJSON parsing, but **capped at 2KB total signature size** to prevent abuse
*   **Mitigation**: All signature types have bounded computational costs that are in the same ballpark as standard ecrecover.

#### Nonce Verification Impact[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#nonce-verification-impact)

*   **2D nonce lookup**: Requires additional storage read from nonce precompile
*   **Cost**: Equivalent to a cold SLOAD (~2,100 gas worth of free computation)
*   **Mitigation**: Cost is bounded to a manageable value.

#### Fee Payer Impact[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#fee-payer-impact)

*   **Additional account read**: When fee payer is specified, must fetch fee payer's account to verify balance
*   **Cost**: Effectively doubles the free account access work for sponsored transactions
*   **Mitigation**: Cost is still bounded to a single additional account read.

#### Comparison to Ethereum[](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#comparison-to-ethereum)

The introduction of 7702 delegated accounts already created complex cross-transaction dependencies in the mempool, which prevents any static pool checks from being useful. Because a single transaction can invalidate multiple others by spending balances of multiple accounts

**Assessment:** While this transaction type introduces additional pre-execution validation costs, all costs are bounded to reasonable limits. The mempool complexity issues around cross-transaction dependencies already exist in Ethereum due to 7702 and accounts with code, making static validation inherently difficult. So the incremental cost from this transaction type is acceptable given these existing constraints.


RUST IMPLEMENTATION
https://github.com/tempoxyz/tempo/blob/main/crates/primitives/src/transaction/tempo_transaction.rs
# Tempo
Use Tempo Transactions[](https://docs.tempo.xyz/guide/tempo-transaction#use-tempo-transactions)
-----------------------------------------------------------------------------------------------

Tempo Transactions are a new [EIP-2718](https://github.com/ethereum/EIPs/blob/master/EIPS/eip-2718.md) transaction type, exclusively available on Tempo.

If you're integrating with Tempo, we **strongly recommend** using Tempo Transactions, and not regular Ethereum transactions. Learn more about the benefits below, or follow the guide on issuance [here](https://docs.tempo.xyz/guide/issuance).

Integration Guides[](https://docs.tempo.xyz/guide/tempo-transaction#integration-guides)
---------------------------------------------------------------------------------------

Integrating Tempo Transactions is easy and can be done quickly by a developer in multiple languages. See below for quick links to some of our guides.

If you are an EVM smart contract developer, see the [Tempo extension for Foundry](https://docs.tempo.xyz/sdk/foundry).

Properties[](https://docs.tempo.xyz/guide/tempo-transaction#properties)
-----------------------------------------------------------------------

### Configurable Fee Tokens[](https://docs.tempo.xyz/guide/tempo-transaction#configurable-fee-tokens)

A fee token is a permissionless [TIP-20 token](https://docs.tempo.xyz/protocol/tip20/overview) that can be used to pay fees on Tempo.

When a TIP-20 token is passed as the `fee_token` parameter in a transaction, Tempo's [Fee AMM](https://docs.tempo.xyz/protocol/fees/spec-fee-amm) automatically facilitates conversion between the user's preferred fee token and the validator's preferred token.

Fee sponsorship enables a third party (the fee payer) to pay transaction fees on behalf of the transaction sender.

The process uses dual signature domains: the sender signs their transaction, and then the fee payer signs over the transaction with a special "fee payer envelope" to commit to paying fees for that specific sender.

### Batch Calls[](https://docs.tempo.xyz/guide/tempo-transaction#batch-calls)

Batch calls enable multiple operations to be executed atomically within a single transaction. Instead of sending separate transactions for each operation, you can bundle multiple calls together using the `calls` parameter.

### Access Keys[](https://docs.tempo.xyz/guide/tempo-transaction#access-keys)

Access keys enable you to delegate signing authority from a primary account to a secondary key, such as device-bound non-extractable [WebCrypto key](https://developer.mozilla.org/en-US/docs/Web/API/CryptoKeyPair). The primary account signs a key authorization that grants the access key permission to sign transactions on its behalf.

This authorization is then attached to the next transaction (that can be signed by either the primary or the access key), then all transactions thereafter can be signed by the access key.

### Concurrent Transactions[](https://docs.tempo.xyz/guide/tempo-transaction#concurrent-transactions)

Concurrent transactions enable higher throughput by allowing multiple transactions from the same account to be sent in parallel without waiting for sequential nonce confirmation.

By using different nonce keys, you can submit multiple transactions simultaneously that don't conflict with each other, enabling parallel execution and significantly improved transaction throughput for high-activity accounts.

### Scheduled Transactions[](https://docs.tempo.xyz/guide/tempo-transaction#scheduled-transactions)

Scheduled transactions allow you to sign a transaction in advance and specify a time window for when it can be executed onchain. By setting `validAfter` and `validBefore` timestamps, you define the earliest and latest times the transaction can be included in a block.
# Send Parallel Transactions ⋅ Tempo
Submit multiple transactions in parallel using Tempo's [2D nonces](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction). The `nonceKey` property allow you to send concurrent transactions without waiting for each one to confirm sequentially.

Understanding nonce keys[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#understanding-nonce-keys)
---------------------------------------------------------------------------------------------------------------------

Tempo uses a **2D nonce system** that enables parallel transaction execution:

*   **Protocol nonce (key 0)**: The default sequential nonce. Transactions must be processed in order.
*   **User nonces (keys 1+)**: Independent nonce sequences that allow concurrent transaction submission.

When you send a transaction without specifying a `nonceKey`, it uses the protocol nonce and behaves like a standard sequential transaction. By specifying different nonce keys, you can submit multiple transactions simultaneously without waiting for confirmations.

Key management strategies[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#key-management-strategies)
-----------------------------------------------------------------------------------------------------------------------

There are two ways to specify a `nonceKey` for a transaction:

### Explicit keys[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#explicit-keys)

Explicit keys (1n, 2n, etc.) are best when you want to reuse keys. For high-throughput applications, these keys can be used to load-balance transaction submission to the network in a gas-efficient way. You can track the most recent call on each key in your application, and, once that transaction confirms, the same key can be used for new transactions. This approach is more gas efficient, as provisioning a new `nonceKey` [costs gas](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction#gas-schedule).

### `'random'` keys[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#random-keys)

For simple cases where you don't need to track keys. This approach is recommended when handling bursts of high activity, in which you need to submit transfers to the network and don't care about the added gas costs for provisioning multiple keys.

Demo[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#demo)
-----------------------------------------------------------------------------

By the end of this guide you will understand how to send parallel payments using nonce keys.

#### Send Parallel Payments

demo

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Send 50 AlphaUSD to two recipients in parallel.

Steps[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#steps)
-------------------------------------------------------------------------------

### Set up Wagmi & integrate accounts[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#set-up-wagmi--integrate-accounts)

### Fetch current nonces[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#fetch-current-nonces)

In order to send a transfer on a custom `nonceKey`, you need to know the current nonce value for the keys you will send on.

```
import {  } from 'wagmi/tempo'
import {  } from 'viem'
import {  } from './wagmi.config'
 
 
// Fetch nonces for each key in parallel
const [, ] = await .([
  ..(, { , : 1n }), 
  ..(, { , : 2n }), 
])
 
.('Current nonce for nonceKey 1:', )
.('Current nonce for nonceKey 2:', )
```


### Send concurrent transactions with nonce keys[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#send-concurrent-transactions-with-nonce-keys)

To send multiple transactions in parallel, specify different `nonceKey` values. Each nonce key maintains its own independent sequence:

```
import {  } from 'wagmi/tempo'
import {  } from 'viem'
import {  } from './wagmi.config'
 
const  = '0x...' // your sender account
const  = '0x20c0000000000000000000000000000000000001'
 
const [, ] = await .([
  ..(, { , : 1n }),
  ..(, { , : 2n }),
])
 
// Send both transfers in parallel using different nonce keys
const [, ] = await .([
  ..(, {
    : ('100', 6),
    : '0x70997970C51812dc3A010C7d01b50e0d17dc79C8',
    : ,
    : 1n, 
    : (), 
  }),
  ..(, {
    : ('50', 6),
    : '0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC',
    : ,
    : 2n, 
    : (), 
  }),
])
 
.('Transaction 1:', )
.('Transaction 2:', )
```


Recipes[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#recipes)
-----------------------------------------------------------------------------------

### Use random nonce keys[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#use-random-nonce-keys)

For simple cases where you don't need to manage specific keys, use `'random'` to automatically generate a unique nonce key:

```
import {  } from 'wagmi/tempo'
import {  } from 'viem'
import {  } from './wagmi.config'
 
// Using 'random' automatically generates a unique nonce key
const  = await ..(, {
  : ('100', 6),
  : '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEbb',
  : '0x20c0000000000000000000000000000000000001',
  : 'random', 
})
 
.('Transaction hash:', )
```


### Query active nonce keys[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#query-active-nonce-keys)

Track how many nonce keys your account is using:

```
import { client } from './viem.config'
 
// Get the count of active nonce keys for an account
const count = await client.nonce.getNonceKeyCount({
  account: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEbb',
})
 
console.log('Active nonce keys:', count)
```


Best Practices[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#best-practices)
-------------------------------------------------------------------------------------------------

### When to use nonce keys[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#when-to-use-nonce-keys)

Use nonce keys when you need to:

*   Send multiple independent transactions simultaneously
*   Build high-throughput applications that can't wait for sequential confirmations
*   Process payments to multiple recipients concurrently

### When to use batch transactions instead[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#when-to-use-batch-transactions-instead)

Use [batch transactions](https://docs.tempo.xyz/guide/use-accounts/batch-transactions) instead of nonce keys when:

*   Operations need to be atomic
*   Calls have sequential dependencies
*   You want a single transaction fee for multiple operations

Batch transactions are not as appropriate for the payments to multiple recipients use case, because if a single payment fails, all the calls in the batch transaction roll back.

Learning Resources[](https://docs.tempo.xyz/guide/payments/send-parallel-transactions#learning-resources)
---------------------------------------------------------------------------------------------------------
