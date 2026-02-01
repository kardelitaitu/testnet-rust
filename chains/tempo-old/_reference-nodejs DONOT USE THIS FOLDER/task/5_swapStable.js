import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier } from '../utils/helpers.js';
import { claimRandomFaucetForWallet } from './2_claimFaucet.js';

const STABLECOIN_DEX_ABI = [
    "function quoteSwapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn) view returns (uint128 amountOut)",
    "function swapExactAmountIn(address tokenIn, address tokenOut, uint128 amountIn, uint128 minAmountOut) returns (uint128 amountOut)",
    "function place(address token, uint128 amount, bool isBid, int16 tick) returns (uint128 orderId)",
    "function withdraw(address token, uint128 amount)"
];

const ERC20_ABI = [
    "function balanceOf(address owner) view returns (uint256)",
    "function allowance(address owner, address spender) view returns (uint256)",
    "function approve(address spender, uint256 amount) returns (bool)",
    "function symbol() view returns (string)",
    "function decimals() view returns (uint8)"
];

export async function swapRandomStableForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const tokenEntries = Object.entries(CONFIG.TOKENS);
    /* 
       Note: We expect CONFIG.TOKENS to look like:
       { "PathUSD": "0x...", "AlphaUSD": "0x..." }
    */
    if (tokenEntries.length < 2) {
        console.log(`${COLORS.fg.red}Not enough tokens configured to swap.${COLORS.reset}`);
        return;
    }

    let tokenInSymbol, tokenInAddress;

    // 1. Check balances to find a token we have (PARALLEL)
    // Shuffle check order
    const shuffled = [...tokenEntries].sort(() => 0.5 - Math.random());

    const initialResults = await Promise.all(shuffled.map(async ([sym, addr]) => {
        try {
            const c = new ethers.Contract(addr, ERC20_ABI, wallet);
            const bal = await c.balanceOf(wallet.address);
            return { sym, addr, bal };
        } catch (e) {
            return { sym, addr, bal: BigInt(0) };
        }
    }));

    const foundInitial = initialResults.find(r => r.bal > BigInt(0));
    if (foundInitial) {
        tokenInSymbol = foundInitial.sym;
        tokenInAddress = foundInitial.addr;
    }

    // 2. If no balance, CREATE STABLECOIN (User Request)
    if (!tokenInSymbol) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No stablecoin balance found. Creating new stablecoin...${COLORS.reset}`);
        await createRandomStableForWallet(wallet, proxy, workerId, walletIndex, silent);

        // Retry checks IN PARALLEL
        const results = await Promise.all(shuffled.map(async ([sym, addr]) => {
            try {
                const c = new ethers.Contract(addr, ERC20_ABI, wallet);
                const bal = await c.balanceOf(wallet.address);
                return { sym, addr, bal };
            } catch (e) {
                return { sym, addr, bal: BigInt(0) };
            }
        }));

        const found = results.find(r => r.bal > BigInt(0));
        if (found) {
            tokenInSymbol = found.sym;
            tokenInAddress = found.addr;
        }
    }

    if (!tokenInSymbol) {
        if (!silent) console.log(`${COLORS.fg.red}✗ Still no tokens after creation attempt. Skipping swap.${COLORS.reset}`);
        return { success: false, reason: 'no_tokens_available' };
    }

    // 3. Pick Token Out (different from In)
    const availableOut = tokenEntries.filter(([s, a]) => s !== tokenInSymbol);
    if (availableOut.length === 0) return; // Should not happen if length >= 2

    const [tokenOutSymbol, tokenOutAddress] = availableOut[Math.floor(Math.random() * availableOut.length)];

    // 4. Random Amount (1 - 5)
    // We can also check balance and do %? Random 1-5 is fine as per spec.
    const amount = (Math.random() * 4 + 1).toFixed(2);

    return await swapStableForWallet(wallet, proxy, tokenInAddress, tokenInSymbol, tokenOutAddress, tokenOutSymbol, amount, workerId, walletIndex, silent);
}

export async function swapStableForWallet(wallet, proxy, tokenIn, tokenInSymbol, tokenOut, tokenOutSymbol, amountIn, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    if (!silent) console.log(`${COLORS.fg.yellow}Swapping ${amountIn} ${tokenInSymbol} → ${tokenOutSymbol}...${COLORS.reset}`);
    let dexAddress = SYSTEM_CONTRACTS.STABLECOIN_DEX;

    // DEBUG: Inspect Dex Address
    if (!dexAddress) {
        if (!silent) console.log(`${COLORS.fg.red}STABLECOIN_DEX address missing config.${COLORS.reset}`);
        return { success: false, reason: 'dex_address_missing' };
    }

    try {
        dexAddress = ethers.getAddress(dexAddress);
    } catch (e) {
        console.log(`${COLORS.fg.red}Invalid DEX Address format: '${dexAddress}' (${e.message})${COLORS.reset}`);
        return;
    }

    const shortIn = tokenInSymbol;
    const shortOut = tokenOutSymbol;

    if (!silent) console.log(`${COLORS.fg.yellow}Swap ${amountIn} ${shortIn} -> ${shortOut}${COLORS.reset}`);

    try {
        const dex = new ethers.Contract(dexAddress, STABLECOIN_DEX_ABI, wallet);
        const tokenInContract = new ethers.Contract(tokenIn, ERC20_ABI, wallet);

        let decimals = 18;
        try {
            decimals = await tokenInContract.decimals();
        } catch (e) { decimals = 18; } // Default

        if (!silent) console.log(`DEBUG: Token ${shortIn} decimals = ${decimals}`);

        const amountWei = ethers.parseUnits(amountIn.toString(), decimals);

        const bal = await tokenInContract.balanceOf(wallet.address);
        if (bal < amountWei) {
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'Swap', 'skipped', `Insufficient ${shortIn}`, silent, duration);
            if (!silent) console.log(`${COLORS.fg.red}✗ Insufficient ${shortIn}: ${ethers.formatUnits(bal, decimals)} < ${amountIn}${COLORS.reset}`);
            // Fallback: Swap ALL balance if close? OR just skip. Skipped for now.
            return { success: false, reason: 'insufficient_balance' };
        }

        // Approve
        const allowance = await tokenInContract.allowance(wallet.address, dexAddress);
        // Use 3x gas multiplier for speed
        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);

        if (allowance < amountWei) {
            if (!silent) console.log(`${COLORS.dim}Approving DEX...${COLORS.reset}`);
            const txApp = await tokenInContract.approve(dexAddress, ethers.MaxUint256, { ...gasOverrides, gasLimit: CONFIG.GAS_LIMIT || 1000000 });
            await txApp.wait();
        }

        // Check Liquidity
        let hasLiquidity = false;
        let expectedOut = BigInt(0);
        try {
            expectedOut = await dex.quoteSwapExactAmountIn(tokenIn, tokenOut, amountWei);
            if (expectedOut > BigInt(0)) hasLiquidity = true;
        } catch (e) {
            // quote reverts if no path/liquidity
            hasLiquidity = false;
        }

        if (hasLiquidity) {
            if (!silent) console.log(`${COLORS.fg.cyan}Liquidity found. Swapping...${COLORS.reset}`);

            // 99.9% Success Buffer Strategy (Doubled):
            // 1. Slippage: 20% (0.80) to handle extreme volatility/low liquidity.
            // 2. Gas Limit: 1,000,000 to ensure execution.

            const minOut = (expectedOut * BigInt(80)) / BigInt(100); // 20% slippage
            if (!silent) console.log(`${COLORS.dim}Expected: ${ethers.formatUnits(expectedOut, decimals)}, Min: ${ethers.formatUnits(minOut, decimals)} (20% buf)${COLORS.reset}`);

            const tx = await dex.swapExactAmountIn(tokenIn, tokenOut, amountWei, minOut, {
                gasLimit: CONFIG.GAS_LIMIT || 1000000,
                ...gasOverrides
            });
            if (!silent) console.log(`${COLORS.dim}Tx Sent: ${CONFIG.EXPLORER_URL}/tx/${tx.hash}${COLORS.reset}`);
            const receipt = await tx.wait();
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'Swap', 'success', `${amountIn} ${shortIn} -> ${shortOut}`, silent, duration);
            if (!silent) console.log(`${COLORS.fg.green}✓ Swap successful! Block: ${receipt.blockNumber}${COLORS.reset}`);
            return { success: true, txHash: tx.hash, block: receipt.blockNumber, tokenIn: tokenInSymbol, tokenOut: tokenOutSymbol, amountIn };
        } else {
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) console.log(`${COLORS.fg.yellow}⚠ No liquidity. Placing LIMIT ORDER...${COLORS.reset}`);
            // TODO: Implement Limit Order placement if critical.
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'Swap', 'skipped', 'No liquidity', silent, duration);
            if (!silent) console.log(`${COLORS.dim}Limit order logic skipped.${COLORS.reset}`);
            return { success: false, reason: 'no_liquidity' };
        }

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'Swap', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Swap failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function runSwapStableMenu() {
    console.log(`\n  ${COLORS.fg.magenta}🔄  SWAP MODULE${COLORS.reset}\n`);
    console.log(`${COLORS.fg.yellow}Running random swaps for all wallets...${COLORS.reset}\n`);

    const privateKeys = getPrivateKeys();
    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);
        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}${COLORS.reset}`);
        console.log(`${COLORS.fg.cyan}Address: ${wallet.address}${COLORS.reset}`);

        await swapRandomStableForWallet(wallet, proxy, 1, i);

        if (i < privateKeys.length - 1) await sleep(2000);
    }
    console.log(`\n${COLORS.fg.green}✓ Done.${COLORS.reset}\n`);
    await countdown(5, 'Returning');
}
