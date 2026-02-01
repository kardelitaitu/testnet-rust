import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { countdown, getRandomInt, getGasWithMultiplier } from '../utils/helpers.js';
import { addRandomLiquidityForWallet } from './6_addLiquidity.js';

const DEX_ABI = [
    "function withdraw(address token, uint128 amount) external",
    "function balanceOf(address user, address token) view returns (uint128)"
];

const ERC20_ABI = [
    "function decimals() view returns (uint8)",
    "function symbol() view returns (string)",
    "function quoteToken() view returns (address)"
];

export async function removeRandomLiquidityForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();

    const dexAddress = SYSTEM_CONTRACTS.STABLECOIN_DEX;
    if (!dexAddress) {
        if (!silent) console.log(`${COLORS.fg.red}STABLECOIN_DEX address missing.${COLORS.reset}`);
        return { success: false, reason: 'dex_address_missing' };
    }

    const dex = new ethers.Contract(dexAddress, DEX_ABI, wallet);

    // Optimized: Parallel Scanning
    if (!silent) console.log(`${COLORS.fg.yellow}Remove Liquidity: Parallel scanning DEX balances...${COLORS.reset}`);

    const tokenEntries = Object.entries(CONFIG.TOKENS);

    const balanceChecks = await Promise.all(tokenEntries.map(async ([sym, addr]) => {
        try {
            const tokenContract = new ethers.Contract(addr, ERC20_ABI, wallet);

            // Parallel fetch metadata + balance 1
            const [quoteAddr, dexBalance] = await Promise.all([
                tokenContract.quoteToken().catch(() => null),
                dex.balanceOf(wallet.address, addr).catch(() => 0n)
            ]);

            if (quoteAddr && quoteAddr !== ethers.ZeroAddress && dexBalance > 0n) {
                const [decimals, symbol] = await Promise.all([
                    tokenContract.decimals().catch(() => 18),
                    tokenContract.symbol().catch(() => sym)
                ]);
                return { sym, addr, dexBalance, decimals, symbol };
            }
        } catch (e) { }
        return null;
    }));

    const fundedItems = balanceChecks.filter(it => it !== null);

    if (fundedItems.length > 0) {
        // Pick one to withdraw
        const it = fundedItems[Math.floor(Math.random() * fundedItems.length)];
        const formattedBalance = ethers.formatUnits(it.dexBalance, it.decimals);

        if (!silent) console.log(`${COLORS.fg.cyan}Found DEX balance: ${formattedBalance} ${it.symbol}${COLORS.reset}`);
        if (!silent) console.log(`${COLORS.dim}Withdrawing...${COLORS.reset}`);

        try {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, 3.0, wallet);
            const txWithdraw = await dex.withdraw(it.addr, it.dexBalance, { gasLimit: 500000, ...gasOverrides });

            if (!silent) console.log(`${COLORS.dim}Tx: ${CONFIG.EXPLORER_URL}/tx/${txWithdraw.hash}${COLORS.reset}`);
            const receipt = await txWithdraw.wait();

            const duration = (Date.now() - startTime) / 1000;
            logWalletAction(workerId, walletIndex, wallet.address, 'RemoveLiq', 'success', `Withdrew ${formattedBalance} ${it.symbol}`, silent, duration);
            if (!silent) console.log(`${COLORS.fg.green}✓ Liquidity Removed! Block: ${receipt.blockNumber}${COLORS.reset}`);

            return { success: true, txHash: txWithdraw.hash, block: receipt.blockNumber, amount: formattedBalance, token: it.symbol };
        } catch (withdrawError) {
            const duration = (Date.now() - startTime) / 1000;
            logWalletAction(workerId, walletIndex, wallet.address, 'RemoveLiq', 'failed', withdrawError.message.substring(0, 50), silent, duration);
            return { success: false, reason: withdrawError.message };
        }
    }

    // FALLBACK: No DEX balance found
    if (!silent) console.log(`${COLORS.fg.yellow}⚠ No DEX balance found. Adding liquidity then withdrawing...${COLORS.reset}`);

    const addResult = await addRandomLiquidityForWallet(wallet, proxy, workerId, walletIndex, silent);

    if (!addResult || !addResult.success) {
        return addResult; // Failed to add
    }

    // Optimization: After adding liquidity, we know exactly what we added.
    // Instead of re-scanning everything and sleeping 5s, we can just try to withdraw what was added.
    // However, for safety and simplicity (as addRandomLiquidity might have multiple branches), 
    // we'll do a quick specific check.

    if (!silent) console.log(`${COLORS.dim}Checking for newly added liquidity...${COLORS.reset}`);

    // Give it a tiny moment or just a single re-fetch of all balances
    // Since we parallelized the scanning above, let's just use it again without the 5s sleep.

    const secondScan = await Promise.all(tokenEntries.map(async ([sym, addr]) => {
        try {
            const dexBalance = await dex.balanceOf(wallet.address, addr);
            if (dexBalance > 0n) {
                const tokenContract = new ethers.Contract(addr, ERC20_ABI, wallet);
                const decimals = await tokenContract.decimals();
                const symbol = await tokenContract.symbol();
                return { sym, addr, dexBalance, decimals, symbol };
            }
        } catch (e) { }
        return null;
    }));

    const foundNew = secondScan.filter(it => it !== null);

    if (foundNew.length > 0) {
        const it = foundNew[0];
        const decimals = it.decimals;

        // Withdraw random 1000-2000
        const withdrawAmount = (Math.random() * 1000 + 1000).toFixed(2);
        const withdrawWei = ethers.parseUnits(withdrawAmount, decimals);

        // Don't withdraw more than we have
        const actualWithdraw = withdrawWei > it.dexBalance ? it.dexBalance : withdrawWei;
        const formattedActual = ethers.formatUnits(actualWithdraw, decimals);

        if (!silent) console.log(`${COLORS.fg.cyan}Withdrawing ${formattedActual} ${it.symbol} from DEX...${COLORS.reset}`);

        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
        const txWithdraw = await dex.withdraw(it.addr, actualWithdraw, { gasLimit: 500000, ...gasOverrides });

        if (!silent) console.log(`${COLORS.dim}Tx: ${CONFIG.EXPLORER_URL}/tx/${txWithdraw.hash}${COLORS.reset}`);
        const receipt = await txWithdraw.wait();

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'RemoveLiq', 'success', `Added & withdrew ${formattedActual} ${it.symbol}`, silent, duration);
        if (!silent) console.log(`${COLORS.fg.green}✓ Liquidity Removed! Block: ${receipt.blockNumber}${COLORS.reset}`);

        return { success: true, txHash: txWithdraw.hash, block: receipt.blockNumber, amount: formattedActual, token: it.symbol };
    }

    if (!silent) console.log(`${COLORS.fg.yellow}ℹ No withdrawable balance yet. Order placed successfully.${COLORS.reset}`);
    const duration = (Date.now() - startTime) / 1000;
    logWalletAction(workerId, walletIndex, wallet.address, 'RemoveLiq', 'success', 'Added liquidity (order placed)', silent, duration);
    return addResult;
}

export async function runRemoveLiquidityMenu() {
    console.log(`\n  ${COLORS.fg.magenta}💧  REMOVE LIQUIDITY MODULE${COLORS.reset}\n`);
    console.log(`${COLORS.fg.yellow}Strategy: Withdraw proceeds from DEX balance to wallet${COLORS.reset}\n`);

    const privateKeys = getPrivateKeys();
    if (privateKeys.length === 0) {
        console.log(`${COLORS.fg.red}No private keys found in pv.txt${COLORS.reset}`);
        return;
    }

    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);

        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}${COLORS.reset}`);
        console.log(`${COLORS.fg.cyan}Address: ${wallet.address}${COLORS.reset}`);

        await removeRandomLiquidityForWallet(wallet, proxy, 1, i);

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(3, 6), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ All operations completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}
