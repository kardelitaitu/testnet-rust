# Add Funds to Your Balance ⋅ Tempo
Get test tokens to start building on Tempo testnet.

Direct Access[](https://docs.tempo.xyz/guide/use-accounts/add-funds#direct-access)
----------------------------------------------------------------------------------

You can access the faucet directly here in the docs.

1

Connect your browser wallet.

No browser wallets found.

2

Add testnet funds to an address.

Testnet Faucet RPC[](https://docs.tempo.xyz/guide/use-accounts/add-funds#testnet-faucet-rpc)
--------------------------------------------------------------------------------------------

The public testnet also offers an RPC endpoint for requesting test tokens. The faucet endpoint is only available at the official Tempo testnet RPC endpoint.

Request test tokens using the `tempo_fundAddress` RPC method:

```
cast rpc tempo_fundAddress <YOUR_ADDRESS> \
  --rpc-url https://rpc.moderato.tempo.xyz
```


Replace `<YOUR_ADDRESS>` with your wallet address.

What You'll Receive[](https://docs.tempo.xyz/guide/use-accounts/add-funds#what-youll-receive)
---------------------------------------------------------------------------------------------

The faucet provides test stablecoins:

*   **pathUSD** - `0x20c0000000000000000000000000000000000000`
*   **AlphaUSD** - `0x20c0000000000000000000000000000000000001`
*   **BetaUSD** - `0x20c0000000000000000000000000000000000002`
*   **ThetaUSD** - `0x20c0000000000000000000000000000000000003`

Each request drips a sufficient amount for testing and development.

Verify Your Balance[](https://docs.tempo.xyz/guide/use-accounts/add-funds#verify-your-balance)
----------------------------------------------------------------------------------------------

After requesting tokens, verify your balance:

```
# Check AlphaUSD balance
cast erc20 balance 0x20c0000000000000000000000000000000000001 \
  <YOUR_ADDRESS> \
  --rpc-url https://rpc.moderato.tempo.xyz
```
