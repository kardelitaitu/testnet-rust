# Wallet Integration Guide ⋅ Tempo
Because there is [no native token on Tempo](https://docs.tempo.xyz/quickstart/evm-compatibility#handling-eth-native-token-balance-checks) and transaction fees are paid directly in stablecoins, wallets need specific UI and logic adjustments to support the network. Follow this guide if your wallet logic and/or interfaces are dependent on the existence of a native token.

As part of supporting Tempo in your wallet, you can also deliver an enhanced experience for your users by integrating [Tempo Transactions](https://docs.tempo.xyz/guide/tempo-transaction). Common use cases include enabling a gasless transactions for your users, letting your users decide what token to use for fees, and more.

Steps[](https://docs.tempo.xyz/quickstart/wallet-developers#steps)
------------------------------------------------------------------

### Handle the absence of a native token[](https://docs.tempo.xyz/quickstart/wallet-developers#handle-the-absence-of-a-native-token)

If you use `eth_getBalance` to validate a user's balance, you should instead check the user's account fee token balance on Tempo. Additionally, you should not display any "native balance" in your UI for Tempo users.

```
// @filename: viem.config.ts
import { createClient, http, publicActions, walletActions } from 'viem'
import { privateKeyToAccount } from 'viem/accounts'
import { tempoModerato } from 'viem/chains'
import { tempoActions } from 'viem/tempo'
 
export const client = createClient({
  account: privateKeyToAccount('0x...'),
  chain: tempoModerato,
  transport: http(),
})
  .extend(publicActions)
  .extend(walletActions)
  .extend(tempoActions())
// @filename: example.ts
// ---cut---
import { client } from './viem.config'
 
const userFeeToken = await client.fee.getUserToken({ 
  account: '0x...' 
})
 
const balance = await client.token.getBalance({ 
  account: '0x...',
  token: userFeeToken.address 
})
```


### Configure native currency symbol[](https://docs.tempo.xyz/quickstart/wallet-developers#configure-native-currency-symbol)

If you need to display a native token symbol, such as showing how much gas a transaction requires, you can set the currency symbol to `USD` for Tempo as fees are denominated in USD.

### Use fee token preferences to quote gas prices[](https://docs.tempo.xyz/quickstart/wallet-developers#use-fee-token-preferences-to-quote-gas-prices)

On Tempo, users can pay fees in any supported stablecoin. You should quote gas/fee prices in your UI based on a transaction's fee token.

### Display token and network assets[](https://docs.tempo.xyz/quickstart/wallet-developers#display-token-and-network-assets)

### Integrate Tempo Transactions[](https://docs.tempo.xyz/quickstart/wallet-developers#integrate-tempo-transactions)

Recipes[](https://docs.tempo.xyz/quickstart/wallet-developers#recipes)
----------------------------------------------------------------------

### Get user's fee token[](https://docs.tempo.xyz/quickstart/wallet-developers#get-users-fee-token)

Retrieve the user's configured fee token preference:

```
import { getUserToken } from 'viem/tempo'
 
const feeToken = await client.fee.getUserToken({ 
  account: userAddress 
})
```


See `[getUserToken](https://viem.sh/tempo/actions/fee.getUserToken)` for full documentation.

### Get token balance[](https://docs.tempo.xyz/quickstart/wallet-developers#get-token-balance)

Check a user's balance for a specific token:

```
import { getBalance } from 'viem/tempo'
 
const balance = await client.token.getBalance({ 
  account: userAddress,
  token: tokenAddress 
})
```


See `[getBalance](https://viem.sh/tempo/actions/token.getBalance)` for full documentation.

### Set user fee token[](https://docs.tempo.xyz/quickstart/wallet-developers#set-user-fee-token)

Set the user's default fee token preference. This will be used for all transactions unless a different fee token is specified at the transaction level.

```
import { setUserToken } from 'viem/tempo'
 
await client.fee.setUserTokenSync({ 
  token: '0x20c0000000000000000000000000000000000001', 
})
```


See `[setUserToken](https://viem.sh/tempo/actions/fee.setUserToken)` for full documentation.

Checklist[](https://docs.tempo.xyz/quickstart/wallet-developers#checklist)
--------------------------------------------------------------------------

Before launching Tempo support, ensure your wallet:

*   Checks fee token balance instead of native balance
*   Hides or removes native balance display for Tempo
*   Displays `USD` as the currency symbol for gas
*   Quotes gas prices in the user's fee token
*   Pulls token/network assets from Tempo's tokenlist
*   (Recommended) Integrates Tempo Transactions for enhanced UX

Learning Resources[](https://docs.tempo.xyz/quickstart/wallet-developers#learning-resources)
--------------------------------------------------------------------------------------------
# Connect to Wallets ⋅ Tempo
It is possible to use Tempo with EVM-compatible wallets that support the Tempo network, or support adding custom networks (like MetaMask).

You can use these wallets when building your application on Tempo.

This guide will walk you through how to set up your application to connect to wallets.

1

Connect your browser wallet.

No browser wallets found.

Wagmi Setup[](https://docs.tempo.xyz/guide/use-accounts/connect-to-wallets#wagmi-setup)
---------------------------------------------------------------------------------------

### Set up Wagmi[](https://docs.tempo.xyz/guide/use-accounts/connect-to-wallets#set-up-wagmi)

Ensure that you have set up your project with Wagmi by following the [guide](https://docs.tempo.xyz/sdk/typescript#wagmi-setup).

### Configure Wagmi[](https://docs.tempo.xyz/guide/use-accounts/connect-to-wallets#configure-wagmi)

Next, let's ensure Wagmi is configured correctly to connect to wallets.

Ensure we have `[multiInjectedProviderDiscovery](https://wagmi.sh/react/api/createConfig#multiinjectedproviderdiscovery)` set to `true` to display injected browser wallets.

We can also utilize [wallet connectors](https://wagmi.sh/react/api/connectors) from Wagmi like `metaMask` to support mobile devices.

config.ts

```
import { ,  } from 'wagmi'
import {  } from 'viem/chains'
import {  } from 'wagmi/connectors'
 
export const  = ({
  : [],
  : [()], 
  : true, 
  : {
    [tempo.id]: (),
  },
})
```


### Display Connect Buttons[](https://docs.tempo.xyz/guide/use-accounts/connect-to-wallets#display-connect-buttons)

After that, we will set up "Connect" buttons so that the user can connect to their wallet.

We will create a new `ConnectWallet.tsx` component to work in.

No browser wallets found.

```
import { ,  } from 'wagmi'
 
export function () {
  const  = ()
  const  = ()
 
  return (
    <>
      {.(() => (
        <
          ={.}
          ={() => .({  })}
          ="button"
        >
          {.}
        </>
      ))}
    </>
  )
}
```


### Display Account & Sign Out[](https://docs.tempo.xyz/guide/use-accounts/connect-to-wallets#display-account--sign-out)

After the user has connected to their wallet, we can display the account information and a sign out button.

We will create a new `Account.tsx` component to work in.

No browser wallets found.

```
import { ,  } from 'wagmi'
 
export function () {
  const  = ()
  const  = ()
 
  return (
    <>
      <>
        {.?.(0, 6)}...{.?.(-4)}
      </>
      < ={() => .()}>
        Sign out
      </>
    </>
  )
}
```


### Display "Add Tempo" Button[](https://docs.tempo.xyz/guide/use-accounts/connect-to-wallets#display-add-tempo-button)

If the wallet is not on the Tempo network, we can display a "Add Tempo" button so that the user can add the network to their wallet.

No browser wallets found.

```
import { , ,  } from 'wagmi'
import {  } from 'viem/chains'
 
export function () {
  const  = ()
  const  = ()
  const  = () 
 
  return (
    <>
      <>
        {.?.(0, 6)}...{.?.(-4)}
      </>
      < ={() => .()}>
        Sign out
      </>
 
      <
        ={() =>
          .({ 
            : tempo.id, 
            : { 
              : { 
                : 'USD', 
                : 18, 
                : 'USD', 
              }, 
            }, 
          }) 
        }
      > {/* // [!code ++] */}
        Add {chain.name} to wallet {/* // [!code ++] */}
      </> {/* // [!code ++] */}
    </>
  )
}
```


Third-Party Libraries[](https://docs.tempo.xyz/guide/use-accounts/connect-to-wallets#third-party-libraries)
-----------------------------------------------------------------------------------------------------------

You can also use a third-party Wallet Connection library to handle the onboarding & connection of wallets.

Such libraries include: [Privy](https://privy.io/), [ConnectKit](https://docs.family.co/connectkit), [AppKit](https://reown.com/appkit), [Dynamic](https://dynamic.xyz/), and [RainbowKit](https://rainbowkit.com/).

The above libraries are all built on top of Wagmi, handle all the edge cases around wallet connection.

Add to Wallet Manually[](https://docs.tempo.xyz/guide/use-accounts/connect-to-wallets#add-to-wallet-manually)
-------------------------------------------------------------------------------------------------------------

You can add Tempo testnet to a wallet that supports custom networks (e.g. MetaMask) manually.

For example, if you are using MetaMask:

1.  Open MetaMask and click on the menu in the top right and select "Networks"
2.  Click "Add a custom network"
3.  Enter the network details:


|Name          |Tempo Testnet (Moderato)      |
|--------------|------------------------------|
|Currency      |USD                           |
|Chain ID      |42431                         |
|HTTP URL      |https://rpc.moderato.tempo.xyz|
|WebSocket URL |wss://rpc.moderato.tempo.xyz  |
|Block Explorer|https://explore.tempo.xyz     |


The official documentation from MetaMask on this process is also available [here](https://support.metamask.io/configure/networks/how-to-add-a-custom-network-rpc#adding-a-network-manually).
