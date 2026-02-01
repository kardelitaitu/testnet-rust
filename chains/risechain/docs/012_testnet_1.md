# Contract Addresses (/docs/builders/contract-addresses)

This page provides a reference for all contract addresses on RISE Testnet.

## Pre-deployed Contracts

These contracts are pre-deployed and available from genesis.

| Contract Name                | Description                                           | Address                                                                                                                                                                                                                                                        |
| ---------------------------- | ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Create2Deployer              | Helper for CREATE2 opcode usage                       | <span className="inline-flex items-center">[`0x13b0D85CcB8bf860b6b79AF3029fCA081AE9beF2`](https://explorer.testnet.riselabs.xyz/address/0x13b0D85CcB8bf860b6b79AF3029fCA081AE9beF2)<CopyAddress address="0x13b0D85CcB8bf860b6b79AF3029fCA081AE9beF2" /></span> |
| DeterministicDeploymentProxy | Integrated with Foundry for deterministic deployments | <span className="inline-flex items-center">[`0x4e59b44847b379578588920ca78fbf26c0b4956c`](https://explorer.testnet.riselabs.xyz/address/0x4e59b44847b379578588920ca78fbf26c0b4956c)<CopyAddress address="0x4e59b44847b379578588920ca78fbf26c0b4956c" /></span> |
| MultiCall3                   | Allows bundling multiple transactions                 | <span className="inline-flex items-center">[`0xcA11bde05977b3631167028862bE2a173976CA11`](https://explorer.testnet.riselabs.xyz/address/0xcA11bde05977b3631167028862bE2a173976CA11)<CopyAddress address="0xcA11bde05977b3631167028862bE2a173976CA11" /></span> |
| GnosisSafe (v1.3.0)          | Multisignature wallet                                 | <span className="inline-flex items-center">[`0x69f4D1788e39c87893C980c06EdF4b7f686e2938`](https://explorer.testnet.riselabs.xyz/address/0x69f4D1788e39c87893C980c06EdF4b7f686e2938)<CopyAddress address="0x69f4D1788e39c87893C980c06EdF4b7f686e2938" /></span> |
| GnosisSafeL2 (v1.3.0)        | Events-based implementation of GnosisSafe             | <span className="inline-flex items-center">[`0xfb1bffC9d739B8D520DaF37dF666da4C687191EA`](https://explorer.testnet.riselabs.xyz/address/0xfb1bffC9d739B8D520DaF37dF666da4C687191EA)<CopyAddress address="0xfb1bffC9d739B8D520DaF37dF666da4C687191EA" /></span> |
| MultiSendCallOnly (v1.3.0)   | Batches multiple transactions (calls only)            | <span className="inline-flex items-center">[`0xA1dabEF33b3B82c7814B6D82A79e50F4AC44102B`](https://explorer.testnet.riselabs.xyz/address/0xA1dabEF33b3B82c7814B6D82A79e50F4AC44102B)<CopyAddress address="0xA1dabEF33b3B82c7814B6D82A79e50F4AC44102B" /></span> |
| MultiSend (v1.3.0)           | Batches multiple transactions                         | <span className="inline-flex items-center">[`0x998739BFdAAdde7C933B942a68053933098f9EDa`](https://explorer.testnet.riselabs.xyz/address/0x998739BFdAAdde7C933B942a68053933098f9EDa)<CopyAddress address="0x998739BFdAAdde7C933B942a68053933098f9EDa" /></span> |
| Permit2                      | Next-generation token approval system                 | <span className="inline-flex items-center">[`0x000000000022D473030F116dDEE9F6B43aC78BA3`](https://explorer.testnet.riselabs.xyz/address/0x000000000022D473030F116dDEE9F6B43aC78BA3)<CopyAddress address="0x000000000022D473030F116dDEE9F6B43aC78BA3" /></span> |
| EntryPoint (v0.7.0)          | ERC-4337 entry point for account abstraction          | <span className="inline-flex items-center">[`0x0000000071727De22E5E9d8BAf0edAc6f37da032`](https://explorer.testnet.riselabs.xyz/address/0x0000000071727De22E5E9d8BAf0edAc6f37da032)<CopyAddress address="0x0000000071727De22E5E9d8BAf0edAc6f37da032" /></span> |
| SenderCreator (v0.7.0)       | Helper for EntryPoint                                 | <span className="inline-flex items-center">[`0xEFC2c1444eBCC4Db75e7613d20C6a62fF67A167C`](https://explorer.testnet.riselabs.xyz/address/0xEFC2c1444eBCC4Db75e7613d20C6a62fF67A167C)<CopyAddress address="0xEFC2c1444eBCC4Db75e7613d20C6a62fF67A167C" /></span> |
| WETH                         | Wrapped ETH                                           | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000006`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000006)<CopyAddress address="0x4200000000000000000000000000000000000006" /></span> |

## L1 (Sepolia) System Contracts

These contracts are deployed on the Sepolia Ethereum testnet and handle the communication between L1 and RISE Testnet.

| Contract Name                     | Description                                         | Address                                                                                                                                                                                                                                               |
| --------------------------------- | --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| AnchorStateRegistryProxy          | Stores state roots of the L2 chain                  | <span className="inline-flex items-center">[`0x5ca4bfe196aa3a1ed9f8522f224ec5a7a7277d5a`](https://sepolia.etherscan.io/address/0x5ca4bfe196aa3a1ed9f8522f224ec5a7a7277d5a)<CopyAddress address="0x5ca4bfe196aa3a1ed9f8522f224ec5a7a7277d5a" /></span> |
| BatchSubmitter                    | Submits batches of transactions                     | <span className="inline-flex items-center">[`0x45Bd8Bc15FfC21315F8a1e3cdF67c73b487768e8`](https://sepolia.etherscan.io/address/0x45Bd8Bc15FfC21315F8a1e3cdF67c73b487768e8)<CopyAddress address="0x45Bd8Bc15FfC21315F8a1e3cdF67c73b487768e8" /></span> |
| Challenger                        | Handles challenges to invalid state transitions     | <span className="inline-flex items-center">[`0xb49077bAd82968A1119B9e717DBCFb9303E91f0F`](https://sepolia.etherscan.io/address/0xb49077bAd82968A1119B9e717DBCFb9303E91f0F)<CopyAddress address="0xb49077bAd82968A1119B9e717DBCFb9303E91f0F" /></span> |
| DelayedWETHProxy                  | Wrapped ETH with withdrawal delay                   | <span className="inline-flex items-center">[`0x3547e7b4af6f0a2d626c72fd7066b939e8489450`](https://sepolia.etherscan.io/address/0x3547e7b4af6f0a2d626c72fd7066b939e8489450)<CopyAddress address="0x3547e7b4af6f0a2d626c72fd7066b939e8489450" /></span> |
| DisputeGameFactoryProxy           | Creates dispute games for challenging invalid state | <span className="inline-flex items-center">[`0x790e18c477bfb49c784ca0aed244648166a5022b`](https://sepolia.etherscan.io/address/0x790e18c477bfb49c784ca0aed244648166a5022b)<CopyAddress address="0x790e18c477bfb49c784ca0aed244648166a5022b" /></span> |
| L1CrossDomainMessengerProxy       | Handles message passing from L1 to L2               | <span className="inline-flex items-center">[`0xcc1c4f905d0199419719f3c3210f43bb990953fc`](https://sepolia.etherscan.io/address/0xcc1c4f905d0199419719f3c3210f43bb990953fc)<CopyAddress address="0xcc1c4f905d0199419719f3c3210f43bb990953fc" /></span> |
| L1ERC721BridgeProxy               | Bridge for NFTs between L1 and L2                   | <span className="inline-flex items-center">[`0xfc197687ac16218bad8589420978f40097c42a44`](https://sepolia.etherscan.io/address/0xfc197687ac16218bad8589420978f40097c42a44)<CopyAddress address="0xfc197687ac16218bad8589420978f40097c42a44" /></span> |
| L1StandardBridgeProxy             | Bridge for ETH and ERC20 tokens                     | <span className="inline-flex items-center">[`0xe9a531a5d7253c9823c74af155d22fe14568b610`](https://sepolia.etherscan.io/address/0xe9a531a5d7253c9823c74af155d22fe14568b610)<CopyAddress address="0xe9a531a5d7253c9823c74af155d22fe14568b610" /></span> |
| MIPS                              | MIPS verification for fault proofs                  | <span className="inline-flex items-center">[`0xaa33f21ada0dc6c40a33d94935de11a0b754fec4`](https://sepolia.etherscan.io/address/0xaa33f21ada0dc6c40a33d94935de11a0b754fec4)<CopyAddress address="0xaa33f21ada0dc6c40a33d94935de11a0b754fec4" /></span> |
| OptimismMintableERC20FactoryProxy | Factory for creating bridged tokens on L2           | <span className="inline-flex items-center">[`0xb9b92645886135838abd71a1bbf55e34260dabf6`](https://sepolia.etherscan.io/address/0xb9b92645886135838abd71a1bbf55e34260dabf6)<CopyAddress address="0xb9b92645886135838abd71a1bbf55e34260dabf6" /></span> |
| OptimismPortalProxy               | Main entry point for L1 to L2 transactions          | <span className="inline-flex items-center">[`0x77cce5cd26c75140c35c38104d0c655c7a786acb`](https://sepolia.etherscan.io/address/0x77cce5cd26c75140c35c38104d0c655c7a786acb)<CopyAddress address="0x77cce5cd26c75140c35c38104d0c655c7a786acb" /></span> |
| PreimageOracle                    | Stores preimages for fault proofs                   | <span className="inline-flex items-center">[`0xca8f0068cd4894e1c972701ce8da7f934444717d`](https://sepolia.etherscan.io/address/0xca8f0068cd4894e1c972701ce8da7f934444717d)<CopyAddress address="0xca8f0068cd4894e1c972701ce8da7f934444717d" /></span> |
| Proposer                          | Proposes new L2 state roots                         | <span className="inline-flex items-center">[`0x407379B3eBd88B4E92F8fF8930D244B592D65c06`](https://sepolia.etherscan.io/address/0x407379B3eBd88B4E92F8fF8930D244B592D65c06)<CopyAddress address="0x407379B3eBd88B4E92F8fF8930D244B592D65c06" /></span> |
| SystemConfigProxy                 | Configuration for the RISE system                   | <span className="inline-flex items-center">[`0x5088a091bd20343787c5afc95aa002d13d9f3535`](https://sepolia.etherscan.io/address/0x5088a091bd20343787c5afc95aa002d13d9f3535)<CopyAddress address="0x5088a091bd20343787c5afc95aa002d13d9f3535" /></span> |
| UnsafeBlockSigner                 | Signs blocks in development mode                    | <span className="inline-flex items-center">[`0x8d451372bAdE8723F45BF5134550017F639dFb11`](https://sepolia.etherscan.io/address/0x8d451372bAdE8723F45BF5134550017F639dFb11)<CopyAddress address="0x8d451372bAdE8723F45BF5134550017F639dFb11" /></span> |

## L2 (RISE Testnet) System Contracts

These are the predeploy contracts on RISE Testnet.

| Contract Name                 | Description                           | Address                                                                                                                                                                                                                                                        |
| ----------------------------- | ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| L2ToL1MessagePasser           | Initiates withdrawals to L1           | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000016`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000016)<CopyAddress address="0x4200000000000000000000000000000000000016" /></span> |
| L2CrossDomainMessenger        | Handles message passing from L2 to L1 | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000007`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000007)<CopyAddress address="0x4200000000000000000000000000000000000007" /></span> |
| L2StandardBridge              | L2 side of the token bridge           | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000010`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000010)<CopyAddress address="0x4200000000000000000000000000000000000010" /></span> |
| L2ERC721Bridge                | L2 side of the NFT bridge             | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000014`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000014)<CopyAddress address="0x4200000000000000000000000000000000000014" /></span> |
| SequencerFeeVault             | Collects sequencer fees               | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000011`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000011)<CopyAddress address="0x4200000000000000000000000000000000000011" /></span> |
| OptimismMintableERC20Factory  | Creates standard bridged tokens       | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000012`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000012)<CopyAddress address="0x4200000000000000000000000000000000000012" /></span> |
| OptimismMintableERC721Factory | Creates bridged NFTs                  | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000017`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000017)<CopyAddress address="0x4200000000000000000000000000000000000017" /></span> |
| L1Block                       | Provides L1 block information         | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000015`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000015)<CopyAddress address="0x4200000000000000000000000000000000000015" /></span> |
| GasPriceOracle                | Provides gas price information        | <span className="inline-flex items-center">[`0x420000000000000000000000000000000000000F`](https://explorer.testnet.riselabs.xyz/address/0x420000000000000000000000000000000000000F)<CopyAddress address="0x420000000000000000000000000000000000000F" /></span> |
| ProxyAdmin                    | Admin for proxy contracts             | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000018`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000018)<CopyAddress address="0x4200000000000000000000000000000000000018" /></span> |
| BaseFeeVault                  | Collects base fee                     | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000019`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000019)<CopyAddress address="0x4200000000000000000000000000000000000019" /></span> |
| L1FeeVault                    | Collects L1 data fees                 | <span className="inline-flex items-center">[`0x420000000000000000000000000000000000001A`](https://explorer.testnet.riselabs.xyz/address/0x420000000000000000000000000000000000001A)<CopyAddress address="0x420000000000000000000000000000000000001A" /></span> |
| GovernanceToken               | RISE governance token                 | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000042`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000042)<CopyAddress address="0x4200000000000000000000000000000000000042" /></span> |
| SchemaRegistry                | EAS schema registry                   | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000020`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000020)<CopyAddress address="0x4200000000000000000000000000000000000020" /></span> |
| EAS                           | Ethereum Attestation Service          | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000021`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000021)<CopyAddress address="0x4200000000000000000000000000000000000021" /></span> |

## Usage Examples

### Bridging ETH from L1 to L2

```solidity
// On Sepolia (L1)
IL1StandardBridge bridge = IL1StandardBridge(0xe9a531a5d7253c9823c74af155d22fe14568b610);

// Deposit ETH to L2
bridge.depositETH{value: amount}(
    minGasLimit,
    emptyBytes  // No additional data
);
```

### Sending a Message from L2 to L1

```solidity
// On RISE Testnet (L2)
IL2CrossDomainMessenger messenger = IL2CrossDomainMessenger(0x4200000000000000000000000000000000000007);

// Send message to L1
messenger.sendMessage(
    targetL1Address,
    abi.encodeWithSignature("someFunction(uint256)", value),
    minGasLimit
);
```
