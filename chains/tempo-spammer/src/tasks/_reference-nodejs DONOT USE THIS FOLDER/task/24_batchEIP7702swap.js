import { ethers } from 'ethers';
import fs from 'fs';
import path from 'path';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, getRandomInt, getGasWithMultiplier } from '../utils/helpers.js';
import { claimRandomFaucetForWallet } from './2_claimFaucet.js';

const ERC20_ABI = [
    "function approve(address spender, uint256 amount) returns (bool)",
    "function allowance(address owner, address spender) view returns (uint256)",
    "function transfer(address to, uint256 amount) returns (bool)",
    "function balanceOf(address owner) view returns (uint256)"
];

const STABLECOIN_DEX_ABI = [
    "function quoteSwapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn) view returns (uint128 amountOut)",
    "function swapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn, uint128 minAmountOut) returns (uint128 amountOut)"
];

const SYSTEM_TOKENS = ['PathUSD', 'AlphaUSD', 'BetaUSD', 'ThetaUSD'];

async function findSystemTokenWithBalance(wallet, silent) {
    const candidates = SYSTEM_TOKENS
        .filter(sym => CONFIG.TOKENS[sym])
        .map(sym => ({ sym, addr: CONFIG.TOKENS[sym] }));

    const shuffled = candidates.sort(() => 0.5 - Math.random());
    for (const c of shuffled) {
        try {
            const contract = new ethers.Contract(c.addr, ERC20_ABI, wallet);
            const bal = await contract.balanceOf(wallet.address);
            if (bal > ethers.parseUnits("0.1", 6)) return c;
        } catch (e) { }
    }
    return null;
}

// 2. Multiple Swaps (SYSTEM Only)
export async function batchMultipleSwapsForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const count = getRandomInt(2, 3);
    if (!silent) console.log(`${COLORS.fg.cyan}📦 BATCH: ${count} Sequential Swaps (System Only)${COLORS.reset}`);

    // 1. Find START token
    let currentToken = await findSystemTokenWithBalance(wallet, silent);
    if (!currentToken) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No System Token balance. Attempting fallback...${COLORS.reset}`);
        await claimRandomFaucetForWallet(wallet, proxy, workerId, walletIndex, true);
        currentToken = await findSystemTokenWithBalance(wallet, silent);
    }

    if (!currentToken) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchMultiSwap', 'failed', 'No system token balance', silent, duration);
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No System Token balance for multi-swap.${COLORS.reset}`);
        return { success: false, reason: 'no_balance' };
    }

    let currentTokenSymbol = currentToken.sym;
    let currentTokenAddr = currentToken.addr;

    let successCount = 0;
    const amount = ethers.parseUnits("0.5", 6);
    const dexAddress = SYSTEM_CONTRACTS.STABLECOIN_DEX;
    const dex = new ethers.Contract(dexAddress, STABLECOIN_DEX_ABI, wallet);

    for (let i = 0; i < count; i++) {
        // Next Token (System Only) -> Iterate for Liquidity
        const others = SYSTEM_TOKENS.filter(s => s !== currentTokenSymbol && CONFIG.TOKENS[s]);
        if (others.length === 0) break;

        let nextTokenSymbol, nextTokenAddr, minOut;

        // Shuffle candidates
        others.sort(() => 0.5 - Math.random());

        for (const candidateSym of others) {
            const candidateAddr = CONFIG.TOKENS[candidateSym];
            try {
                const quote = await dex.quoteSwapExactAmountIn(currentTokenAddr, candidateAddr, amount);
                if (quote > 0n) {
                    nextTokenSymbol = candidateSym;
                    nextTokenAddr = candidateAddr;
                    minOut = (quote * 90n) / 100n; // 10% slippage
                    break; // Found valid path
                }
            } catch (e) { }
        }

        if (!nextTokenSymbol) {
            if (!silent) console.log(`${COLORS.fg.red}  ✗ No liquidity path from ${currentTokenSymbol}${COLORS.reset}`);
            break;
        }

        try {
            if (!silent) console.log(`${COLORS.dim}${i + 1}/${count} ${currentTokenSymbol} -> ${nextTokenSymbol}...${COLORS.reset}`);

            const tokenIn = new ethers.Contract(currentTokenAddr, ERC20_ABI, wallet);
            const allowance = await tokenIn.allowance(wallet.address, dexAddress);
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            if (allowance < amount) {
                const appTx = await tokenIn.approve(dexAddress, ethers.MaxUint256, { ...gasOverrides });
                await appTx.wait();
            }

            const tx = await dex.swapExactAmountIn(currentTokenAddr, nextTokenAddr, amount, minOut, {
                gasLimit: 500000,
                ...gasOverrides
            });
            await tx.wait();
            successCount++;
            var lastTxHash = tx.hash;

            currentTokenSymbol = nextTokenSymbol;
            currentTokenAddr = nextTokenAddr;

            if (i < count - 1) await sleep(1000);

        } catch (e) {
            if (!silent) console.log(`${COLORS.fg.red}  ✗ Failed: ${e.message.substring(0, 50)}...${COLORS.reset}`);
            break;
        }
    }

    const duration = (Date.now() - startTime) / 1000;

    if (successCount > 0) {
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchMultiSwap', 'success', `${successCount}/${count} swaps`, silent, duration);
        if (!silent) console.log(`${COLORS.fg.green}✓ Completed ${successCount}/${count} swaps!${COLORS.reset}`);
    } else {
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchMultiSwap', 'failed', 'No swaps completed', silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ No swaps completed${COLORS.reset}`);
    }

    return { success: successCount > 0, count: successCount, total: count, txHash: lastTxHash };
}

// Menu
export async function runBatchSwapMenu() {
    const privateKeys = getPrivateKeys();
    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);
        await batchMultipleSwapsForWallet(wallet, proxy, 1, i, false);
    }
}