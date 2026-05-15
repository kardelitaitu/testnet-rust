# Testnet Tokens (/docs/builders/testnet-tokens)

ERC20 tokens deployed on RISE Testnet (Chain ID: 11155931).

These tokens have been minted purely for testing purposes. They hold no value.

You can acquire tokens via the faucet on the [Testnet Portal](https://portal.risechain.com).

## Token Addresses

| Name            | Symbol | Decimals | Address                                                                                                                                                                                                                                                        |
| --------------- | ------ | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Wrapped ETH     | WETH   | 18       | <span className="inline-flex items-center">[`0x4200000000000000000000000000000000000006`](https://explorer.testnet.riselabs.xyz/address/0x4200000000000000000000000000000000000006)<CopyAddress address="0x4200000000000000000000000000000000000006" /></span> |
| USD Coin        | USDC   | 6        | <span className="inline-flex items-center">[`0x8a93d247134d91e0de6f96547cb0204e5be8e5d8`](https://explorer.testnet.riselabs.xyz/address/0x8a93d247134d91e0de6f96547cb0204e5be8e5d8)<CopyAddress address="0x8a93d247134d91e0de6f96547cb0204e5be8e5d8" /></span> |
| Tether USD      | USDT   | 8        | <span className="inline-flex items-center">[`0x40918ba7f132e0acba2ce4de4c4baf9bd2d7d849`](https://explorer.testnet.riselabs.xyz/address/0x40918ba7f132e0acba2ce4de4c4baf9bd2d7d849)<CopyAddress address="0x40918ba7f132e0acba2ce4de4c4baf9bd2d7d849" /></span> |
| Wrapped Bitcoin | WBTC   | 18       | <span className="inline-flex items-center">[`0xf32d39ff9f6aa7a7a64d7a4f00a54826ef791a55`](https://explorer.testnet.riselabs.xyz/address/0xf32d39ff9f6aa7a7a64d7a4f00a54826ef791a55)<CopyAddress address="0xf32d39ff9f6aa7a7a64d7a4f00a54826ef791a55" /></span> |
| RISE            | RISE   | 18       | <span className="inline-flex items-center">[`0xd6e1afe5ca8d00a2efc01b89997abe2de47fdfaf`](https://explorer.testnet.riselabs.xyz/address/0xd6e1afe5ca8d00a2efc01b89997abe2de47fdfaf)<CopyAddress address="0xd6e1afe5ca8d00a2efc01b89997abe2de47fdfaf" /></span> |
| Mog Coin        | MOG    | 18       | <span className="inline-flex items-center">[`0x99dbe4aea58e518c50a1c04ae9b48c9f6354612f`](https://explorer.testnet.riselabs.xyz/address/0x99dbe4aea58e518c50a1c04ae9b48c9f6354612f)<CopyAddress address="0x99dbe4aea58e518c50a1c04ae9b48c9f6354612f" /></span> |
| Pepe            | PEPE   | 18       | <span className="inline-flex items-center">[`0x6f6f570f45833e249e27022648a26f4076f48f78`](https://explorer.testnet.riselabs.xyz/address/0x6f6f570f45833e249e27022648a26f4076f48f78)<CopyAddress address="0x6f6f570f45833e249e27022648a26f4076f48f78" /></span> |

## Contract Features

All tokens implement:

* ERC20 standard functionality (transfer, approve, transferFrom)
* Custom decimals support (6 to 18)
* Minting capability (restricted to owner)
* Burning capability (anyone can burn their own tokens)

### WETH

The WETH token at `0x4200000000000000000000000000000000000006` is a predeploy contract with additional functionality:

* Wrap ETH by sending ETH to the contract or calling `deposit()`
* Unwrap WETH by calling `withdraw(uint)`
* Standard ERC20 interface for wrapped ETH

## Example Commands

```bash
# Check token balance
cast call <TOKEN_ADDRESS> "balanceOf(address)(uint256)" <YOUR_ADDRESS> --rpc-url https://testnet.riselabs.xyz

# Transfer tokens
cast send <TOKEN_ADDRESS> "transfer(address,uint256)(bool)" <RECIPIENT> <AMOUNT> --private-key $PRIVATE_KEY --rpc-url https://testnet.riselabs.xyz

# Wrap ETH
cast send 0x4200000000000000000000000000000000000006 "deposit()" --value <AMOUNT_WEI> --private-key $PRIVATE_KEY --rpc-url https://testnet.riselabs.xyz

# Unwrap WETH
cast send 0x4200000000000000000000000000000000000006 "withdraw(uint256)" <AMOUNT_WEI> --private-key $PRIVATE_KEY --rpc-url https://testnet.riselabs.xyz
```
