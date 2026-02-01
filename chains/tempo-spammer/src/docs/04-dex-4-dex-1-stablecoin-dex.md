# Exchanging Stablecoins ⋅ Tempo
Tempo features an enshrined decentralized exchange (DEX) designed specifically for trading between stablecoins of the same underlying asset (e.g., USDC to USDT). The exchange provides optimal pricing for cross-stablecoin payments while minimizing chain load from excessive market activity.

The exchange operates as a singleton precompiled contract at address `0xdec0000000000000000000000000000000000000`. It maintains an orderbook with separate queues for each price tick, using price-time priority for order matching.

Trading pairs are determined by each token's quote token. All TIP-20 tokens specify a quote token for trading on the exchange. Tokens can choose [pathUSD](https://docs.tempo.xyz/protocol/exchange/pathUSD) as their quote token. See the [Stablecoin DEX Specification](https://docs.tempo.xyz/protocol/exchange/spec) for detailed information on the exchange structure.

The exchange supports three types of orders, each with different execution behavior:



* Order Type: Limit Orders
  * Description: Place orders at specific price levels that wait in the book until matched or cancelled. Orders are added to the book immediately when placed.
* Order Type: Flip Orders
  * Description: Special orders that automatically reverse to the opposite side when completely filled, acting like a perpetual liquidity pool. When a flip order is fully filled, a new order is immediately created on the opposite side.
* Order Type: Market Orders
  * Description: Execute immediately against the best available orders in the book (via swap functions). Swaps and cancellations execute immediately within the transaction.


For the complete execution mechanics, see the [Stablecoin DEX Specification](https://docs.tempo.xyz/protocol/exchange/spec).

To get started with the exchange, explore these guides:

# Stablecoin DEX ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/exchange/spec#abstract)
------------------------------------------------------------------

This specification defines an enshrined decentralized exchange for trading between TIP-20 stablecoins. The exchange currently only supports trading between TIP-20 stablecoins with USD as their currency. By only allowing each stablecoin to be paired against its designated "quote token" the exchange enforces that there is only one route for trading between any two tokens.

The exchange maintains price‑time priority at each discrete price tick, executes swaps immediately against the active book, and supports auto‑replenishing “flip orders” that recreate themselves on the opposite side after being fully filled.

Users maintain internal balances per token on the exchange. Order placement escrows funds from these balances (or transfers from the user if necessary), fills credit makers internally, and withdrawals transfer tokens out.

Motivation[](https://docs.tempo.xyz/protocol/exchange/spec#motivation)
----------------------------------------------------------------------

Tempo aims to provide high‑quality execution for cross‑stablecoin payments while avoiding unnecessary chain load and minimizing mid‑block MEV surfaces.

A simple, on‑chain, price‑time‑priority orderbook tailored to stable pairs encourages passive liquidity to rest on chain and allows takers to execute deterministically at the best available ticks.

Another design goal is to avoid fragmentation of liquidity across many different pairs. By enforcing that each stablecoin only trades against a single quote token, the system guarantees that there is only one path between any two tokens.

Specification[](https://docs.tempo.xyz/protocol/exchange/spec#specification)
----------------------------------------------------------------------------

### Contract and scope[](https://docs.tempo.xyz/protocol/exchange/spec#contract-and-scope)

The exchange is a singleton contract deployed at `0xdec0000000000000000000000000000000000000`. It exposes functions to create trading pairs, place and cancel orders (including flip orders), execute swaps, produce quotes, and manage internal balances.

### Key concepts[](https://docs.tempo.xyz/protocol/exchange/spec#key-concepts)

#### Internal balances[](https://docs.tempo.xyz/protocol/exchange/spec#internal-balances)

The contract maintains per‑user, per‑token internal balances. Order placement escrows funds from these balances (or transfers any shortfall from the user). When an order fills, the maker is credited internally with the counter‑asset at the order’s tick price. Users can withdraw available balances at any time.

#### Flip orders[](https://docs.tempo.xyz/protocol/exchange/spec#flip-orders)

A flip order behaves like a normal resting order until it is fully filled. When filled, the exchange places a new order for the same maker on the opposite side at a configured `flipTick` (which must be greater than `tick` for bids and less for asks). This enables passive liquidity with flexible strategies.

When a flip order flips, it draws escrow exclusively from the maker's internal exchange balance. Unlike initial order placement, the exchange does not fall back to `transferFrom` if the internal balance is insufficient—the flip simply does not occur. This ensures that flip execution is self-contained and does not require additional token approvals or external balance checks at fill time.

#### Pairs, ticks, and prices[](https://docs.tempo.xyz/protocol/exchange/spec#pairs-ticks-and-prices)

Pairs are identified deterministically from the two token addresses (the base token is any TIP‑20, and its `quoteToken()` function points to the quote token). Prices are discretized into integer ticks with a tick size of 0.1 bps: with `PRICE_SCALE = 100_000`, `price = PRICE_SCALE + tick`. Orders may only be placed at ticks divisible by `TICK_SPACING = 10` (effectively setting a 1 bp tick size). The orderbook tracks best bid (highest active bid tick) and best ask (lowest active ask tick), and uses bitmaps over tick words for efficient discovery of the next initialized tick.

#### Quote tokens[](https://docs.tempo.xyz/protocol/exchange/spec#quote-tokens)

Each TIP‑20 token specifies a single quote token in its metadata via `quoteToken()`. A trading pair on the Stablecoin DEX exists only between a base token and its designated quote token, and prices for the pair are denominated in units of the quote token.

This design reduces liquidity fragmentation by giving every token exactly one paired asset.

It also simplifies routing. We require that:

1.  each token picks a single other stablecoin as its quote token, and,
2.  quote token relationships cannot have circular dependencies.

This forces liquidity into a tree structure, which in turn implies that there is only one path between any two stablecoins

USD tokens can only choose USD tokens as their quote token. Non-USD TIP-20 tokens can pick any token as their quote token, but currently there is no support for cross-currency trading, or same-currency trading of non-USD tokens, on the DEX.

The platform offers a neutral USD stablecoin, `[pathUSD](https://docs.tempo.xyz/protocol/exchange/pathUSD)`, as an option for quote token. PathUSD is the first stablecoin deployed on the chain, which means it has no quote token. Use of pathUSD is optional.

#### Swaps[](https://docs.tempo.xyz/protocol/exchange/spec#swaps)

Swaps execute immediately against the active book. Selling base for quote starts at the current best bid and walks downward as ticks are exhausted; selling quote for base starts at the best ask and walks upward. Within a tick, fills are FIFO and decrement the tick’s total liquidity. When a tick empties, it is de‑initialized.

Callers can swap between any two USD TIP-20 tokens. If `tokenIn` and `tokenOut` are not directly paired, the implementation finds the unique path between them through quote‑token relationships, and performs a multi‑hop swap/quote.

#### Crossed books[](https://docs.tempo.xyz/protocol/exchange/spec#crossed-books)

Crossed books are permitted; the implementation does not enforce that best bid ≤ best ask. This primarily supports flip‑order scenarios.

#### Constraints[](https://docs.tempo.xyz/protocol/exchange/spec#constraints)

*   Only USD‑denominated tokens are supported, and their quotes must also be USD
*   Orders must specify ticks within the configured bounds (±2000)
*   Tick spacing is 10: `tick % 10 == 0` for orders and flip orders
*   Withdrawals require sufficient internal balance

### Interface[](https://docs.tempo.xyz/protocol/exchange/spec#interface)

Below is the complete on‑chain interface, organized by function. Behavior notes and constraints are included with each function where relevant.

#### Constants and pricing[](https://docs.tempo.xyz/protocol/exchange/spec#constants-and-pricing)

```
function PRICE_SCALE() external view returns (uint32);
```


Scaling factor for tick‑based prices. One tick equals 1/PRICE\_SCALE above or below the peg. Current value: `100_000` (0.001% per tick).

```
function TICK_SPACING() external view returns (int16);
```


Orders must be placed on ticks divisible by `TICK_SPACING`. Current value: `10` (i.e., 1 bp grid).

```
function MIN_TICK() external view returns (int16);
function MAX_TICK() external view returns (int16);
```


Inclusive tick bounds for order placement. Current range: ±2000 ticks (±2%).

```
function MIN_PRICE() external view returns (uint32);
function MAX_PRICE() external view returns (uint32);
```


Price bounds implied by tick bounds and `PRICE_SCALE`.

```
function tickToPrice(int16 tick) external pure returns (uint32 price);
function priceToTick(uint32 price) external pure returns (int16 tick);
```


Convert between discrete ticks and scaled prices. `priceToTick` reverts if `price` is out of bounds.

#### Pairing and orderbook[](https://docs.tempo.xyz/protocol/exchange/spec#pairing-and-orderbook)

```
function pairKey(address tokenA, address tokenB) external pure returns (bytes32 key);
```


Deterministic key for a pair derived from the two token addresses (order‑independent).

```
function createPair(address base) external returns (bytes32 key);
```


Creates the pair between `base` and its `quoteToken()` (from TIP‑20). Both must be USD‑denominated. Reverts if the pair already exists or tokens are not USD.

```
function books(bytes32 pairKey) external view returns (address base, address quote, int16 bestBidTick, int16 bestAskTick);
```


Returns pair metadata and current best‑of‑side ticks. Best ticks may be sentinel values when no liquidity exists.

```
function getTickLevel(address base, int16 tick, bool isBid) external view returns (uint128 head, uint128 tail, uint128 totalLiquidity);
```


Returns FIFO head/tail order IDs and aggregate liquidity for a tick on a side, allowing indexers to reconstruct the active book.

#### Internal balances[](https://docs.tempo.xyz/protocol/exchange/spec#internal-balances-1)

```
function balanceOf(address user, address token) external view returns (uint128);
```


Returns a user’s internal balance for `token` held on the exchange.

```
function withdraw(address token, uint128 amount) external;
```


Transfers `amount` of `token` from the caller’s internal balance to the caller. Reverts if insufficient internal balance.

#### Order placement and lifecycle[](https://docs.tempo.xyz/protocol/exchange/spec#order-placement-and-lifecycle)

```
function place(address token, uint128 amount, bool isBid, int16 tick) external returns (uint128 orderId);
```


Places a limit order against the pair of `token` and its quote, immediately adding it to the active book. Escrows funds: bids escrow quote at tick price; asks escrow base.

Notes:

*   `tick` must be within `[MIN_TICK, MAX_TICK]` and divisible by `TICK_SPACING` (10).
*   The maker must be authorized by the TIP-403 transfer policies of both the base and quote tokens. This ensures makers cannot place orders to buy or sell tokens they are not permitted to transfer.
*   Additionally, the DEX contract itself must be authorized by the TIP-20 transfer policies of both the base and quote tokens. This allows token issuers to prevent their tokens from being traded on the DEX.

```
function placeFlip(address token, uint128 amount, bool isBid, int16 tick, int16 flipTick) external returns (uint128 orderId);
```


Like `place`, but marks the order as a flip order. When fully filled, a new order for the same maker is scheduled on the opposite side at `flipTick` (which must be greater than `tick` for bids and less for asks).

Notes:

*   Both `tick` and `flipTick` must be within `[MIN_TICK, MAX_TICK]` and divisible by `TICK_SPACING` (10).
*   When the order flips, escrow is drawn exclusively from the maker's internal exchange balance. If the internal balance is insufficient, the flip silently fails—no `transferFrom` is attempted, even if the maker has sufficient external balance and approval.
*   The maker must be authorized by the TIP-403 transfer policies of both the base and quote tokens, both at initial placement and when the order flips. If the maker becomes unauthorized before a flip, the flip silently fails and no new order is created (although the existing order is executed).

```
function cancel(uint128 orderId) external;
```


Cancels an order owned by the caller. When canceled, the order is removed from the tick queue, liquidity is decremented, and remaining escrow is refunded to the order owner's exchange balance which can then be withdrawn.

```
function cancelStaleOrder(uint128 orderId) external;
```


Cancels an order where the maker is forbidden by the escrowed token's [TIP-403 transfer policy](https://docs.tempo.xyz/protocol/tip403/overview). Unlike `cancel`, this function can be called by anyone—not just the order maker—but only succeeds if the maker is no longer authorized to transfer the escrowed token (e.g., the maker has been blacklisted). This allows third parties to clean up stale orders from the book.

When canceled, the order is removed from the tick queue, liquidity is decremented, and remaining escrow is refunded to the order maker's exchange balance. Reverts with `OrderNotStale` if the maker is still authorized.

```
function nextOrderId() external view returns (uint128);
```


Monotonic counter for next orderId.

#### Swaps and quoting[](https://docs.tempo.xyz/protocol/exchange/spec#swaps-and-quoting)

```
function quoteSwapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn) external view returns (uint128 amountOut);
```


Simulates an exact‑in swap walking initialized ticks and returns the expected output. Reverts if the pair path lacks sufficient liquidity.

```
function quoteSwapExactAmountOut(address tokenIn, address tokenOut, uint128 amountOut) external view returns (uint128 amountIn);
```


Simulates an exact‑out swap and returns the required input. Reverts if insufficient liquidity.

```
function swapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn, uint128 minAmountOut) external returns (uint128 amountOut);
```


Executes an exact‑in swap against the active book. Deducts `amountIn` from caller’s internal balance (transferring any shortfall) and transfers output to the caller. Reverts if resulting `amountOut` is below `minAmountOut` or liquidity is insufficient.

```
function swapExactAmountOut(address tokenIn, address tokenOut, uint128 amountOut, uint128 maxAmountIn) external returns (uint128 amountIn);
```


Executes an exact‑out swap. Deducts the actual input from the caller’s internal balance (transferring any shortfall from the user) and transfers `amountOut` to the caller. Reverts if required input exceeds `maxAmountIn` or liquidity is insufficient.

#### Events[](https://docs.tempo.xyz/protocol/exchange/spec#events)

```
event PairCreated(bytes32 indexed key, address indexed base, address indexed quote);
event OrderPlaced(uint128 indexed orderId, address indexed maker, address indexed token, uint128 amount, bool isBid, int16 tick, bool isFlipOrder, int16 flipTick);
event OrderCancelled(uint128 indexed orderId);
event OrderFilled(uint128 indexed orderId, address indexed maker, address indexed taker, uint128 amountFilled, bool partialFill);
```


#### Errors[](https://docs.tempo.xyz/protocol/exchange/spec#errors)

*   Pair creation or usage: `PAIR_EXISTS`, `PAIR_NOT_EXISTS`, `ONLY_USD_PAIRS`
*   Bounds: `TICK_OUT_OF_BOUNDS`, `FLIP_TICK_OUT_OF_BOUNDS`, `FLIP_TICK_MUST_BE_GREATER_FOR_BID`, `FLIP_TICK_MUST_BE_LESS_FOR_ASK`, "Price out of bounds"
*   Tick spacing: `TICK_NOT_MULTIPLE_OF_SPACING`, `FLIP_TICK_NOT_MULTIPLE_OF_SPACING`
*   Liquidity and limits: `INSUFFICIENT_LIQUIDITY`, `MAX_IN_EXCEEDED`, `INSUFFICIENT_OUTPUT`
*   Authorization: `UNAUTHORIZED` (cancel not by maker)
*   Stale orders: `ORDER_NOT_STALE` (cancelStaleOrder when maker is still authorized)
*   Balance: `INSUFFICIENT_BALANCE` (withdraw)

# pathUSD ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/exchange/pathUSD#abstract)
---------------------------------------------------------------------

pathUSD is a USD-denominated stablecoin that can be used as a quote token on Tempo's decentralized exchange. It is the first stablecoin deployed to the chain, and is used as a fallback gas token when the user or validator does not specify a gas token. Use of pathUSD is optional.

Motivation[](https://docs.tempo.xyz/protocol/exchange/pathUSD#motivation)
-------------------------------------------------------------------------

Each USD TIP-20 on Tempo can choose any other USD TIP-20 as its [quote token](https://docs.tempo.xyz/protocol/exchange/spec.mdx#quote-tokens)—the token it is paired against on the [native decentralized exchange](https://docs.tempo.xyz/protocol/exchange/spec). This guarantees that there is one path between any two tokens, which reduces fragmentation of liquidity and simplifies routing.

While on other chains, most liquidity accrues to a few stablecoins, or even one, Tempo offers a USD-denominated stablecoin, pathUSD, that other stablecoins can choose as their quote token. PathUSD is not meant to compete as a consumer-facing stablecoin. Use of pathUSD is optional, and tokens are able to list any other token as their quote token if they choose.

pathUSD can also be accepted as a fee token by validators.

Specification[](https://docs.tempo.xyz/protocol/exchange/pathUSD#specification)
-------------------------------------------------------------------------------

### Contract[](https://docs.tempo.xyz/protocol/exchange/pathUSD#contract)

pathUSD is a predeployed [TIP-20](https://docs.tempo.xyz/protocol/tip20/spec) at genesis. Note that since it is the first TIP-20 contract deployed, its quote token is the zero address.


|Property    |Value                                     |
|------------|------------------------------------------|
|address     |0x20c0000000000000000000000000000000000000|
|name()      |"pathUSD"                                 |
|symbol()    |"pathUSD"                                 |
|currency()  |"USD"                                     |
|decimals()  |6                                         |
|quoteToken()|address(0)                                |


How It Works[](https://docs.tempo.xyz/protocol/exchange/pathUSD#how-it-works)
-----------------------------------------------------------------------------

When you create a USD stablecoin on Tempo, you can set pathUSD as its quote token:

```
TIP20 token = factory.createToken(
  "My Company USD",
  "MCUSD",
  "USD",
  TIP20(0x20c0000000000000000000000000000000000000), // pathUSD
  msg.sender,
  bytes32("my-unique-salt") // salt for deterministic address
);
```


This means:

*   Your token trades against pathUSD on the decentralized exchange
*   Users can swap between your token and other USD stablecoins that also use pathUSD, or ones that are connected to it by a multi-hop path

### Tree Structure[](https://docs.tempo.xyz/protocol/exchange/pathUSD#tree-structure)

This creates a tree structure where all USD stablecoins are connected via multi-hop paths.

```
               USDX
                |
             pathUSD -- USDY -- USDZ
                |
               USDA

```


The tree structure guarantees that there is a single path between any two USD stablecoins, ensuring simple routing, concentrated liquidity, and efficient pricing, even for thinly-traded pairs.

### Example: Cross-Stablecoin Payment[](https://docs.tempo.xyz/protocol/exchange/pathUSD#example-cross-stablecoin-payment)

1.  Market makers provide liquidity for USDX/pathUSD and USDY/pathUSD pairs
2.  User wants to send USDX to a merchant who prefers USDY
3.  DEX atomically routes: User's USDX → pathUSD → Merchant's USDY
4.  Single action, no manual swaps

This is critical for payments between parties with different stablecoin preferences. The user and merchant never touch pathUSD; it is used only as a routing mechanism.

# Executing Swaps ⋅ Tempo
Swap Functions[](https://docs.tempo.xyz/protocol/exchange/executing-swaps#swap-functions)
-----------------------------------------------------------------------------------------

The exchange provides two primary swap functions:

### Swap Exact Amount In[](https://docs.tempo.xyz/protocol/exchange/executing-swaps#swap-exact-amount-in)

Specify the exact amount of tokens you want to sell, and receive at least a minimum amount:

```
function swapExactAmountIn(
    address tokenIn,
    address tokenOut,
    uint128 amountIn,
    uint128 minAmountOut
) external returns (uint128 amountOut)
```


**Parameters:**

*   `tokenIn` - The token address you're selling
*   `tokenOut` - The token address you're buying
*   `amountIn` - The exact amount of `tokenIn` to sell
*   `minAmountOut` - Minimum amount of `tokenOut` you'll accept (slippage protection)

**Returns:**

*   `amountOut` - The actual amount of `tokenOut` received

**Example:** Swap exactly 1000 USDC for at least 998 USDT:

```
uint128 amountOut = exchange.swapExactAmountIn(
    USDC_ADDRESS,
    USDT_ADDRESS,
    1000e6,      // Sell exactly 1000 USDC
    998e6        // Receive at least 998 USDT
);
```


### Swap Exact Amount Out[](https://docs.tempo.xyz/protocol/exchange/executing-swaps#swap-exact-amount-out)

Specify the exact amount of tokens you want to receive, and pay at most a maximum amount:

```
function swapExactAmountOut(
    address tokenIn,
    address tokenOut,
    uint128 amountOut,
    uint128 maxAmountIn
) external returns (uint128 amountIn)
```


**Parameters:**

*   `tokenIn` - The token address you're selling
*   `tokenOut` - The token address you're buying
*   `amountOut` - The exact amount of `tokenOut` to receive
*   `maxAmountIn` - Maximum amount of `tokenIn` you'll pay (slippage protection)

**Returns:**

*   `amountIn` - The actual amount of `tokenIn` spent

**Example:** Receive exactly 1000 USDT by spending at most 1002 USDC:

```
uint128 amountIn = exchange.swapExactAmountOut(
    USDC_ADDRESS,
    USDT_ADDRESS,
    1000e6,      // Receive exactly 1000 USDT
    1002e6       // Pay at most 1002 USDC
);
```


Quoting Prices[](https://docs.tempo.xyz/protocol/exchange/executing-swaps#quoting-prices)
-----------------------------------------------------------------------------------------

Before executing a swap, you can query the expected price using view functions that simulate the swap without executing it:

### Quote Exact Amount In[](https://docs.tempo.xyz/protocol/exchange/executing-swaps#quote-exact-amount-in)

```
function quoteSwapExactAmountIn(
    address tokenIn,
    address tokenOut,
    uint128 amountIn
) external view returns (uint128 amountOut)
```


Returns how much `tokenOut` you would receive for a given `amountIn`.

### Quote Exact Amount Out[](https://docs.tempo.xyz/protocol/exchange/executing-swaps#quote-exact-amount-out)

```
function quoteSwapExactAmountOut(
    address tokenIn,
    address tokenOut,
    uint128 amountOut
) external view returns (uint128 amountIn)
```


Returns how much `tokenIn` you would need to spend to receive a given `amountOut`.

**Example: Getting a price quote**

```
// Check how much USDT you'd get for 1000 USDC
uint128 expectedOut = exchange.quoteSwapExactAmountIn(
    USDC_ADDRESS,
    USDT_ADDRESS,
    1000e6
);
 
// Only execute if the price is acceptable
if (expectedOut >= 998e6) {
    exchange.swapExactAmountIn(USDC_ADDRESS, USDT_ADDRESS, 1000e6, 998e6);
}
```


How Swaps Execute[](https://docs.tempo.xyz/protocol/exchange/executing-swaps#how-swaps-execute)
-----------------------------------------------------------------------------------------------

When you call a swap function:

1.  **Balance Check**: The contract first checks your balance on the DEX
2.  **Transfer if Needed**: If your DEX balance is insufficient, tokens are transferred from your wallet
3.  **Order Matching**: The DEX walks through orders at each price tick, from best to worst:
    *   Orders are consumed in price-time priority order
    *   Each filled order credits the maker's balance on the DEX
    *   Continues until your swap is complete or limit price is reached
4.  **Slippage Check**: Reverts if `minAmountOut` (or `maxAmountIn`) constraints aren't met
5.  **Settlement**: Your output tokens are transferred to your wallet

Gas Costs[](https://docs.tempo.xyz/protocol/exchange/executing-swaps#gas-costs)
-------------------------------------------------------------------------------

Swap gas costs scale with the number of orders and ticks your trade crosses:

*   Base swap cost (transfers and setup)
*   Per-order cost (for each order filled)
*   Per-tick cost (for each price level crossed)
*   Per-flip cost (if any flip orders are triggered)

Larger swaps that cross more orders will cost more gas, but the cost per unit of volume decreases.

Token Balances on the DEX[](https://docs.tempo.xyz/protocol/exchange/executing-swaps#token-balances-on-the-dex)
---------------------------------------------------------------------------------------------------------------

The DEX allows you to track token balances directly within the DEX contract, which saves gas by avoiding ERC-20 transfers on every trade. When you execute a swap, the contract first checks your DEX balance and only transfers from your wallet if needed.

For complete details on checking balances, depositing, withdrawing, and managing your DEX balance, see the [DEX Balance](https://docs.tempo.xyz/protocol/exchange/exchange-balance) page.

# Providing Liquidity ⋅ Tempo
Provide liquidity to the DEX by placing limit orders or flip orders in the onchain orderbook.

When your orders are filled, you earn the spread between bid and ask prices while helping facilitate trades for other users.

You can only place orders on pairs between a token and its designated quote token. All TIP-20 tokens specify a quote token for trading pairs. [pathUSD](https://docs.tempo.xyz/protocol/exchange/pathUSD) can be used as a simple choice for a quote token.

Overview[](https://docs.tempo.xyz/protocol/exchange/providing-liquidity#overview)
---------------------------------------------------------------------------------

The DEX uses an onchain orderbook where you can place orders at specific price ticks. Orders are matched using price-time priority, meaning better-priced orders fill first, and within the same price, earlier orders fill first.

Unlike traditional AMMs, you specify exact prices where you want to buy or sell, giving you more precise control over your liquidity provision strategy.

Order Types[](https://docs.tempo.xyz/protocol/exchange/providing-liquidity#order-types)
---------------------------------------------------------------------------------------

### Limit Orders[](https://docs.tempo.xyz/protocol/exchange/providing-liquidity#limit-orders)

Standard orders that remain in the book at a specific price until filled or cancelled.

```
function place(
    address token,
    uint128 amount,
    bool isBid,
    int16 tick
) external returns (uint128 orderId)
```


**Parameters:**

*   `token` - The token address you're trading (must trade against its quote token)
*   `amount` - The amount of the token denominated in `token`
*   `isBid` - `true` for a buy order, `false` for a sell order
*   `tick` - The price tick: `(price - 1) * 100_000` where price is in quote token per token

**Returns:**

*   `orderId` - Unique identifier for this order

**Example: Place a bid to buy 1000 USDC at $0.9990**

```
// tick = (0.9990 - 1) * 100_000 = -10
uint128 orderId = exchange.place(
    USDC_ADDRESS,
    1000e6,      // Amount: 1000 USDC
    true,        // isBid: buying USDC
    -10          // tick: price = $0.9990
);
```


**Example: Place an ask to sell 1000 USDC at $1.0010**

```
// tick = (1.0010 - 1) * 100_000 = 10
uint128 orderId = exchange.place(
    USDC_ADDRESS,
    1000e6,      // Amount: 1000 USDC
    false,       // isBid: selling USDC
    10           // tick: price = $1.0010
);
```


### Flip Orders[](https://docs.tempo.xyz/protocol/exchange/providing-liquidity#flip-orders)

Special orders that automatically reverse to the opposite side when completely filled, creating perpetual liquidity similar to an automated market maker pool.

```
function placeFlip(
    address token,
    uint128 amount,
    bool isBid,
    int16 tick,
    int16 flipTick
) external returns (uint128 orderId)
```


**Parameters:**

*   All parameters from `place()`, plus:
*   `flipTick` - The price where the order will flip to when filled
    *   Must be greater than `tick` if `isBid` is true
    *   Must be less than `tick` if `isBid` is false

**Returns:**

*   `orderId` - Unique identifier for this flip order

**Example: Place a flip order providing liquidity on both sides**

```
// Place a bid at $0.9990 that flips to an ask at $1.0010
uint128 orderId = exchange.placeFlip(
    USDC_ADDRESS,
    1000e6,      // Amount: 1000 USDC
    true,        // isBid: start as a buy order
    -10,         // tick: buy at $0.9990
    10           // flipTick: sell at $1.0010 after filled
);
```


When this order is completely filled:

1.  You buy 1000 USDC at $0.9990
2.  A new order automatically sells 1000 USDC at $1.0010
3.  When that fills, it flips back to a bid at $0.9990
4.  This continues indefinitely, earning the spread each time

Understanding Ticks[](https://docs.tempo.xyz/protocol/exchange/providing-liquidity#understanding-ticks)
-------------------------------------------------------------------------------------------------------

Prices are specified using ticks with 0.1 basis point (0.001%) precision:

**Tick Formula:** `tick = (price - 1) × 100_000`

**Price Formula:** `price = 1 + (tick / 100_000)`

Where `price` is the token price in quote token units.

### Example Tick Calculations[](https://docs.tempo.xyz/protocol/exchange/providing-liquidity#example-tick-calculations)


|Price  |Tick|Calculation                  |
|-------|----|-----------------------------|
|$0.9990|-100|(0.9990 - 1) × 100_000 = -100|
|$0.9998|-20 |(0.9998 - 1) × 100_000 = -20 |
|$1.0000|0   |(1.0000 - 1) × 100_000 = 0   |
|$1.0002|20  |(1.0002 - 1) × 100_000 = 20  |
|$1.0010|100 |(1.0010 - 1) × 100_000 = 100 |


Bid vs Ask[](https://docs.tempo.xyz/protocol/exchange/providing-liquidity#bid-vs-ask)
-------------------------------------------------------------------------------------

*   **Bid (isBid = true)**: An order to _buy_ the token using its quote token
*   **Ask (isBid = false)**: An order to _sell_ the token for its quote token

For a USDC/USD pair where USD is the quote:

*   A bid buys USDC with USD at your specified price
*   An ask sells USDC for USD at your specified price

Order Execution Timeline[](https://docs.tempo.xyz/protocol/exchange/providing-liquidity#order-execution-timeline)
-----------------------------------------------------------------------------------------------------------------

Orders follow a specific lifecycle:

1.  **Placement**: When you call `place()` or `placeFlip()`:
    
    *   Tokens are debited from your DEX balance (or transferred if insufficient)
    *   Order is immediately added to the active book and visible to other contracts
    *   Returns an order ID immediately
2.  **Filling**: As market orders execute against your order:
    
    *   Your order fills partially or completely
    *   Proceeds are credited to your DEX balance
    *   If a flip order fills completely, a new order is immediately created on the opposite side

Cancelling Orders[](https://docs.tempo.xyz/protocol/exchange/providing-liquidity#cancelling-orders)
---------------------------------------------------------------------------------------------------

Remove an order from the book before it's filled:

```
function cancel(
    uint128 orderId
) external
```


**Example:**

```
// Cancel order #12345
exchange.cancel(12345);
```


Cancellations execute immediately, and any unfilled portion of your order is refunded to your [DEX balance](https://docs.tempo.xyz/protocol/exchange/exchange-balance).

# DEX Balance ⋅ Tempo
The Stablecoin DEX allows you to hold token balances directly using the DEX contract. This eliminates the need for token transfers on every trade, significantly reducing gas costs for active traders and liquidity providers.

Why DEX Balances?[](https://docs.tempo.xyz/protocol/exchange/exchange-balance#why-dex-balances)
-----------------------------------------------------------------------------------------------

When you trade or provide liquidity on the DEX, constantly transferring tokens between your wallet and the DEX contract wastes gas. By maintaining a balance via the DEX contract, you can:

*   **Save on gas costs** - Avoid ERC-20 transfer costs for each trade
*   **Trade more efficiently** - Execute multiple swaps without transfers between each trade
*   **Receive maker proceeds automatically** - When your limit orders are filled, proceeds are credited to your DEX balance instead of requiring a transfer for each fill

Checking Your Balance[](https://docs.tempo.xyz/protocol/exchange/exchange-balance#checking-your-balance)
--------------------------------------------------------------------------------------------------------

Use the DEX contract to view your balance of any token held on the DEX:

```
function balanceOf(
    address user,
    address token
) external view returns (uint128)
```


**Example:**

```
uint128 balance = exchange.balanceOf(msg.sender, USDC_ADDRESS);
```


Using Your DEX Balance[](https://docs.tempo.xyz/protocol/exchange/exchange-balance#using-your-dex-balance)
----------------------------------------------------------------------------------------------------------

Each transaction that you authorize will use your DEX balance before using funds you approve from your wallet. When you execute a swap or place an order, the DEX contract automatically:

1.  Checks if you have sufficient balance in the DEX
2.  If insufficient, transfers the needed amount from your wallet to your DEX balance
3.  Uses your DEX balance for the operation

Withdrawing from the DEX[](https://docs.tempo.xyz/protocol/exchange/exchange-balance#withdrawing-from-the-dex)
--------------------------------------------------------------------------------------------------------------

Transfer tokens from your DEX balance back to your wallet:

```
function withdraw(
    address token,
    uint128 amount
) external
```


**Parameters:**

*   `token` - The token address to withdraw
*   `amount` - The amount to withdraw

**Example:**

```
// Withdraw 1000 USDC from exchange to your wallet
exchange.withdraw(USDC_ADDRESS, 1000e6);
```


How Balances Work[](https://docs.tempo.xyz/protocol/exchange/exchange-balance#how-balances-work)
------------------------------------------------------------------------------------------------

### When Swapping[](https://docs.tempo.xyz/protocol/exchange/exchange-balance#when-swapping)

*   **Before swap**: Exchange checks your balance, transfers from wallet if needed
*   **After swap**: Output tokens are transferred directly to your wallet (not kept on exchange)

### When Placing Orders[](https://docs.tempo.xyz/protocol/exchange/exchange-balance#when-placing-orders)

*   **On placement**: Required tokens are debited from your exchange balance (or transferred from wallet if insufficient)
*   **When filled**: Proceeds are credited to your exchange balance
*   **On cancellation**: Unfilled portion is refunded to your exchange balance
# Place-only mode for next quote token ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/tips/tip-1001#abstract)
------------------------------------------------------------------

This TIP adds a `createNextPair` function to the Stablecoin DEX that creates a trading pair between a base token and its `nextQuoteToken()`, along with `place` and `placeFlip` overloads that accept a book key to target specific pairs. This enables market makers to place orders on the new pair before a quote token update is finalized, providing a smooth liquidity transition.

Motivation[](https://docs.tempo.xyz/protocol/tips/tip-1001#motivation)
----------------------------------------------------------------------

When a token issuer decides to change their quote token (via `setNextQuoteToken` and `completeQuoteTokenUpdate`), there is currently no way to establish liquidity on the new pair before the transition completes. This means that market makers will need to wait until the quote token has been updated before they can place orders, which could cause a period where there is no liquidity, or limited liquidity, for the token, which will interrupt swaps involving that token.

By allowing pair creation against `nextQuoteToken()`, this change allows users and market makers to add liquidity to the DEX before it is used on swaps. Since swaps route through `quoteToken()` (not `nextQuoteToken()`), the new pair operates in "place-only" mode: orders can be placed and cancelled, but no swaps route through it until `completeQuoteTokenUpdate()` is called.

* * *

Specification
-------------

New functions[](https://docs.tempo.xyz/protocol/tips/tip-1001#new-functions)
----------------------------------------------------------------------------

Add the following functions to the Stablecoin DEX interface:

```
/// @notice Creates a trading pair between a base token and its next quote token
/// @param base The base token address
/// @return key The pair key for the created pair
/// @dev Reverts if:
///   - The base token has no next quote token staged (nextQuoteToken is zero)
///   - The pair already exists
///   - Either token is not USD-denominated
function createNextPair(address base) external returns (bytes32 key);
 
/// @notice Places an order on a specific pair identified by book key
/// @param bookKey The pair key identifying the orderbook
/// @param token The base token of the pair
/// @param amount The order amount in base tokens
/// @param isBid True for buy orders, false for sell orders
/// @param tick The price tick for the order
/// @return orderId The ID of the placed order
function place(bytes32 bookKey, address token, uint128 amount, bool isBid, int16 tick) external returns (uint128 orderId);
 
/// @notice Places a flip order on a specific pair identified by book key
/// @param bookKey The pair key identifying the orderbook
/// @param token The base token of the pair
/// @param amount The order amount in base tokens
/// @param isBid True for buy orders, false for sell orders
/// @param tick The price tick for the order
/// @param flipTick The price tick for the flipped order when filled
/// @param internalBalanceOnly If true, only use internal balance for the flipped order
/// @return orderId The ID of the placed order
function placeFlip(bytes32 bookKey, address token, uint128 amount, bool isBid, int16 tick, int16 flipTick, bool internalBalanceOnly) external returns (uint128 orderId);
```


Behavior[](https://docs.tempo.xyz/protocol/tips/tip-1001#behavior)
------------------------------------------------------------------

### Pair creation[](https://docs.tempo.xyz/protocol/tips/tip-1001#pair-creation)

`createNextPair(base)` creates a pair between `base` and `base.nextQuoteToken()`. The function:

1.  Calls `nextQuoteToken()` on the base token
2.  Reverts with `NO_NEXT_QUOTE_TOKEN` if the result is `address(0)`
3.  Validates both tokens are USD-denominated (same as `createPair`)
4.  Creates the pair using the same mechanism as `createPair`
5.  Emits `PairCreated(key, base, nextQuoteToken)`

### Place-only mode[](https://docs.tempo.xyz/protocol/tips/tip-1001#place-only-mode)

Once the pair exists, it supports the full order lifecycle:

*   `place(bookKey, ...)` and `placeFlip(bookKey, ...)` allow placing orders on the pair
*   `cancel` and `cancelStaleOrder` work normally (they use order ID, not pair lookup)
*   `books` returns accurate data (it takes the book key directly)

The new `place` and `placeFlip` overloads are required because the existing functions derive the pair from `token.quoteToken()`, which would look up the wrong pair. The overloads accept a `bookKey` parameter to target the correct pair.

Swap functions (`swapExactAmountIn`, `swapExactAmountOut`) and quote functions (`quoteSwapExactAmountIn`, `quoteSwapExactAmountOut`) do not route through this pair because routing uses `quoteToken()` to find paths between tokens.

### After quote token update[](https://docs.tempo.xyz/protocol/tips/tip-1001#after-quote-token-update)

When the token issuer calls `completeQuoteTokenUpdate()`:

1.  The token's `quoteToken()` changes to what was `nextQuoteToken()`
2.  The token's `nextQuoteToken()` becomes `address(0)`
3.  The existing pair (created via `createNextPair`) is now the active pair
4.  Swaps begin routing through the pair

The old pair (against the previous quote token) remains but will no longer be used for routing swaps involving this base token. Orders on it can be canceled using their ID.

New error[](https://docs.tempo.xyz/protocol/tips/tip-1001#new-error)
--------------------------------------------------------------------

```
/// @notice The base token has no next quote token staged
error NO_NEXT_QUOTE_TOKEN();
```


Events[](https://docs.tempo.xyz/protocol/tips/tip-1001#events)
--------------------------------------------------------------

No new events. The existing `PairCreated` event is emitted by `createNextPair`, and the existing `OrderPlaced` event is emitted by the `place` and `placeFlip` overloads.

* * *

Invariants
----------

*   A pair created via `createNextPair` must be identical to one created via `createPair` once `completeQuoteTokenUpdate` is called
*   `createNextPair` must revert if `nextQuoteToken()` returns `address(0)`
*   `createNextPair` must revert if the pair already exists (same as `createPair`)
*   Orders placed on a next-quote-token pair must be executable via swaps after the quote token update completes
*   Swap routing must not change until `completeQuoteTokenUpdate` is called on the base token
# Prevent crossed orders and allow same-tick flip orders ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/tips/tip-1002#abstract)
------------------------------------------------------------------

This TIP makes two related changes to the Stablecoin DEX:

1.  **Prevent crossed orders**: Modify `place` and `placeFlip` to reject orders that would cross existing orders on the opposite side of the book. An order "crosses" when a bid is placed at a tick higher than the best ask, or an ask is placed at a tick lower than the best bid.
    
2.  **Allow same-tick flip orders**: Relax the `placeFlip` validation to allow `flipTick` to equal `tick`, enabling flip orders that flip to the same price.
    

Motivation[](https://docs.tempo.xyz/protocol/tips/tip-1002#motivation)
----------------------------------------------------------------------

### Preventing crossed orders[](https://docs.tempo.xyz/protocol/tips/tip-1002#preventing-crossed-orders)

Currently, the Stablecoin DEX allows orders to be placed at any valid tick, even if they would cross existing orders. Since matching only occurs during swaps (not during order placement), crossed orders can accumulate in the order book. This is unusual behavior and could confuse market makers who are accustomed to books that do not allow crossing.

By preventing crossed orders at placement time, the order book maintains a clean invariant: `best_bid_tick <= best_ask_tick`.

### Allowing same-tick flip orders[](https://docs.tempo.xyz/protocol/tips/tip-1002#allowing-same-tick-flip-orders)

Currently, `placeFlip` requires `flipTick` to be strictly on the opposite side of `tick` (e.g., for a bid, `flipTick > tick`). This prevents use cases like instant token convertibility, where an issuer wants to place flip orders on both sides at the same tick to create a stable two-sided market that automatically replenishes when orders are filled.

* * *

Specification
-------------

Modified behavior[](https://docs.tempo.xyz/protocol/tips/tip-1002#modified-behavior)
------------------------------------------------------------------------------------

The `place` and `placeFlip` functions (including the `bookKey` overloads from TIP-1001) are modified to check for crossing before accepting an order:

*   **For bids**: Revert if `tick > best_ask_tick` (when `best_ask_tick` exists)
*   **For asks**: Revert if `tick < best_bid_tick` (when `best_bid_tick` exists)

### Same-tick orders[](https://docs.tempo.xyz/protocol/tips/tip-1002#same-tick-orders)

Orders at the same tick as the best order on the opposite side are **allowed**. This means:

*   A bid at `tick == best_ask_tick` is allowed
*   An ask at `tick == best_bid_tick` is allowed

While this is non-standard behavior for most order books (which would immediately match same-tick orders), it is intentionally permitted to support flip orders that flip to the same tick (see below).

Same-tick flip orders[](https://docs.tempo.xyz/protocol/tips/tip-1002#same-tick-flip-orders)
--------------------------------------------------------------------------------------------

The `placeFlip` validation is relaxed to allow `flipTick == tick`:

*   **Current behavior**: For bids, `flipTick > tick` required; for asks, `flipTick < tick` required
*   **New behavior**: For bids, `flipTick >= tick` required; for asks, `flipTick <= tick` required

This enables use cases like instant token convertibility, where an issuer places flip orders on both sides at the same tick to create a stable two-sided market that automatically replenishes when orders are filled.

Interaction with TIP-1001[](https://docs.tempo.xyz/protocol/tips/tip-1002#interaction-with-tip-1001)
----------------------------------------------------------------------------------------------------

If TIP-1001 is accepted, the crossing check only applies when the pair is **active**—that is, when the pair's quote token equals the base token's current `quoteToken()`.

For pairs created via `createNextPair` (where the quote token is the base token's `nextQuoteToken()`), the crossing check is skipped. This allows orders to accumulate freely during "place-only mode" before the quote token update is finalized. Such orders would likely be arbitraged nearly instantly once the pair launches, but this prevents someone from causing a denial-of-service to one side of the book by placing an extremely aggressive order on the other side.

New error[](https://docs.tempo.xyz/protocol/tips/tip-1002#new-error)
--------------------------------------------------------------------

```
/// @notice The order would cross existing orders on the opposite side
error ORDER_WOULD_CROSS();
```


Events[](https://docs.tempo.xyz/protocol/tips/tip-1002#events)
--------------------------------------------------------------

No new events.

* * *

Invariants
----------

*   On active pairs, `best_bid_tick <= best_ask_tick` after any successful `place` or `placeFlip` call
*   On inactive pairs (per TIP-1001), no crossing check is enforced
*   Flip orders may create orders at the same tick as the opposite side, potentially resulting in `best_bid_tick == best_ask_tick`
# Client order IDs ⋅ Tempo
TIP-1003: Client order IDs
--------------------------

Abstract[](https://docs.tempo.xyz/protocol/tips/tip-1003#abstract)
------------------------------------------------------------------

This TIP adds support for optional client order IDs (`clientOrderId`) to the Stablecoin DEX. Users can specify a `uint128` identifier when placing orders, which serves as an idempotency key and a predictable handle for the order. The system-generated `orderId` is not predictable before transaction execution, making client order IDs useful for order management.

Motivation[](https://docs.tempo.xyz/protocol/tips/tip-1003#motivation)
----------------------------------------------------------------------

Traditional exchanges allow users to specify a client order ID (called `ClOrdID` in FIX protocol, `cloid` in Hyperliquid) for several reasons:

1.  **Idempotency**: If a transaction is submitted twice (e.g., due to network issues), the duplicate can be detected and rejected
2.  **Predictable reference**: Users know the order identifier before the transaction confirms, enabling them to prepare cancel requests or track orders without waiting for confirmation
3.  **Integration**: External systems can use their own ID schemes to correlate orders

* * *

Specification
-------------

New storage[](https://docs.tempo.xyz/protocol/tips/tip-1003#new-storage)
------------------------------------------------------------------------

A new mapping tracks active client order IDs per user:

```
mapping(address user => mapping(uint128 clientOrderId => uint128 orderId)) public clientOrderIds;
```


Modified functions[](https://docs.tempo.xyz/protocol/tips/tip-1003#modified-functions)
--------------------------------------------------------------------------------------

All order placement functions gain an optional `clientOrderId` parameter:

```
/// @notice Places an order with an optional client order ID
/// @param token The base token of the pair
/// @param amount The order amount in base tokens
/// @param isBid True for buy orders, false for sell orders
/// @param tick The price tick for the order
/// @param clientOrderId Optional client-specified ID (0 for none)
/// @return orderId The system-assigned order ID
function place(
    address token,
    uint128 amount,
    bool isBid,
    int16 tick,
    uint128 clientOrderId
) external returns (uint128 orderId);
 
/// @notice Places an order on a specific pair with an optional client order ID
/// @dev Overload from TIP-1001
function place(
    bytes32 bookKey,
    address token,
    uint128 amount,
    bool isBid,
    int16 tick,
    uint128 clientOrderId
) external returns (uint128 orderId);
 
/// @notice Places a flip order with an optional client order ID
function placeFlip(
    address token,
    uint128 amount,
    bool isBid,
    int16 tick,
    int16 flipTick,
    bool internalBalanceOnly,
    uint128 clientOrderId
) external returns (uint128 orderId);
 
/// @notice Places a flip order on a specific pair with an optional client order ID
/// @dev Overload from TIP-1001
function placeFlip(
    bytes32 bookKey,
    address token,
    uint128 amount,
    bool isBid,
    int16 tick,
    int16 flipTick,
    bool internalBalanceOnly,
    uint128 clientOrderId
) external returns (uint128 orderId);
```


New functions[](https://docs.tempo.xyz/protocol/tips/tip-1003#new-functions)
----------------------------------------------------------------------------

```
/// @notice Cancels an order by its client order ID
/// @param clientOrderId The client-specified order ID
function cancelByClientOrderId(uint128 clientOrderId) external;
 
/// @notice Gets the system order ID for a client order ID
/// @param user The user who placed the order
/// @param clientOrderId The client-specified order ID
/// @return orderId The system-assigned order ID, or 0 if not found
function getOrderByClientOrderId(address user, uint128 clientOrderId) external view returns (uint128 orderId);
```


Behavior[](https://docs.tempo.xyz/protocol/tips/tip-1003#behavior)
------------------------------------------------------------------

### Placing orders with clientOrderId[](https://docs.tempo.xyz/protocol/tips/tip-1003#placing-orders-with-clientorderid)

When `clientOrderId` is non-zero:

1.  Check if `clientOrderIds[msg.sender][clientOrderId]` maps to an active order
2.  If it does, revert with `DUPLICATE_CLIENT_ORDER_ID`
3.  Otherwise, proceed with order placement and set `clientOrderIds[msg.sender][clientOrderId] = orderId`

When `clientOrderId` is zero, no client order ID tracking occurs.

### Uniqueness and reuse[](https://docs.tempo.xyz/protocol/tips/tip-1003#uniqueness-and-reuse)

A `clientOrderId` must be unique among a user's **active orders**. Once an order is filled or cancelled, its `clientOrderId` can be reused. This matches the standard FIX protocol behavior where `ClOrdID` uniqueness is required only for working orders.

When an order reaches a terminal state (filled or cancelled), the `clientOrderIds` mapping entry is cleared.

### Flip orders[](https://docs.tempo.xyz/protocol/tips/tip-1003#flip-orders)

When a flip order is filled and creates a new order on the opposite side:

1.  The new (flipped) order inherits the original order's `clientOrderId`
2.  The `clientOrderIds` mapping is updated to point to the new order ID
3.  This allows users to track their position across flips using a single `clientOrderId`

If the original order had no `clientOrderId` (was zero), the flipped order also has no `clientOrderId`.

### Cancellation[](https://docs.tempo.xyz/protocol/tips/tip-1003#cancellation)

`cancelByClientOrderId(clientOrderId)` looks up `clientOrderIds[msg.sender][clientOrderId]` and cancels that order. It reverts if no active order exists for that `clientOrderId`.

New event[](https://docs.tempo.xyz/protocol/tips/tip-1003#new-event)
--------------------------------------------------------------------

```
/// @notice Emitted when an order is placed (V2 with clientOrderId)
/// @dev Replaces OrderPlaced for new orders
event OrderPlacedV2(
    uint128 indexed orderId,
    address indexed maker,
    address token,
    uint128 amount,
    bool isBid,
    int16 tick,
    bool isFlipOrder,
    int16 flipTick,
    uint128 clientOrderId
);
```


`OrderPlacedV2` is identical to `OrderPlaced` but adds the `clientOrderId` field. When an order is placed, only `OrderPlacedV2` is emitted (not both events).

New errors[](https://docs.tempo.xyz/protocol/tips/tip-1003#new-errors)
----------------------------------------------------------------------

```
/// @notice The client order ID is already in use by an active order
error DUPLICATE_CLIENT_ORDER_ID();
 
/// @notice No active order found for the given client order ID
error CLIENT_ORDER_ID_NOT_FOUND();
```


* * *

Invariants
----------

*   A non-zero `clientOrderId` maps to at most one active order per user
*   `clientOrderIds[user][clientOrderId]` is cleared when the order is filled or cancelled
*   Flip orders inherit `clientOrderId` and update the mapping atomically
*   `clientOrderId = 0` is reserved to mean "no client order ID"
# Fix ask swap rounding loss ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/tips/tip-1005#abstract)
------------------------------------------------------------------

This TIP fixes a rounding bug in the `swapExactAmountIn` function when filling ask orders. Due to double-rounding, the maker can receive slightly less quote tokens than the taker paid, causing tokens to be lost.

Motivation[](https://docs.tempo.xyz/protocol/tips/tip-1005#motivation)
----------------------------------------------------------------------

When a taker swaps quote tokens for base tokens against an ask order, the following calculation occurs:

1.  Convert taker's `amountIn` (quote) to base: `base_out = floor(amountIn / price)`
2.  Credit maker with quote: `makerReceives = ceil(base_out * price)`

Due to the floor in step 1, `makerReceives` can be less than `amountIn`. For example:

*   Taker pays `amountIn = 102001` quote at price 1.02 (tick 2000)
*   `base_out = floor(102001 / 1.02) = 100000`
*   `makerReceives = ceil(100000 * 1.02) = 102000`
*   **1 token is lost**

This violates the zero-sum invariant: the taker pays more than the maker receives. It also means there is no canonical amount swapped—the trade for the maker is different from the trade for the taker.

* * *

Specification
-------------

Bug location[](https://docs.tempo.xyz/protocol/tips/tip-1005#bug-location)
--------------------------------------------------------------------------

The bug is in `_fillOrdersExactIn` when processing ask orders (the `baseForQuote = false` path). Specifically, when a partial fill occurs:

1.  `fillAmount` (base) is calculated by rounding down: `baseOut = (remainingIn * PRICE_SCALE) / price`
2.  `_fillOrder` is called with `fillAmount`
3.  Inside `_fillOrder`, the maker's quote credit is re-derived: `quoteAmount = ceil(fillAmount * price)`

The re-derivation in step 3 loses the original `remainingIn` information.

Fix[](https://docs.tempo.xyz/protocol/tips/tip-1005#fix)
--------------------------------------------------------

For partial fills in the ask path, pass the actual `remainingIn` (quote) to `_fillOrder` and use it directly for the maker's credit, rather than re-deriving it from `fillAmount`.

The fix requires:

1.  Modify `_fillOrder` to accept an optional `quoteOverride` parameter for ask orders
2.  In `_fillOrdersExactIn`, when partially filling an ask, pass `remainingIn` as the quote override
3.  When `quoteOverride` is provided, use it directly for the maker's balance increment instead of computing `ceil(fillAmount * price)`

Reference implementation changes[](https://docs.tempo.xyz/protocol/tips/tip-1005#reference-implementation-changes)
------------------------------------------------------------------------------------------------------------------

The fix requires changes to two functions in `[docs/specs/src/StablecoinDEX.sol](https://github.com/tempoxyz/tempo/blob/main/docs/specs/src/StablecoinDEX.sol)`:

### 1\. `_fillOrder` ([line 551-556](https://github.com/tempoxyz/tempo/blob/main/docs/specs/src/StablecoinDEX.sol#L551-L556))
[](https://docs.tempo.xyz/protocol/tips/tip-1005#1-_fillorder-line-551-556)

Add an optional `quoteOverride` parameter. When non-zero and the order is an ask, use `quoteOverride` directly for the maker's balance increment instead of computing `ceil(fillAmount * price)`.

```
// Before:
uint128 quoteAmount =
    uint128((uint256(fillAmount) * uint256(price) + PRICE_SCALE - 1) / PRICE_SCALE);
balances[order.maker][book.quote] += quoteAmount;
 
// After:
uint128 quoteAmount = quoteOverride > 0
    ? quoteOverride
    : uint128((uint256(fillAmount) * uint256(price) + PRICE_SCALE - 1) / PRICE_SCALE);
balances[order.maker][book.quote] += quoteAmount;
```


### 2\. `_fillOrdersExactIn` ([line 923-926](https://github.com/tempoxyz/tempo/blob/main/docs/specs/src/StablecoinDEX.sol#L923-L926))
[](https://docs.tempo.xyz/protocol/tips/tip-1005#2-_fillordersexactin-line-923-926)

In the partial fill branch for asks, pass `remainingIn` as the quote override:

```
// Before:
orderId = _fillOrder(orderId, fillAmount);
 
// After (for partial fills where fillAmount == baseOut):
orderId = _fillOrder(orderId, fillAmount, remainingIn);
```


Affected code paths[](https://docs.tempo.xyz/protocol/tips/tip-1005#affected-code-paths)
----------------------------------------------------------------------------------------

*   `_fillOrdersExactIn` with `baseForQuote = false` (ask path), partial fill case only
*   Full fills are not affected because the quote amount is derived from `order.remaining`, not `remainingIn`
*   Bid swaps are not affected because the taker pays base tokens directly

Example: Before and after[](https://docs.tempo.xyz/protocol/tips/tip-1005#example-before-and-after)
---------------------------------------------------------------------------------------------------

**Before (buggy):**

```
amountIn = 102001 quote
base_out = floor(102001 / 1.02) = 100000
makerReceives = ceil(100000 * 1.02) = 102000
Lost: 1 token

```


**After (fixed):**

```
amountIn = 102001 quote
base_out = floor(102001 / 1.02) = 100000
makerReceives = 102001 (passed directly)
Lost: 0 tokens

```


* * *

Invariants
----------

*   Zero-sum: for any swap, `takerPaid == makerReceived` (within the same token)
*   Taker receives `floor(amountIn / price)` base tokens (rounds in favor of protocol)
*   Maker receives exactly what taker paid in quote tokens
