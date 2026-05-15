# Internal Oracles (/docs/builders/internal-oracles)

Internal price oracles deployed on RISE Testnet.

## Oracle Addresses

| Ticker | Address                                                                                                                                                                                                                                                        |
| ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ETH    | <span className="inline-flex items-center">[`0x7114E2537851e727678DE5a96C8eE5d0Ca14f03D`](https://explorer.testnet.riselabs.xyz/address/0x7114E2537851e727678DE5a96C8eE5d0Ca14f03D)<CopyAddress address="0x7114E2537851e727678DE5a96C8eE5d0Ca14f03D" /></span> |
| USDC   | <span className="inline-flex items-center">[`0x50524C5bDa18aE25C600a8b81449B9CeAeB50471`](https://explorer.testnet.riselabs.xyz/address/0x50524C5bDa18aE25C600a8b81449B9CeAeB50471)<CopyAddress address="0x50524C5bDa18aE25C600a8b81449B9CeAeB50471" /></span> |
| USDT   | <span className="inline-flex items-center">[`0x9190159b1bb78482Dca6EBaDf03ab744de0c0197`](https://explorer.testnet.riselabs.xyz/address/0x9190159b1bb78482Dca6EBaDf03ab744de0c0197)<CopyAddress address="0x9190159b1bb78482Dca6EBaDf03ab744de0c0197" /></span> |
| BTC    | <span className="inline-flex items-center">[`0xadDAEd879D549E5DBfaf3e35470C20D8C50fDed0`](https://explorer.testnet.riselabs.xyz/address/0xadDAEd879D549E5DBfaf3e35470C20D8C50fDed0)<CopyAddress address="0xadDAEd879D549E5DBfaf3e35470C20D8C50fDed0" /></span> |

## Usage

The oracle price for each asset can be fetched by calling the `latest_answer` function on the respective oracle address.

```solidity
interface IPriceOracle {
    function latest_answer() external view returns (int256);
}

// Example: Get ETH price
IPriceOracle ethOracle = IPriceOracle(0x7114E2537851e727678DE5a96C8eE5d0Ca14f03D);
int256 ethPrice = ethOracle.latest_answer();
```
