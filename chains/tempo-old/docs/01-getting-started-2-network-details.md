# Connect to the Network ⋅ Tempo
You can connect with the Tempo Testnet like you would with any other EVM chain.

Connect using a Browser Wallet[](https://docs.tempo.xyz/quickstart/connection-details#connect-using-a-browser-wallet)
---------------------------------------------------------------------------------------------------------------------

Click on your browser wallet below to automatically connect it to the Tempo Testnet.

No browser wallets found.

Connect via CLI[](https://docs.tempo.xyz/quickstart/connection-details#connect-via-cli)
---------------------------------------------------------------------------------------

To connect via CLI, we recommend using `[cast](https://getfoundry.sh/cast/overview/)`, which is a command-line tool for interacting with Ethereum networks. To install cast, you can read more in the [Foundry SDK docs](https://docs.tempo.xyz/sdk/foundry#get-started-with-foundry).

```
# Check block height (should be steadily increasing)
cast block-number --rpc-url https://rpc.moderato.tempo.xyz
```


Direct Connection Details[](https://docs.tempo.xyz/quickstart/connection-details#direct-connection-details)
-----------------------------------------------------------------------------------------------------------

If you're manually connecting to Tempo Testnet, you can use the following details:


|Property      |Value                         |
|--------------|------------------------------|
|Network Name  |Tempo Testnet (Moderato)      |
|Currency      |USD                           |
|Chain ID      |42431                         |
|HTTP URL      |https://rpc.moderato.tempo.xyz|
|WebSocket URL |wss://rpc.moderato.tempo.xyz  |
|Block Explorer|https://explore.tempo.xyz     |

# Faucet ⋅ Tempo
Get test stablecoins on Tempo testnet.

The faucet funds the following assets.


|Asset   |Address                                   |Amount|
|--------|------------------------------------------|------|
|pathUSD |0x20c0000000000000000000000000000000000000|1M    |
|AlphaUSD|0x20c0000000000000000000000000000000000001|1M    |
|BetaUSD |0x20c0000000000000000000000000000000000002|1M    |
|ThetaUSD|0x20c0000000000000000000000000000000000003|1M    |
# EVM Differences ⋅ Tempo
Tempo is fully compatible with the Ethereum Virtual Machine (EVM), targeting the **Osaka** EVM hard fork. Developers can deploy and interact with smart contracts using the same tools, languages, and frameworks they use on Ethereum, such as Solidity, Foundry, and Hardhat. All Ethereum JSON-RPC methods work out of the box.

While the execution environment mirrors Ethereum's, Tempo introduces some differences optimized for payments, described below.

Wallet Differences[](https://docs.tempo.xyz/quickstart/evm-compatibility#wallet-differences)
--------------------------------------------------------------------------------------------

By default, all existing functionality will work for EVM-compatible wallets, with only a few quirks. For developers of wallets, we strongly encourage you to implement support for Tempo Transactions over regular EVM transactions. See the [transaction differences](https://docs.tempo.xyz/quickstart/evm-compatibility#transaction-differences) for more.

### Handling ETH (native token) Balance Checks[](https://docs.tempo.xyz/quickstart/evm-compatibility#handling-eth-native-token-balance-checks)

Remember that on Tempo, there is no native gas token.

Many wallets and applications check a user's "native account balance" before letting them complete some action. In this scenario, you might see an error message like "Insufficient balance".

This stems from the return value of the `eth_getBalance` RPC method. When a wallet calls this method, it expects a hex string representing the "native token balance", hard-coded to be represented as an 18-decimal place number.

On Tempo, the `eth_getBalance` method returns a hex string representing an extremely large number. Specifically it returns: `0x9612084f0316e0ebd5182f398e5195a51b5ca47667d4c9b26c9b26c9b26c9b2` which is represented in decimals as 4.242424242424242e+75.

Our recommendation to wallets and to applications using this method is to remove this balance check, and to not represent any "native balance" in your user's UI. This will allow users to complete actions without being blocked by balance checks.

We endorse [this proposed ERC](https://github.com/ethereum/ERCs/pull/1220) to standardize this behavior.

### Specifying a Native Token Currency Symbol[](https://docs.tempo.xyz/quickstart/evm-compatibility#specifying-a-native-token-currency-symbol)

Sometimes wallets will need to specify the currency symbol for the native token. On Tempo, there is no native token, but fees are denominated in USD. So, we recommend using the currency symbol "USD".

Transaction Differences[](https://docs.tempo.xyz/quickstart/evm-compatibility#transaction-differences)
------------------------------------------------------------------------------------------------------

### Dealing with the fee token selection[](https://docs.tempo.xyz/quickstart/evm-compatibility#dealing-with-the-fee-token-selection)

Tempo does not have a native gas token. Instead, fees are denominated in USD and fees can be paid in an stablecoin. For Tempo Transactions, the `fee_token` field can be set to any TIP-20 token, and fees are paid in that token.

If your transactions are not using Tempo Transactions, there is a cascading fee token selection algorithm that determines the default fee token based on the user's preferences and the contract being called.

This preference system is specified [here](https://docs.tempo.xyz/protocol/fees/spec-fee#fee-token-preferences) in detail.

#### Consideration 1: Setting a user default fee token[](https://docs.tempo.xyz/quickstart/evm-compatibility#consideration-1-setting-a-user-default-fee-token)

As specified in the preference system above, the simplest way to specify the fee token for a user is to set the user default fee token. Read about how to do that [here](https://docs.tempo.xyz/protocol/fees/spec-fee#account-level) on behalf of an account.

#### Consideration 2: Paying fees in the TIP-20 contract being interacted with[](https://docs.tempo.xyz/quickstart/evm-compatibility#consideration-2-paying-fees-in-the-tip-20-contract-being-interacted-with)

If the user is calling a method on a TIP-20 token (e.g., `transfer`), the default fee token is that token itself. For example, if the user is calling the `transfer` method on a TIP-20 token with a symbol of "USDC", the default fee token would be "USDC".

Importantly, note that the `amount` field in this case is sent in full. So, if the user is calling the `transfer` method on a TIP-20 token with a symbol of "USDC" with the `amount` field set to 1000, the full amount of the token will be transferred **and** the sender's balance will be reduced by the amount spent in fees. So, the recipient will receive 1000 USDC.

#### Consideration 3: The fallback in the case of a non-TIP-20 contract[](https://docs.tempo.xyz/quickstart/evm-compatibility#consideration-3-the-fallback-in-the-case-of-a-non-tip-20-contract)

If the user is calling a contract that is not a TIP-20 token, the EVM transaction will default to the pathUSD token. Thus, in order to send transactions to non-TIP-20 contracts, the wallet must hold some balance of pathUSD.

On the Tempo Testnet, pathUSD is available from the [faucet](https://docs.tempo.xyz/quickstart/faucet).

If a wallet wants to submit a non-TIP20 transaction without having to submit the above transaction, we recommend investing in using [Tempo Transactions](https://docs.tempo.xyz/quickstart/integrate-tempo#tempo-transactions) instead.

VM Layer Differences[](https://docs.tempo.xyz/quickstart/evm-compatibility#vm-layer-differences)
------------------------------------------------------------------------------------------------

At the VM layer, all opcodes are supported out of the box. Due to the lack of a native token, native token balance is always returning zero balances.

### Balance Opcodes and RPC Methods[](https://docs.tempo.xyz/quickstart/evm-compatibility#balance-opcodes-and-rpc-methods)


|Feature                |Behavior on Tempo   |Alternatives                |
|-----------------------|--------------------|----------------------------|
|BALANCE and SELFBALANCE|Will always return 0|Use TIP-20 balanceOf instead|
|CALLVALUE              |Will always return 0|There is no alternative     |


Consensus & Finality[](https://docs.tempo.xyz/quickstart/evm-compatibility#consensus--finality)
-----------------------------------------------------------------------------------------------

Tempo uses **Simplex BFT consensus** with a permissioned validator set at launch, providing deterministic finality, unlike Ethereum's finality gadget which takes approximately 12 minutes.

Block times are targeted at ~0.5 seconds compared to Ethereum's ~12 second slots.
# Predeployed Contracts ⋅ Tempo
System Contracts[](https://docs.tempo.xyz/quickstart/predeployed-contracts#system-contracts)
--------------------------------------------------------------------------------------------

Core protocol contracts that power Tempo's features.


|Contract        |Address                                   |Description                        |
|----------------|------------------------------------------|-----------------------------------|
|TIP-20 Factory  |0x20fc000000000000000000000000000000000000|Create new TIP-20 tokens           |
|Fee Manager     |0xfeec000000000000000000000000000000000000|Handle fee payments and conversions|
|Stablecoin DEX  |0xdec0000000000000000000000000000000000000|Enshrined DEX for stablecoin swaps |
|TIP-403 Registry|0x403c000000000000000000000000000000000000|Transfer policy registry           |
|pathUSD         |0x20c0000000000000000000000000000000000000|First stablecoin deployed          |


Standard Utilities[](https://docs.tempo.xyz/quickstart/predeployed-contracts#standard-utilities)
------------------------------------------------------------------------------------------------

Popular Ethereum contracts deployed for convenience.



* Contract: Multicall3
  * Address: 0xcA11bde05977b3631167028862bE2a173976CA11
  * Description: Batch multiple calls in one transaction
* Contract: CreateX
  * Address: 0xba5Ed099633D3B313e4D5F7bdc1305d3c28ba5Ed
  * Description: Deterministic contract deployment
* Contract: Permit2
  * Address: 0x000000000022d473030f116ddee9f6b43ac78ba3
  * Description: Token approvals and transfers
* Contract: Arachnid Create2 Factory
  * Address: 0x4e59b44847b379578588920cA78FbF26c0B4956C
  * Description: CREATE2 deployment proxy
* Contract: Safe Deployer
  * Address: 0x914d7Fec6aaC8cd542e72Bca78B30650d45643d7
  * Description: Safe deployer contract


Contract ABIs[](https://docs.tempo.xyz/quickstart/predeployed-contracts#contract-abis)
--------------------------------------------------------------------------------------

ABIs for these contracts are available in the SDK:

```
import { Abis } from 'viem/tempo'
 
const tip20Abi = Abis.tip20
const tip20FactoryAbi = Abis.tip20Factory
const stablecoinDexAbi = Abis.stablecoinDex
const feeManagerAbi = Abis.feeManager
const feeAmmAbi = Abis.feeAmm
// ...
```
