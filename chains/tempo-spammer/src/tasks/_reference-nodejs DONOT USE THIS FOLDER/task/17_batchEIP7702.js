import { ethers } from 'ethers';
import fs from 'fs';
import path from 'path';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';
import { claimRandomFaucetForWallet } from './2_claimFaucet.js';

// Batch Contract ABI
const BATCH_ABI = [
    "function approveAndSwap(address token, address dex, address tokenOut, uint128 amount, uint128 minOut) returns (uint128)"
];

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

const BATCH_CONTRACT_FILE = path.join(process.cwd(), 'data', 'batch_contract.json');
const SYSTEM_TOKENS = ['PathUSD', 'AlphaUSD', 'BetaUSD', 'ThetaUSD'];

function getBatchContract(wallet) {
    if (fs.existsSync(BATCH_CONTRACT_FILE)) {
        try {
            const data = JSON.parse(fs.readFileSync(BATCH_CONTRACT_FILE, 'utf8'));
            if (data.address) {
                return new ethers.Contract(data.address, BATCH_ABI, wallet);
            }
        } catch (e) { }
    }
    return null;
}

async function findSystemTokenWithBalance(wallet, silent) {
    const candidates = SYSTEM_TOKENS
        .filter(sym => CONFIG.TOKENS[sym])
        .map(sym => ({ sym, addr: CONFIG.TOKENS[sym] }));

    const shuffled = candidates.sort(() => 0.5 - Math.random());
    for (const c of shuffled) {
        try {
            const contract = new ethers.Contract(c.addr, ERC20_ABI, wallet);
            const bal = await contract.balanceOf(wallet.address);
            if (bal > ethers.parseUnits("0.1", 6)) return { ...c, bal };
        } catch (e) { }
    }
    return null;
}

async function ensureBalanceOfType(wallet, proxy, workerId, walletIndex, silent) {
    if (!silent) console.log(`${COLORS.fg.yellow}⚠ No System token balance found. Attempting fallback...${COLORS.reset}`);
    await claimRandomFaucetForWallet(wallet, proxy, workerId, walletIndex, true);
    await sleep(2000);
}

// 1. Approve + Swap (SYSTEM Only)
export async function batchApproveSwapForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const batchContract = getBatchContract(wallet);
    const useBatch = !!batchContract;

    // Find SYSTEM token
    let tokenIn = await findSystemTokenWithBalance(wallet, silent);
    if (!tokenIn) {
        await ensureBalanceOfType(wallet, proxy, workerId, walletIndex, silent);
        tokenIn = await findSystemTokenWithBalance(wallet, silent);
    }

    if (!tokenIn) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchApprove+Swap', 'failed', 'No system token balance', silent, duration);
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No System Token balance. Skipping.${COLORS.reset}`);
        return { success: false, reason: 'no_balance_system' };
    }

    const tokenInSymbol = tokenIn.sym;
    const tokenInAddr = tokenIn.addr;
    const balance = tokenIn.bal;

    let amountVal = getRandomInt(100, 1000); // Reduced to 100-1000 for safety
    let amount = ethers.parseUnits(amountVal.toString(), 6);

    // Cap amount to balance
    if (balance < amount) {
        amount = balance;
        // Leave dust? (Optional, but full swap is fine)
        if (!silent) console.log(`${COLORS.dim}Capping swap amount of ${tokenInSymbol} to balance: ${ethers.formatUnits(amount, 6)}${COLORS.reset}`);
    }
    const dexAddress = SYSTEM_CONTRACTS.STABLECOIN_DEX;
    const dex = new ethers.Contract(dexAddress, STABLECOIN_DEX_ABI, wallet);

    // Iterate potential outputs for liquidity
    const others = SYSTEM_TOKENS.filter(s => s !== tokenInSymbol && CONFIG.TOKENS[s]);
    if (others.length === 0) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchApprove+Swap', 'failed', 'No target tokens', silent, duration);
        return { success: false, reason: 'no_target' };
    }

    let tokenOutSymbol, tokenOutAddr, minOut;

    // Shuffle to randomize
    others.sort(() => 0.5 - Math.random());

    if (!silent) console.log(`${COLORS.dim}Checking potential pairs for ${tokenInSymbol}...${COLORS.reset}`);

    for (const candidateSym of others) {
        const candidateAddr = CONFIG.TOKENS[candidateSym];
        try {
            if (!silent) process.stdout.write(`${COLORS.dim}  -> ${candidateSym}: ${COLORS.reset}`);

            const quote = await dex.quoteSwapExactAmountIn(tokenInAddr, candidateAddr, amount);

            if (!silent) console.log(`${COLORS.fg.green}${ethers.formatUnits(quote, 6)} out ✓${COLORS.reset}`);

            if (quote > 0n) {
                tokenOutSymbol = candidateSym;
                tokenOutAddr = candidateAddr;
                minOut = (quote * 90n) / 100n; // 10% slippage
                break;
            }
        } catch (e) {
            const msg = e.reason || e.message || "Unknown";
            if (!silent) console.log(`${COLORS.dim}${msg.substring(0, 60)}${COLORS.reset}`);
        }
    }

    if (!tokenOutSymbol) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchApprove+Swap', 'failed', 'No liquidity', silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ No liquidity found for ${tokenInSymbol} -> [${others.join(', ')}]${COLORS.reset}`);
        return { success: false, reason: 'no_liquidity' };
    }

    const tokenContract = new ethers.Contract(tokenInAddr, ERC20_ABI, wallet);

    if (!silent) console.log(`${COLORS.fg.cyan}📦 BATCH: Approve + Swap (${tokenInSymbol} -> ${tokenOutSymbol})${COLORS.reset}`);

    try {
        let batchSuccess = false;
        if (useBatch) {
            try {
                const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
                // Ensure approval using retry
                await sendTxWithRetry(wallet, async () => {
                    return tokenContract.approve(await batchContract.getAddress(), ethers.MaxUint256, { ...gasOverrides });
                });

                // Attempt Batch Swap
                const result = await sendTxWithRetry(wallet, async () => {
                    return batchContract.approveAndSwap(tokenInAddr, dexAddress, tokenOutAddr, amount, minOut, { ...gasOverrides, gasLimit: 3000000 });
                });
                const receipt = result.receipt;
                const txHash = result.hash;

                if (!silent) console.log(`${COLORS.dim}Tx: ${CONFIG.EXPLORER_URL}/tx/${txHash}${COLORS.reset}`);

                if (receipt.status === 1) {
                    const duration = (Date.now() - startTime) / 1000;
                    logWalletAction(workerId, walletIndex, wallet.address, 'BatchApprove+Swap', 'success', `${tokenInSymbol}->${tokenOutSymbol} (batch)`, silent, duration);
                    if (!silent) console.log(`${COLORS.fg.green}✓ Batch swap successful!${COLORS.reset}`);
                    return { success: true, mode: 'batch_contract', txHash: txHash, block: receipt.blockNumber };
                }
            } catch (batchError) {
                if (!silent) console.log(`${COLORS.fg.yellow}⚠ Batch contract failed (${batchError.message.substring(0, 50)}...). Falling back to sequential.${COLORS.reset}`);
            }
        }

        // Fallback or Sequential Mode
        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
        if (!silent) console.log(`${COLORS.fg.yellow}1/2 Approve DEX...${COLORS.reset}`);

        await sendTxWithRetry(wallet, async () => {
            return tokenContract.approve(dexAddress, ethers.MaxUint256, { ...gasOverrides });
        });
        if (!silent) console.log(`${COLORS.fg.green}  ✓ Approved${COLORS.reset}`);

        if (!silent) console.log(`${COLORS.fg.yellow}2/2 Swap...${COLORS.reset}`);
        const result = await sendTxWithRetry(wallet, async () => {
            return dex.swapExactAmountIn(tokenInAddr, tokenOutAddr, amount, minOut, {
                gasLimit: 3000000,
                ...gasOverrides
            });
        });

        const receipt = result.receipt;
        const txHash = result.hash;

        if (!silent) console.log(`${COLORS.dim}Tx: ${CONFIG.EXPLORER_URL}/tx/${txHash}${COLORS.reset}`);

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchApprove+Swap', 'success', `${tokenInSymbol}->${tokenOutSymbol} (seq)`, silent, duration);
        if (!silent) console.log(`${COLORS.fg.green}✓ Sequential swap successful!${COLORS.reset}`);

        return { success: true, mode: 'sequential', txHash: txHash, block: receipt.blockNumber };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchApprove+Swap', 'failed', error.message.substring(0, 50), silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Error: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}
