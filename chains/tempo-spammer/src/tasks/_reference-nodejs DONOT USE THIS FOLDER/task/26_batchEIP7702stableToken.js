import { ethers } from 'ethers';
import fs from 'fs';
import path from 'path';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet, loadCreatedTokens } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, getRandomInt, getGasWithMultiplier } from '../utils/helpers.js';
import { mintRandomTokenForWallet } from './7_mintStable.js';

// Batch Contract ABI
const BATCH_ABI = [
    "function batchTransfer(address token, address[] recipients, uint256[] amounts)"
];

const ERC20_ABI = [
    "function approve(address spender, uint256 amount) returns (bool)",
    "function allowance(address owner, address spender) view returns (uint256)",
    "function transfer(address to, uint256 amount) returns (bool)",
    "function balanceOf(address owner) view returns (uint256)"
];

const BATCH_CONTRACT_FILE = path.join(process.cwd(), 'data', 'batch_contract.json');

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

async function findStableTokenWithBalance(wallet, silent) {
    const created = loadCreatedTokens();
    const myCreated = created[wallet.address] || created[wallet.address.toLowerCase()] || [];
    const candidates = myCreated.map(t => ({ sym: t.symbol, addr: t.token }));

    if (candidates.length === 0) return null;

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

function shortHash(hash) {
    return `${hash.substring(0, 6)}...${hash.substring(hash.length - 4)}`;
}

// 4. Batch Transfers (STABLE Only)
export async function batchTransferStableForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const count = getRandomInt(2, 3);
    if (!silent) console.log(`${COLORS.fg.cyan}📦 BATCH: ${count} Transfers [STABLE]${COLORS.reset}`);

    // Find token of specific type
    let token = await findStableTokenWithBalance(wallet, silent);

    if (!token) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No Stable token balance. Attempting fallback (Mint)...${COLORS.reset}`);

        // Pass 'silent' instead of 'true' to see logs if we are in debug mode (silent=false)
        const mintResult = await mintRandomTokenForWallet(wallet, proxy, workerId, walletIndex, silent);

        if (mintResult.success && mintResult.tokenAddress) {
            if (!silent) console.log(`${COLORS.fg.green}✓ Using freshly minted token: ${mintResult.token} (${mintResult.tokenAddress})${COLORS.reset}`);
            token = { sym: mintResult.token, addr: mintResult.tokenAddress };
        }

        if (!token) {
            // Re-try finding if above failed or wasn't set (should be set if success)
            token = await findStableTokenWithBalance(wallet, silent);
        }
    }

    if (!token) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchTxStable', 'failed', 'No stable token balance after mint attempt', silent, duration);
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No Stable token balance for transfer.${COLORS.reset}`);
        return { success: false, reason: 'no_balance' };
    }

    const tokenAddr = token.addr;
    const tokenSymbol = token.sym;
    const amount = ethers.parseUnits("0.01", 6);

    // Set fee token preference to PathUSD before transfers
    // Prevents using the stable token itself as fee token (which may lack Fee AMM liquidity)
    if (!silent) console.log(`${COLORS.dim}Setting fee token to PathUSD...${COLORS.reset}`);
    const FEE_MANAGER_ABI = ["function setUserToken(address token)"];
    const feeManager = new ethers.Contract(SYSTEM_CONTRACTS.FEE_MANAGER, FEE_MANAGER_ABI, wallet);
    try {
        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
        await (await feeManager.setUserToken(CONFIG.TOKENS.PathUSD, { ...gasOverrides, gasLimit: 400000 })).wait();
        if (!silent) console.log(`${COLORS.fg.green}✓ Fee token set to PathUSD${COLORS.reset}`);
    } catch (e) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ setUserToken failed: ${e.message}${COLORS.reset}`);
    }

    const recipients = Array.from({ length: count }, () => ethers.Wallet.createRandom().address);
    const batchContract = getBatchContract(wallet);

    try {
        if (batchContract) {
            const tokenC = new ethers.Contract(tokenAddr, ERC20_ABI, wallet);
            const allowance = await tokenC.allowance(wallet.address, await batchContract.getAddress());
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);

            if (allowance < (amount * BigInt(count))) {
                await (await tokenC.approve(await batchContract.getAddress(), ethers.MaxUint256, { ...gasOverrides })).wait();
            }

            const amounts = Array(count).fill(amount);
            const tx = await batchContract.batchTransfer(tokenAddr, recipients, amounts, {
                ...gasOverrides,
                gasLimit: 3000000
            });
            if (!silent) console.log(`${COLORS.dim}Batch Tx (${tokenSymbol}): ${CONFIG.EXPLORER_URL}/tx/${tx.hash}${COLORS.reset}`);
            const receipt = await tx.wait();

            const duration = (Date.now() - startTime) / 1000;
            logWalletAction(workerId, walletIndex, wallet.address, 'BatchTxStable', 'success', `${count}x${tokenSymbol} (batch)`, silent, duration);
            if (!silent) console.log(`${COLORS.fg.green}✓ ${count} transfers completed (batch)!${COLORS.reset}`);

            return { success: true, mode: 'batch_contract', txHash: tx.hash, count };
        }
    } catch (e) {
        if (!silent) console.log(`${COLORS.dim}Batch failed (${e.message.substring(0, 30)}). Sequential fallback...${COLORS.reset}`);
    }

    // Sequential fallback
    const tokenC = new ethers.Contract(tokenAddr, ERC20_ABI, wallet);
    let successCount = 0;
    for (let i = 0; i < count; i++) {
        try {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            const tx = await tokenC.transfer(recipients[i], amount, {
                ...gasOverrides,
                gasLimit: 500000
            });
            if (!silent) console.log(`${COLORS.dim}Tx ${i + 1}: ${shortHash(tx.hash)}${COLORS.reset}`);
            await tx.wait();
            successCount++;
            var lastTxHash = tx.hash;
        } catch (e) {
            if (!silent) console.log(`${COLORS.fg.red}Tx ${i + 1} failed${COLORS.reset}`);
        }
    }

    const duration = (Date.now() - startTime) / 1000;
    if (successCount > 0) {
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchTxStable', 'success', `${successCount}/${count}x${tokenSymbol} (seq)`, silent, duration);
        if (!silent) console.log(`${COLORS.fg.green}✓ ${successCount}/${count} transfers completed (sequential)!${COLORS.reset}`);
    } else {
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchTxStable', 'failed', 'All transfers failed', silent, duration);
    }

    return { success: successCount > 0, mode: 'sequential', count: successCount, txHash: lastTxHash };
}
