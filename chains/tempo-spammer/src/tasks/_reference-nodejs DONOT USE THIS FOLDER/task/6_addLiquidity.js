import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { createRandomStableForWallet } from './4_createStable.js';
import { getGasWithMultiplier } from '../utils/helpers.js';
import { loadCreatedTokens } from '../utils/wallet.js';
import { ConcurrentService } from '../utils/tempoConcurrent.js';

const DEX_ABI = [
    "function place(address token, uint128 amount, bool isBid, int16 tick) external returns (uint128 orderId)",
    "function placeFlip(address token, uint128 amount, bool isBid, int16 tick, int16 flipTick) external returns (uint128 orderId)",
    "function createPair(address base) external returns (bytes32 key)",
    "function pairKey(address tokenA, address tokenB) external pure returns (bytes32 key)",
    "function books(bytes32 pairKey) external view returns (address base, address quote, int16 bestBidTick, int16 bestAskTick)"
];

const ERC20_ABI = [
    "function balanceOf(address owner) view returns (uint256)",
    "function allowance(address owner, address spender) view returns (uint256)",
    "function approve(address spender, uint256 amount) returns (bool)",
    "function decimals() view returns (uint8)",
    "function symbol() view returns (string)",
    "function quoteToken() view returns (address)"
];

export async function addRandomLiquidityForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const tokenEntries = Object.entries(CONFIG.TOKENS);

    // Load dynamic tokens for this wallet
    try {
        const createdTokensData = loadCreatedTokens();
        const checksumAddress = ethers.getAddress(wallet.address);
        const myCreatedTokens = createdTokensData[checksumAddress] || [];

        for (const token of myCreatedTokens) {
            tokenEntries.push([token.symbol, token.token]);
        }
    } catch (e) {
        if (!silent) console.log(`${COLORS.dim}Error loading dynamic tokens: ${e.message}${COLORS.reset}`);
    }

    if (tokenEntries.length === 0) {
        if (!silent) console.log(`${COLORS.fg.red}No tokens configured.${COLORS.reset}`);
        return;
    }

    // 1. Find a valid Base Token (Must have quoteToken)
    let action = null;
    const shuffled = [...tokenEntries].sort(() => 0.5 - Math.random());

    for (const [sym, addr] of shuffled) {
        try {
            const c = new ethers.Contract(addr, ERC20_ABI, wallet);
            let quoteAddr;
            try { quoteAddr = await c.quoteToken(); } catch (e) { continue; }

            if (!quoteAddr || quoteAddr === ethers.ZeroAddress) continue;

            const balBase = await c.balanceOf(wallet.address);
            if (balBase > BigInt(0)) {
                action = { type: 'SELL', baseToken: addr, baseSymbol: sym, quoteToken: quoteAddr };
                break;
            }
            // Check Quote Balance for Buying
            const q = new ethers.Contract(quoteAddr, ERC20_ABI, wallet);
            const balQuote = await q.balanceOf(wallet.address);
            if (balQuote > BigInt(0)) {
                action = { type: 'BUY', baseToken: addr, baseSymbol: sym, quoteToken: quoteAddr };
                break;
            }
        } catch (e) { }
    }

    // Fallback: Create Token
    if (!action) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No valid Base/Quote balance. Creating new stablecoin...${COLORS.reset}`);
        await createRandomStableForWallet(wallet, proxy, workerId, walletIndex, silent);
        await new Promise(r => setTimeout(r, 2000));
        return { success: false, reason: 'created_token_retry_next_loop' };
    }

    const amount = (Math.random() * 10000 + 20000).toFixed(2);
    return await addLiquidityForWallet(wallet, proxy, action, amount, workerId, walletIndex, silent);
}

export async function addLiquidityForWallet(wallet, proxy, action, amount, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    let dexAddress = SYSTEM_CONTRACTS.STABLECOIN_DEX;

    if (!dexAddress) {
        if (!silent) console.log(`${COLORS.fg.red}STABLECOIN_DEX address missing.${COLORS.reset}`);
        return { success: false, reason: 'dex_address_missing' };
    }
    try { dexAddress = ethers.getAddress(dexAddress); } catch (e) { return { success: false, reason: 'invalid_dex_address' }; }

    const isBid = action.type === 'BUY';
    const tokenSymbol = action.baseSymbol;
    let tick = 0;

    const dex = new ethers.Contract(dexAddress, DEX_ABI, wallet);

    // Optimized: Parallel Data Fetching
    let book;
    try {
        const pairKey = await dex.pairKey(action.baseToken, action.quoteToken);
        book = await dex.books(pairKey);

        if (book.base !== ethers.ZeroAddress) {
            if (isBid) {
                tick = book.bestBidTick > -32000 && book.bestBidTick != 0 ? Number(book.bestBidTick) : -500;
            } else {
                tick = book.bestAskTick < 32000 && book.bestAskTick != 0 ? Number(book.bestAskTick) : 500;
            }
        } else {
            tick = isBid ? -500 : 500;
        }
    } catch (e) {
        tick = isBid ? -500 : 500;
    }

    const desc = isBid ? `Buy ${amount} ${tokenSymbol} (Bid)` : `Sell ${amount} ${tokenSymbol} (Ask)`;

    try {
        const payTokenAddr = isBid ? action.quoteToken : action.baseToken;
        const payToken = new ethers.Contract(payTokenAddr, ERC20_ABI, wallet);

        const [decimals, allowance, balance] = await Promise.all([
            payToken.decimals().catch(() => 18),
            payToken.allowance(wallet.address, dexAddress).catch(() => 0n),
            payToken.balanceOf(wallet.address).catch(() => 0n)
        ]);

        const amountWei = ethers.parseUnits(amount.toString(), decimals);

        if (balance < amountWei) {
            logWalletAction(workerId, walletIndex, wallet.address, 'AddLiquidity', 'failed', 'Insufficient Balance', silent);
            return { success: false, reason: 'insufficient_balance' };
        }

        // --- ATOMIC BATCHING (Type 0x76) ---
        if (!silent) console.log(`${COLORS.fg.yellow}Add Liquidity (Atomic Batch): ${desc} @ Tick ${tick}${COLORS.reset}`);

        const service = new ConcurrentService(wallet.privateKey, proxy);
        const calls = [];

        // 1. Create Pair if necessary
        if (book && book.base === ethers.ZeroAddress) {
            calls.push({
                to: dexAddress,
                data: dex.interface.encodeFunctionData('createPair', [action.baseToken]),
                value: 0n
            });
        }

        // 2. Approve if necessary
        if (allowance < amountWei) {
            calls.push({
                to: payTokenAddr,
                data: payToken.interface.encodeFunctionData('approve', [dexAddress, ethers.MaxUint256]),
                value: 0n
            });
        }

        // 3. Place Order
        calls.push({
            to: dexAddress,
            data: dex.interface.encodeFunctionData('place', [action.baseToken, amountWei, isBid, tick]),
            value: 0n
        });

        const txHash = await service.sendAtomicBatch(calls, Date.now(), CONFIG.TOKENS.PathUSD, { gas: 3000000n });
        if (!silent) console.log(`${COLORS.dim}Atomic Tx: ${CONFIG.EXPLORER_URL}/tx/${txHash}${COLORS.reset}`);

        const receipt = await service.publicClient.waitForTransactionReceipt({ hash: txHash });

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'AddLiquidity', 'success', desc, silent, duration);
        if (!silent) console.log(`${COLORS.fg.green}✓ Order Placed! Block: ${receipt.blockNumber}${COLORS.reset}`);

        return { success: true, txHash, block: Number(receipt.blockNumber) };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        let msg = error.message.substring(0, 50);
        logWalletAction(workerId, walletIndex, wallet.address, 'AddLiquidity', 'failed', msg, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Add Liquidity failed: ${msg}${COLORS.reset}`);
        return { success: false, reason: msg };
    }
}
