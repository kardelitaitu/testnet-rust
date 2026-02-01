import { ethers } from 'ethers';
import fs from 'fs';
import path from 'path';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';
import { createRandomMemeForWallet } from './21_createMeme.js'; // Back to meme

// Batch Contract ABI
const BATCH_ABI = [
    "function batchTransfer(address token, address[] recipients, uint256[] amounts)"
];

const ERC20_ABI = [
    "function approve(address spender, uint256 amount) returns (bool)",
    "function allowance(address owner, address spender) view returns (uint256)",
    "function transfer(address to, uint256 amount) returns (bool)",
    "function balanceOf(address owner) view returns (uint256)",
    "function mint(address to, uint256 amount) returns (bool)",
    "function hasRole(bytes32 role, address account) view returns (bool)",
    "function grantRole(bytes32 role, address account)"
];

const BATCH_CONTRACT_FILE = path.join(process.cwd(), 'data', 'batch_contract.json');
const ISSUER_ROLE = ethers.id("ISSUER_ROLE");

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

function shortHash(hash) {
    return `${hash.substring(0, 6)}...${hash.substring(hash.length - 4)}`;
}

// 5. Batch Transfers - Creates meme token for transfers
export async function batchTransferMemeForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const count = getRandomInt(2, 3);
    if (!silent) console.log(`${COLORS.fg.cyan}📦 BATCH: ${count} Transfers [MEME]${COLORS.reset}`);

    // Step 1: Create a meme token (Retry 3 times)
    if (!silent) console.log(`${COLORS.fg.yellow}Step 1/3: Creating meme token...${COLORS.reset}`);

    let createResult;
    for (let i = 0; i < 3; i++) {
        createResult = await createRandomMemeForWallet(wallet, proxy, workerId, walletIndex, true); // silent creation
        if (createResult?.success && createResult?.tokenAddress && createResult?.symbol) break;
        if (!silent) console.log(`${COLORS.dim}Creation attempt ${i + 1} failed. Retrying...${COLORS.reset}`);
        await sleep(2000);
    }

    if (!createResult?.success || !createResult?.tokenAddress || !createResult?.symbol) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchTxMeme', 'failed', 'Failed to create token after 3 attempts', silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Failed to create token${COLORS.reset}`);
        return { success: false, reason: 'create_failed' };
    }

    const tokenAddr = createResult.tokenAddress;
    const tokenSymbol = createResult.symbol;

    if (!silent) console.log(`${COLORS.fg.green}✓ Created ${tokenSymbol} at ${tokenAddr}${COLORS.reset}`);

    // Step 2: Mint the newly created meme token
    if (!silent) console.log(`${COLORS.fg.yellow}Step 2/3: Minting ${tokenSymbol}...${COLORS.reset}`);

    const tokenContract = new ethers.Contract(tokenAddr, ERC20_ABI, wallet);

    try {
        // Optimistic Approach: Try to mint directly first
        // Most of the time, creator already has the role.
        try {
            const mintAmount = getRandomInt(1000, 5000);
            const amountWei = ethers.parseUnits(mintAmount.toString(), 6);

            // Estimate gas to check if it will revert due to missing role
            // If this fails, we catch it and do the role grant flow
            await tokenContract.mint.estimateGas(wallet.address, amountWei);

            // If estimate succeeds, proceed to mint
            // If estimate succeeds, proceed to mint
            const gasOverridesMint = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            await sendTxWithRetry(wallet, async () => {
                return tokenContract.mint(wallet.address, amountWei, {
                    gasLimit: 1000000, // Increased limit 500k -> 1M
                    ...gasOverridesMint
                });
            });

            if (!silent) console.log(`${COLORS.fg.green}✓ Minted (Optimistic) ${mintAmount} ${tokenSymbol}${COLORS.reset}`);
        } catch (mintError) {
            // If mint failed, likely due to missing role (or other issue)
            // Fallback to robust Role verify & Grant flow
            if (!silent) console.log(`${COLORS.dim}Optimistic mint failed, verifying roles...${COLORS.reset}`);

            // 1. Verify/Grant Role
            let hasIssuerRole = await tokenContract.hasRole(ISSUER_ROLE, wallet.address);
            if (!hasIssuerRole) {
                if (!silent) console.log(`${COLORS.dim}Granting ISSUER_ROLE...${COLORS.reset}`);
                const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
                await sendTxWithRetry(wallet, async () => {
                    return tokenContract.grantRole(ISSUER_ROLE, wallet.address, {
                        gasLimit: 600000, // Increased 300k -> 600k
                        ...gasOverrides
                    });
                });
                // No sleep needed, tx confirmation is usually enough
            }

            // 2. Retry Mint
            const mintAmount = getRandomInt(1000, 5000);
            const amountWei = ethers.parseUnits(mintAmount.toString(), 6);
            // 2. Retry Mint

            const gasOverridesMint = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            await sendTxWithRetry(wallet, async () => {
                return tokenContract.mint(wallet.address, amountWei, {
                    gasLimit: 1000000, // Increased 500k -> 1M
                    ...gasOverridesMint
                });
            });
            if (!silent) console.log(`${COLORS.fg.green}✓ Minted ${mintAmount} ${tokenSymbol}${COLORS.reset}`);
        }

        // Quick Balance Check (Optimized)
        // No arbitrary 3s wait. Just check.
        const balanceAfter = await tokenContract.balanceOf(wallet.address);
        if (balanceAfter < ethers.parseUnits("5", 18)) {
            // Only if balance is low, maybe wait a bit to ensure sync?
            await sleep(1000);
            // Re-check
        }

        // Set fee token preference to PathUSD (Fire and Forget / Parallel)
        // We don't necessarily need to await this to block the next step if we manage nonces correctly,
        // but for safety we await. However, we can use our sendTxWithRetry if needed later.
        // For now, standard wait is fine, but we remove excessive logging/checking
        const FEE_MANAGER_ABI = ["function setUserToken(address token)"];
        const feeManager = new ethers.Contract(SYSTEM_CONTRACTS.FEE_MANAGER, FEE_MANAGER_ABI, wallet);
        try {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            // We use a lower gas limit for this simple call
            await (await feeManager.setUserToken(CONFIG.TOKENS.PathUSD, { ...gasOverrides, gasLimit: 400000 })).wait();
        } catch (e) { }

    } catch (e) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchTxMeme', 'failed', `Mint error: ${e.message}`, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Mint failed: ${e.message}${COLORS.reset}`);
        return { success: false, reason: `mint_failed: ${e.message}` };
    }

    // Step 3: Batch transfer the minted token
    if (!silent) console.log(`${COLORS.fg.yellow}Step 3/3: Batch transferring ${tokenSymbol}...${COLORS.reset}`);

    // Random transfer amount between 0.01 and 1.00 tokens
    const randomAmount = (Math.random() * 1000 + 100).toFixed(2);
    const amount = ethers.parseUnits(randomAmount, 6);

    if (!silent) console.log(`${COLORS.dim}Transferring ${randomAmount} ${tokenSymbol} to ${count} recipients${COLORS.reset}`);

    const recipients = Array.from({ length: count }, () => ethers.Wallet.createRandom().address);
    const batchContract = getBatchContract(wallet);
    const tokenC = new ethers.Contract(tokenAddr, ERC20_ABI, wallet);

    try {
        if (batchContract) {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            const allowance = await tokenC.allowance(wallet.address, await batchContract.getAddress());
            if (allowance < (amount * BigInt(count))) {
                await sendTxWithRetry(wallet, async () => {
                    return tokenC.approve(await batchContract.getAddress(), ethers.MaxUint256, { ...gasOverrides });
                });
            }

            const amounts = Array(count).fill(amount);

            // Increased Gas Limit for Batch
            const result = await sendTxWithRetry(wallet, async () => {
                return batchContract.batchTransfer(tokenAddr, recipients, amounts, {
                    ...gasOverrides,
                    gasLimit: 6000000, // Increased 3M -> 6M
                    feeCurrency: CONFIG.TOKENS.PathUSD
                });
            });

            const txHash = result.hash;
            if (!silent) console.log(`${COLORS.dim}Batch Tx (${tokenSymbol}): ${CONFIG.EXPLORER_URL}/tx/${txHash}${COLORS.reset}`);
            const receipt = result.receipt;

            const duration = (Date.now() - startTime) / 1000;
            logWalletAction(workerId, walletIndex, wallet.address, 'BatchTxMeme', 'success', `${count}x${tokenSymbol} (batch)`, silent, duration);
            if (!silent) console.log(`${COLORS.fg.green}✓ ${count} transfers completed (batch)!${COLORS.reset}`);

            return { success: true, mode: 'batch_contract', txHash: txHash, count };
        }
    } catch (e) {
        if (!silent) console.log(`${COLORS.dim}Batch failed (${e.message.substring(0, 30)}). Sequential fallback...${COLORS.reset}`);
    }

    // Sequential fallback
    let successCount = 0;
    for (let i = 0; i < count; i++) {
        try {
            // Estimate gas and add 50% buffer
            const gasEstimate = await tokenC.transfer.estimateGas(recipients[i], amount);
            const gasLimit = (gasEstimate * 300n) / 100n; // Increased buffer 150 -> 300%
            const gasOverridesSeq = await getGasWithMultiplier(wallet.provider, undefined, wallet);

            const tx = await tokenC.transfer(recipients[i], amount, {
                gasLimit,
                ...gasOverridesSeq,
                feeCurrency: CONFIG.TOKENS.PathUSD
            });
            if (!silent) console.log(`${COLORS.dim}Tx ${i + 1}: ${shortHash(tx.hash)}${COLORS.reset}`);
            await tx.wait();
            successCount++;
            var lastTxHash = tx.hash;
        } catch (e) {
            if (!silent) console.log(`${COLORS.fg.red}Tx ${i + 1} failed: ${e.message}${COLORS.reset}`);
        }
    }

    const duration = (Date.now() - startTime) / 1000;
    if (successCount > 0) {
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchTxMeme', 'success', `${successCount}/${count}x${tokenSymbol} (seq)`, silent, duration);
        if (!silent) console.log(`${COLORS.fg.green}✓ ${successCount}/${count} transfers completed (sequential)!${COLORS.reset}`);
    } else {
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchTxMeme', 'failed', 'All transfers failed', silent, duration);
    }

    return { success: successCount > 0, mode: 'sequential', count: successCount, txHash: lastTxHash };
}
