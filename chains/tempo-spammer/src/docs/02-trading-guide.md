# Executing Swaps ⋅ Tempo
Execute swaps between stablecoins on the exchange. Swaps execute immediately against existing orders in the orderbook, providing instant liquidity for cross-stablecoin payments.

By the end of this guide you will be able to execute swaps, get price quotes, and manage slippage protection.

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

Steps[](https://docs.tempo.xyz/guide/stablecoin-dex/executing-swaps#steps)
--------------------------------------------------------------------------

### Set up your client[](https://docs.tempo.xyz/guide/stablecoin-dex/executing-swaps#set-up-your-client)

Ensure that you have set up your client by following the [guide](https://docs.tempo.xyz/sdk/typescript).

### Get a price quote[](https://docs.tempo.xyz/guide/stablecoin-dex/executing-swaps#get-a-price-quote)

Before executing a swap, get a quote to see the expected price.

```
import {  } from 'wagmi/tempo'
import { ,  } from 'viem'
 
const  = '0x20c0000000000000000000000000000000000001'
const  = '0x20c0000000000000000000000000000000000002'
 
function () {
  const  = ('10', 6)
 
  // How much AlphaUSD do I need to spend to receive 10 BetaUSD?
  const { :  } = ..({ 
    : , 
    : , 
    : , 
  }) 
 
  return <>Quote: {(, 6)}</>
}
```


### Calculate slippage tolerance[](https://docs.tempo.xyz/guide/stablecoin-dex/executing-swaps#calculate-slippage-tolerance)

Set appropriate slippage based on your quote to protect against unfavorable price movements.

```
import {  } from 'wagmi/tempo'
import { ,  } from 'viem'
 
const  = '0x20c0000000000000000000000000000000000001'
const  = '0x20c0000000000000000000000000000000000002'
 
function () {
  const  = ('10', 6)
 
  // How much AlphaUSD do I need to spend to receive 10 BetaUSD?
  const { :  } = ..({
    : ,
    : ,
    : ,
  })
 
  // Calculate 0.5% slippage tolerance
  const  = 0.005
  const  =  
    ?  * (.((1 + ) * 1000)) / 1000n
    : 0n
 
  return (
    <>
      <>Quote: {(, 6)}</>
      <>Max input (0.5% slippage): {(, 6)}</> // [!code ++]
    </>
  )
}
```


### Approve Spend[](https://docs.tempo.xyz/guide/stablecoin-dex/executing-swaps#approve-spend)

To execute a swap, you need to approve the Stablecoin DEX contract to spend the token you're using to fund the swap.

```
import { ,  } from 'viem/tempo'
import {  } from 'wagmi/tempo'
import { ,  } from 'viem'
import {  } from 'wagmi'
 
const  = '0x20c0000000000000000000000000000000000001'
const  = '0x20c0000000000000000000000000000000000002'
 
function () {
  const  = ('10', 6)
 
  // How much AlphaUSD do I need to spend to receive 10 BetaUSD?
  const { :  } = ..({
    : ,
    : ,
    : ,
  })
 
  // Calculate 0.5% slippage tolerance
  const  = 0.005
  const  = 
    ?  * (.((1 + ) * 1000)) / 1000n
    : 0n
 
  const  = () 
 
  return (
    <>
      <>Quote: {(, 6)}</>
      <>Max input (0.5% slippage): {(, 6)}</>
      < ="button" ={() => { 
        const  = [ 
          ...({ 
            : , // Approve the max amount with slippage
            : ., 
            : , 
          }), 
        ] 
        .({  }) 
      }}> // [!code ++]
        Approve Spend // [!code ++]
      </> // [!code ++]
    </>
  )
}
```


### Execute a swap[](https://docs.tempo.xyz/guide/stablecoin-dex/executing-swaps#execute-a-swap)

Batch the token approval with the swap in a single transaction for better UX.

```
import { ,  } from 'viem/tempo'
import {  } from 'wagmi/tempo'
import { ,  } from 'viem'
import {  } from 'wagmi'
 
const  = '0x20c0000000000000000000000000000000000001'
const  = '0x20c0000000000000000000000000000000000002'
 
function () {
  const  = ('10', 6)
 
  // How much AlphaUSD do I need to spend to receive 10 BetaUSD?
  const { :  } = ..({
    : ,
    : ,
    : ,
  })
 
  // Calculate 0.5% slippage tolerance
  const  = 0.005
  const  = 
    ?  * (.((1 + ) * 1000)) / 1000n
    : 0n
 
  const  = ()
 
  return (
    <>
      <>Quote: {(, 6)}</>
      <>Max input (0.5% slippage): {(, 6)}</>
      < ="button" ={() => {
        const  = [
          ...({
            : , // Approve the max amount with slippage
            : .,
            : ,
          }),
          ...({ 
            : , 
            , 
            : , 
            : , 
          }), 
        ]
        .({  })
      }}>
        Execute Swap
      </>
    </>
  )
}
```


Recipes[](https://docs.tempo.xyz/guide/stablecoin-dex/executing-swaps#recipes)
------------------------------------------------------------------------------

### Handling insufficient liquidity[](https://docs.tempo.xyz/guide/stablecoin-dex/executing-swaps#handling-insufficient-liquidity)

Quote requests will fail with an `InsufficientLiquidity` error if there isn't enough liquidity in the orderbook to satisfy the requested amount.

Handle this error when fetching quotes:

```
import { Hooks } from 'wagmi/tempo'
import { parseUnits } from 'viem'
 
const alphaUsd = '0x20c0000000000000000000000000000000000001'
const betaUsd = '0x20c0000000000000000000000000000000000002'
 
function Swap() {
  const amount = parseUnits('10', 6)
 
  const { data: quote, error } = Hooks.dex.useSellQuote({
    tokenIn: alphaUsd,
    tokenOut: betaUsd,
    amountIn: amount,
  })
 
  if (error) {
    if (error.message.includes('InsufficientLiquidity')) {
      return <div>Not enough liquidity available. Try a smaller amount.</div>
    }
    return <div>Error: {error.message}</div>
  }
 
  if (!quote) {
    return <div>Loading quote...</div>
  }
 
  return <div>Quote: {quote.toString()}</div>
}
```


Best Practices[](https://docs.tempo.xyz/guide/stablecoin-dex/executing-swaps#best-practices)
--------------------------------------------------------------------------------------------

### Always get quotes before swapping[](https://docs.tempo.xyz/guide/stablecoin-dex/executing-swaps#always-get-quotes-before-swapping)

Query the expected price before executing a swap to ensure you're getting a fair rate and to set appropriate slippage protection.

### Set appropriate slippage protection[](https://docs.tempo.xyz/guide/stablecoin-dex/executing-swaps#set-appropriate-slippage-protection)

Use `minAmountOut` or `maxAmountIn` to protect against unfavorable price movements between quoting and execution.

Learning Resources[](https://docs.tempo.xyz/guide/stablecoin-dex/executing-swaps#learning-resources)
----------------------------------------------------------------------------------------------------
# View the Orderbook ⋅ Tempo
Query and inspect the orderbook to see available liquidity, price levels, and individual orders on Tempo's Stablecoin DEX.

Recommended Approach[](https://docs.tempo.xyz/guide/stablecoin-dex/view-the-orderbook#recommended-approach)
-----------------------------------------------------------------------------------------------------------

We recommend using indexed data to query the orderbook for better performance and ease of use. While you can query logs and transactions directly from an RPC node, indexed data providers offer structured SQL interfaces that make complex queries simpler and more efficient.

In this guide, we use [Index Supply](https://www.indexsupply.net/) as our indexing provider, but you're free to choose your own indexing solution or query the chain directly based on your needs.

Recipes[](https://docs.tempo.xyz/guide/stablecoin-dex/view-the-orderbook#recipes)
---------------------------------------------------------------------------------

### Get the current spread[](https://docs.tempo.xyz/guide/stablecoin-dex/view-the-orderbook#get-the-current-spread)

Query the best bid and ask prices to calculate the current spread for a token pair.

Find the highest bid prices (buyers) for AlphaUSD. This query filters out fully filled and cancelled orders, groups by price level (tick), and shows the top 5 bid prices with their total liquidity.

#### Best Bid Prices for AlphaUSD

OrderPlaced

OrderFilled

OrderCancelled

Find the lowest ask prices (sellers) for AlphaUSD. The spread is the difference between the highest bid and lowest ask price.

#### Best Ask Prices for AlphaUSD

OrderPlaced

OrderFilled

OrderCancelled

### Inspect order depth[](https://docs.tempo.xyz/guide/stablecoin-dex/view-the-orderbook#inspect-order-depth)

View aggregated liquidity at each price level to understand the orderbook structure.

This query shows all active orders for BetaUSD, including both regular and flip orders.

#### BetaUSD Order Depth by Price Level

OrderPlaced

OrderFilled

OrderCancelled

### Inspect an individual order[](https://docs.tempo.xyz/guide/stablecoin-dex/view-the-orderbook#inspect-an-individual-order)

#### Order details[](https://docs.tempo.xyz/guide/stablecoin-dex/view-the-orderbook#order-details)

Get detailed information about a specific order including its placement details, fill history, and cancellation status.

This query inspects the details of the most recent order for AlphaUSD. It shows when the order was created, at what price (tick), the order size, whether it's a flip order, and who placed it.

#### Order Placement Details

#### Order fill status[](https://docs.tempo.xyz/guide/stablecoin-dex/view-the-orderbook#order-fill-status)

Check if an order has been partially or fully filled. This query shows up to 5 fill events for order number `2`, including the amount filled in each transaction and whether it was a partial fill.

#### Order Fill History

#### Cancelled orders[](https://docs.tempo.xyz/guide/stablecoin-dex/view-the-orderbook#cancelled-orders)

Check if an order has been cancelled. This query returns an order for AlphaUSD that was explicitly cancelled by the maker before being fully filled.

#### Order Cancellation Status

OrderCancelled

OrderPlaced

### Get recent trade prices[](https://docs.tempo.xyz/guide/stablecoin-dex/view-the-orderbook#get-recent-trade-prices)

View the last prices a token traded at to understand recent market activity.

This query joins order fill events with their corresponding placement details to show the price tick and amount for recent trades.

#### Recent Trade Prices for AlphaUSD
# Providing Liquidity ⋅ Tempo
Add liquidity for a token pair by placing orders on the Stablecoin DEX. You can provide liquidity on the `buy` or `sell` side of the orderbook, with `limit` or `flip` orders. To learn more about order types see the [documentation on order types](https://docs.tempo.xyz/protocol/exchange/providing-liquidity#order-types).

In this guide you will learn how to place buy and sell orders to provide liquidity on the Stablecoin DEX orderbook.

Demo[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#demo)
----------------------------------------------------------------------------

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Approve spend and place buy order for 100 AlphaUSD

Steps[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#steps)
------------------------------------------------------------------------------

### Set up your client[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#set-up-your-client)

Ensure that you have set up your client by following the [guide](https://docs.tempo.xyz/sdk/typescript).

### Approve spend[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#approve-spend)

To place an order, you need to approve the Stablecoin DEX contract to spend the order's "spend" token.

1

Create an account, or use an existing one.

2

Approve the Stablecoin DEX to spend pathUSD

```
import {  } from 'viem'
import {  } from 'viem/tempo'
import {  } from 'wagmi/tempo'
 
const  = '0x20c0000000000000000000000000000000000000'
const  = '0x20c0000000000000000000000000000000000001'
 
function (: { : 'buy' | 'sell' }) {
  const {  } = 
  // buying AlphaUSD requires we spend pathUSD
  const  =  === 'buy' ?  :  
  const { :  } = ..() 
 
  return (
    < ="button" ={() => {
      ({ 
        : ('100', 6), 
        : ., 
        : , 
      }) 
  }}>
      Approve Spend
    </>
  )
}
```


### Place order[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#place-order)

Once the spend is approved, you can place an order by calling the `place` action on the Stablecoin DEX.

1

Create an account, or use an existing one.

2

Approve spend and place buy order for 100 AlphaUSD

```
import { ,  } from 'viem/tempo'
import {  } from 'viem'
import {  } from 'wagmi'
 
const  = '0x20c0000000000000000000000000000000000000'
const  = '0x20c0000000000000000000000000000000000001'
 
function (: { : 'buy' | 'sell' }) {
  const {  } = 
  // buying AlphaUSD requires we spend pathUSD
  const  =  === 'buy' ?  : 
 
  const  = () 
 
  return (
    < ="button" ={() => {
      const  = [ 
        ...({ 
          : ., 
          : ('100', 6), 
          : , 
        }), 
        ...({ 
          : , 
          : ('100', 6), 
          : , 
          : 0, 
        }), 
      ] 
      .({  }) 
    }}>
      Place Order
    </>
  )
}
```


### View order details[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#view-order-details)

After placing an order, you can query its details to see the current state, including the amount filled and remaining using `[Hooks.dex.useOrder](https://wagmi.sh/tempo/hooks/dex.useOrder)`.

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Approve spend and place buy order for 100 AlphaUSD

```
import {  } from 'wagmi/tempo'
 
const  = 123n
 
const { : ,  } = ..({
  ,
})
 
.('Type:', ?. ? 'Buy' : 'Sell')
.('Amount:', ?..())
.('Remaining:', ?..())
.('Tick:', ?.)
.('Is flip order:', ?.)
```


For more details on querying orders, see the `[Hooks.dex.useOrder](https://wagmi.sh/tempo/hooks/dex.useOrder)` documentation.

Recipes[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#recipes)
----------------------------------------------------------------------------------

### Cancel order[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#cancel-order)

Cancel an order using its order ID.

When you cancel an order, any remaining funds are credited to your exchange balance (not directly to your wallet). To move funds back to your wallet, you can [withdraw them to your wallet](https://docs.tempo.xyz/protocol/exchange/exchange-balance#withdrawing-funds).

#### Place and Cancel an Order

demo

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Approve spend and place buy order for 100 AlphaUSD

```
import { ,  } from 'viem/tempo'
import {  } from 'wagmi/tempo'
import {  } from 'viem'
import {  } from 'wagmi'
 
const  = '0x20c0000000000000000000000000000000000000'
const  = '0x20c0000000000000000000000000000000000001'
 
function () {
  const  = ()
  const  = ..() 
 
  const  = () => {
    const  = [
      ...({
        : .,
        : ('100', 6),
        : ,
      }),
      ...({
        : ,
        : ('100', 6),
        : 'buy',
        : 0,
      }),
    ]
    .({  })
  }
 
  return (
    <>
      < ="button" ={}>
        Place Order
      </>
      < ={
        () => {
          .()
          const  = new (. as HTMLFormElement)
          const  = (.('orderId') as string)
 
          .({  }) 
        }
      }>
        < ="text" ="orderId" ="Order ID" />
        <
          ="submit"
          ={.}
        > 
          {. ? 'Canceling...' : 'Cancel Order'}
        </> 
      </>
    </>
  )
}
```


### Determining quote token[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#determining-quote-token)

Each token has a designated quote token that it trades against on the DEX. For most stablecoins, this will be `pathUSD`.

Use the `token.useGetMetadata` hook to retrieve a token's quote token.

```
import {  } from 'wagmi/tempo'
 
const { :  } = ..({ 
  : '0x20c0000000000000000000000000000000000001', // AlphaUSD
}) 
 
.('Token:', ?.)

Token: AlphaUSD.('Quote Token:', ?.) // returns `pathUSD` address 
Quote Token: 0x20c0000000000000000000000000000000000000
```


### Flip order[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#flip-order)

Flip orders automatically switch between buy and sell sides when filled, providing continuous liquidity. Use viem's `[dex.placeFlip](https://viem.sh/tempo/actions/dex.placeFlip)` to create a flip order call.

```
import { ,  } from 'viem/tempo'
import {  } from 'viem'
import {  } from 'wagmi'
 
const  = '0x20c0000000000000000000000000000000000000'
const  = '0x20c0000000000000000000000000000000000001'
 
function (: { : 'buy' | 'sell' }) {
  const {  } = 
  // buying AlphaUSD requires we spend pathUSD
  const  =  === 'buy' ?  : 
 
  const  = ()
 
  return (
    < ="button" ={() => {
      const  = [
        ...({
          : .,
          : ('100', 6),
          : ,
        }),
        ...({ 
          : ,
          : ('100', 6),
          : ,
          : 0,
        }),
      ]
      .({  })
    }}>
      Place Flip Order
    </>
  )
}
```


### Place order at specific price[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#place-order-at-specific-price)

Ticks represent prices relative to the quote token (usually pathUSD). The formula is:

```
tick = (price - 1) * 100_000

```


For example, price $1.0000 → tick = 0, price $0.9990 → tick = -10, and price $1.0010 → tick = 10.

Use the `Tick` utility to convert between prices and ticks:

```
import { Actions, Tick } from 'viem/tempo'
import { parseUnits } from 'viem'
 
const alphaUsd = '0x20c0000000000000000000000000000000000001'
 
// buy order at $0.9990 (tick: -10)
const buyCall = Actions.dex.place.call({ 
  token: alphaUsd, 
  amount: parseUnits('100', 6), 
  type: 'buy', 
  tick: Tick.fromPrice('0.9990'), // -10
}) 
 
// sell order at $1.0010 (tick: 10)
const sellCall = Actions.dex.place.call({ 
  token: alphaUsd, 
  amount: parseUnits('100', 6), 
  type: 'sell', 
  tick: Tick.fromPrice('1.0010'), // 10
}) 
```


For more details including tick precision, limits, and calculation examples, see [Understanding Ticks](https://docs.tempo.xyz/protocol/exchange/providing-liquidity#understanding-ticks).

Best practices[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#best-practices)
------------------------------------------------------------------------------------------------

### Batch calls[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#batch-calls)

You can batch the calls to approve spend and place the order in a single transaction for efficiency. See the [guide on batch transactions](https://docs.tempo.xyz/guide/use-accounts/batch-transactions) for more details.

Learning resources[](https://docs.tempo.xyz/guide/stablecoin-dex/providing-liquidity#learning-resources)
--------------------------------------------------------------------------------------------------------
