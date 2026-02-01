import * as Hex from 'ox/Hex';
import * as Hash from 'ox/Hash';
import { parseAccount } from 'viem/utils'; // viem/accounts might be safer but often utils has parseAccount? No, usually accounts/utils.  Actually `import { parseAccount } from 'viem/accounts'` is correct in viem exports?
// viem package.json: "./accounts": { ... }
// So `import { parseAccount } from 'viem/accounts'` is correct.
import { parseAccount as parseAccountViem } from 'viem/accounts';
import { parseEventLogs } from 'viem/utils';
import { defineCall } from '../internal/utils.js';
import * as Abis from '../Abis.js';

const readContract = (client, args) => client.readContract(args);
const writeContract = (client, args) => client.writeContract(args);
const watchContractEvent = (client, args) => client.watchContractEvent(args);
const multicall = (client, args) => client.multicall(args);
import * as Addresses from '../Addresses.js';

/**
 * Creates a new trading pair on the DEX.
 */
export async function createPair(client, parameters) {
    return createPairInner(writeContract, client, parameters);
}

createPair.call = createPairCall;
createPair.extractEvent = createPairExtractEvent;

function createPairCall(args) {
    const { base } = args;
    return defineCall({
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        functionName: 'createPair',
        args: [base],
    });
}

function createPairExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.stablecoinDex,
        logs,
        eventName: 'PairCreated',
        strict: true,
    });
    if (!log) throw new Error('`PairCreated` event not found.');
    return log;
}

async function createPairInner(action, client, parameters) {
    const { base, ...rest } = parameters;
    const call = createPairCall({ base });
    return await action(client, {
        ...rest,
        ...call,
    });
}

/**
 * Places a limit order on the orderbook.
 */
export async function place(client, parameters) {
    return placeInner(writeContract, client, parameters);
}

place.call = placeCall;
place.extractEvent = placeExtractEvent;

function placeCall(args) {
    const { token, amount, type, tick } = args;
    const isBid = type === 'buy';
    return defineCall({
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        functionName: 'place',
        args: [token, amount, isBid, tick],
    });
}

function placeExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.stablecoinDex,
        logs,
        eventName: 'OrderPlaced',
        strict: true,
    });
    if (!log) throw new Error('`OrderPlaced` event not found.');
    return log;
}

async function placeInner(action, client, parameters) {
    const { amount, token, type, tick, ...rest } = parameters;
    const call = placeCall({ amount, token, type, tick });
    return await action(client, {
        ...rest,
        ...call,
    });
}

/**
 * Places a flip order.
 */
export async function placeFlip(client, parameters) {
    return placeFlipInner(writeContract, client, parameters);
}

placeFlip.call = placeFlipCall;
placeFlip.extractEvent = placeFlipExtractEvent;

function placeFlipCall(args) {
    const { token, amount, type, tick, flipTick } = args;
    const isBid = type === 'buy';
    return defineCall({
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        functionName: 'placeFlip',
        args: [token, amount, isBid, tick, flipTick],
    });
}

function placeFlipExtractEvent(logs) {
    const parsedLogs = parseEventLogs({
        abi: Abis.stablecoinDex,
        logs,
        eventName: 'OrderPlaced',
        strict: true,
    });
    const log = parsedLogs.find((l) => l.args.isFlipOrder);
    if (!log) throw new Error('`OrderPlaced` event (flip order) not found.');
    return log;
}

async function placeFlipInner(action, client, parameters) {
    const { amount, flipTick, tick, token, type, ...rest } = parameters;
    const call = placeFlipCall({ amount, flipTick, tick, token, type });
    return await action(client, {
        ...rest,
        ...call,
    });
}

/**
 * Cancels an order.
 */
export async function cancel(client, parameters) {
    return cancelInner(writeContract, client, parameters);
}

cancel.call = cancelCall;
cancel.extractEvent = cancelExtractEvent;

function cancelCall(args) {
    const { orderId } = args;
    return defineCall({
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        functionName: 'cancel',
        args: [orderId],
    });
}

function cancelExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.stablecoinDex,
        logs,
        eventName: 'OrderCancelled',
        strict: true,
    });
    if (!log) throw new Error('`OrderCancelled` event not found.');
    return log;
}

async function cancelInner(action, client, parameters) {
    const { orderId, ...rest } = parameters;
    const call = cancelCall({ orderId });
    return await action(client, {
        ...rest,
        ...call,
    });
}

/**
 * Cancels a stale order.
 */
export async function cancelStale(client, parameters) {
    return cancelStaleInner(writeContract, client, parameters);
}

cancelStale.call = cancelStaleCall;
cancelStale.extractEvent = cancelStaleExtractEvent;

function cancelStaleCall(args) {
    const { orderId } = args;
    return defineCall({
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        functionName: 'cancelStaleOrder',
        args: [orderId],
    });
}

function cancelStaleExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.stablecoinDex,
        logs,
        eventName: 'OrderCancelled',
        strict: true,
    });
    if (!log) throw new Error('`OrderCancelled` event not found.');
    return log;
}

async function cancelStaleInner(action, client, parameters) {
    const { orderId, ...rest } = parameters;
    const call = cancelStaleCall({ orderId });
    return await action(client, {
        ...rest,
        ...call,
    });
}
/**
 * Buys tokens (swapExactAmountIn if buying base, swapExactAmountOut if buying quote... wait, DEX logic might differ).
 * 
 * Tempo DEX 'buy' usually means paying Quote to get Base?
 * Or is it TokenIn -> TokenOut?
 * 
 * In dex.ts:
 * export async function buy(client, parameters)
 * It calls `buy.inner`.
 * 
 * `buy.call`:
 * args: { tokenIn, tokenOut, amountIn, minAmountOut }
 * function: 'swapExactAmountIn' 
 * 
 * Ah, `buy` uses `swapExactAmountIn`?
 * 
 * Let's check `dex.ts` `buy` implementation helper.
 * 
 * `buy.call` lines 272-278:
 * functionName: 'swapExactAmountIn',
 * args: [tokenIn, tokenOut, amountIn, minAmountOut]
 * 
 * So `buy` is just a wrapper for `swapExactAmountIn`.
 */

// ... implementing buy/sell logic ...
/**
 * Since "buy" and "sell" in the original file map to `swapExactAmountIn`, I will implement them.
 * But wait, `buy` usually implies getting the "Base" token.
 * `dex.ts` just exposes `buy` which calls `swapExactAmountIn`. 
 * It's just a semantic wrapper? 
 * 
 * Actually, `dex.ts` has `buy` and `sell` exports.
 * 
 * `buy` -> `swapExactAmountIn`
 * `sell` -> `swapExactAmountIn` (Wait, checking line 1741 calls `sell.call` which calls `swapExactAmountIn` too?)
 * 
 * Yes, both seem to use `swapExactAmountIn`.
 * 
 * Wait, `getBuyQuote` uses `quoteSwapExactAmountOut` (line 941).
 * 
 * Let's just implement `swap` functions generically if simpler? 
 * But I should mirror the exports for compatibility.
 */
export async function buy(client, parameters) {
    return buyInner(writeContract, client, parameters);
}
buy.call = buyCall;
// No event extract needed typically for simple write, but we can return receipt.

function buyCall(args) {
    const { tokenIn, tokenOut, amountIn, minAmountOut } = args;
    return defineCall({
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        functionName: 'swapExactAmountIn',
        args: [tokenIn, tokenOut, amountIn, minAmountOut],
    });
}

async function buyInner(action, client, parameters) {
    const { tokenIn, tokenOut, amountIn, minAmountOut, ...rest } = parameters;
    const call = buyCall({ tokenIn, tokenOut, amountIn, minAmountOut });
    return await action(client, { ...rest, ...call });
}

export async function sell(client, parameters) {
    return sellInner(writeContract, client, parameters);
}
sell.call = sellCall;

function sellCall(args) {
    const { tokenIn, tokenOut, amountIn, minAmountOut } = args;
    return defineCall({
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        functionName: 'swapExactAmountIn',
        args: [tokenIn, tokenOut, amountIn, minAmountOut],
    });
}
async function sellInner(action, client, parameters) {
    const { tokenIn, tokenOut, amountIn, minAmountOut, ...rest } = parameters;
    const call = sellCall({ tokenIn, tokenOut, amountIn, minAmountOut });
    return await action(client, { ...rest, ...call });
}

/**
 * Withdraw
 */
export async function withdraw(client, parameters) {
    return withdrawInner(writeContract, client, parameters);
}

withdraw.call = withdrawCall;

function withdrawCall(args) {
    const { token, amount } = args;
    return defineCall({
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        functionName: 'withdraw',
        args: [token, amount],
    });
}

async function withdrawInner(action, client, parameters) {
    const { token, amount, ...rest } = parameters;
    const call = withdrawCall({ token, amount });
    return await action(client, { ...rest, ...call });
}

/**
 * Reads
 */

export async function getBalance(client, parameters) {
    const { account: acc = client.account, token, ...rest } = parameters;
    // Handle account parsing
    const address = acc ? (typeof acc === 'string' ? acc : acc.address) : undefined;
    // Simplified parsing. Real `parseAccount` is better.
    // I imported `parseAccountViem` as `parseAccountViem`.
    const validatedAddress = parseAccountViem(acc).address;

    if (!validatedAddress) throw new Error('account is required.');

    return readContract(client, {
        ...rest,
        ...getBalanceCall({ account: validatedAddress, token }),
    });
}
getBalance.call = getBalanceCall;
function getBalanceCall(args) {
    const { account, token } = args;
    return defineCall({
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        args: [account, token],
        functionName: 'balanceOf',
    });
}

export async function getOrder(client, parameters) {
    const { orderId, ...rest } = parameters;
    return readContract(client, {
        ...rest,
        ...getOrderCall({ orderId }),
    });
}
getOrder.call = getOrderCall;
function getOrderCall(args) {
    const { orderId } = args;
    return defineCall({
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        args: [orderId],
        functionName: 'getOrder',
    });
}

export async function getOrderbook(client, parameters) {
    const { base, quote, ...rest } = parameters;
    return readContract(client, {
        ...rest,
        ...getOrderbookCall({ base, quote }),
    });
}
getOrderbook.call = getOrderbookCall;
function getOrderbookCall(args) {
    const { base, quote } = args;
    const pairKey = getPairKey(base, quote);
    return defineCall({
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        args: [pairKey],
        functionName: 'books',
    });
}

export async function getTickLevel(client, parameters) {
    const { base, tick, isBid, ...rest } = parameters;
    const [head, tail, totalLiquidity] = await readContract(client, {
        ...rest,
        ...getTickLevelCall({ base, tick, isBid }),
    });
    return { head, tail, totalLiquidity };
}
getTickLevel.call = getTickLevelCall;
function getTickLevelCall(args) {
    const { base, tick, isBid } = args;
    return defineCall({
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        args: [base, tick, isBid],
        functionName: 'getTickLevel',
    });
}

// Watchers
export function watchOrderPlaced(client, parameters) {
    const { onOrderPlaced, maker, token, ...rest } = parameters;
    return watchContractEvent(client, {
        ...rest,
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        eventName: 'OrderPlaced',
        args: {
            // Check if undefined check is needed or if strict filtering applies
            ...(maker !== undefined && { maker }),
            ...(token !== undefined && { token }),
        },
        onLogs: (logs) => {
            for (const log of logs) onOrderPlaced(log.args, log);
        },
        strict: true,
    });
}

export function watchFlipOrderPlaced(client, parameters) {
    const { onFlipOrderPlaced, maker, token, ...rest } = parameters;
    return watchContractEvent(client, {
        ...rest,
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        eventName: 'OrderPlaced',
        args: {
            ...(maker !== undefined && { maker }),
            ...(token !== undefined && { token }),
        },
        onLogs: (logs) => {
            for (const log of logs) {
                if (log.args.isFlipOrder) onFlipOrderPlaced(log.args, log);
            }
        },
        strict: true,
    });
}

export function watchOrderCancelled(client, parameters) {
    const { onOrderCancelled, orderId, ...rest } = parameters;
    return watchContractEvent(client, {
        ...rest,
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        eventName: 'OrderCancelled',
        args: orderId !== undefined ? { orderId } : undefined,
        onLogs: (logs) => {
            for (const log of logs) onOrderCancelled(log.args, log);
        },
        strict: true,
    });
}

export function watchOrderFilled(client, parameters) {
    const { onOrderFilled, maker, taker, orderId, ...rest } = parameters;
    return watchContractEvent(client, {
        ...rest,
        address: Addresses.stablecoinDex,
        abi: Abis.stablecoinDex,
        eventName: 'OrderFilled',
        args: {
            ...(orderId !== undefined && { orderId }),
            ...(maker !== undefined && { maker }),
            ...(taker !== undefined && { taker }),
        },
        onLogs: (logs) => {
            for (const log of logs) onOrderFilled(log.args, log);
        },
        strict: true,
    });
}

function getPairKey(base, quote) {
    return Hash.keccak256(Hex.concat(base, quote));
}
