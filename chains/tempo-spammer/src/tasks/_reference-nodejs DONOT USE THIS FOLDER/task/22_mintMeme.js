
import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';
import { createRandomMemeForWallet } from './21_createMeme.js';
import { ConcurrentService } from '../utils/tempoConcurrent.js';

const MINT_ABI = [
    "function mint(address to, uint256 amount)",
    "function balanceOf(address owner) view returns (uint256)",
    "function decimals() view returns (uint8)",
    "function hasRole(bytes32 role, address account) view returns (bool)",
    "function grantRole(bytes32 role, address account)"
];

export async function mintMemeForWallet(wallet, proxy, tokenAddress, tokenSymbol, amount, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    if (!silent) console.log(`${COLORS.fg.yellow}Minting ${amount} ${tokenSymbol} (Atomic Batch)...${COLORS.reset}`);

    try {
        const tokenContract = new ethers.Contract(tokenAddress, MINT_ABI, wallet);
        let decimals = 6;
        try { decimals = await tokenContract.decimals(); } catch (e) { }

        const amountWei = ethers.parseUnits(amount.toString(), decimals);
        const calls = [];

        // Optimistically Grant Role always? Or check? 
        // Checking reads state (fast-ish), but saving a tx is key. 
        // If we batch, we can include Grant Role + Mint in one tx. 
        // If we already have role, Grant Role might revert or just do nothing? 
        // Standard OpenZeppelin AccessControl reverts if account already has role? No, it just emits event or does nothing usually?
        // Actually, it emits `RoleGranted`. It does NOT revert if you already have it.
        // So we can SAFELY always batch Grant + Mint! This is huge speedup. No need to check.

        const ISSUER_ROLE = ethers.id("ISSUER_ROLE");

        // 1. Grant Role Call
        const grantData = tokenContract.interface.encodeFunctionData("grantRole", [ISSUER_ROLE, wallet.address]);
        calls.push({ to: tokenAddress, data: grantData, value: 0n });

        // 2. Mint Call
        const mintData = tokenContract.interface.encodeFunctionData("mint", [wallet.address, amountWei]);
        calls.push({ to: tokenAddress, data: mintData, value: 0n });

        // Send Atomic Batch
        const service = new ConcurrentService(wallet.privateKey, proxy);
        const txHash = await service.sendAtomicBatch(calls, Date.now(), CONFIG.TOKENS.PathUSD);

        if (!silent) console.log(`${COLORS.dim}Batch Sent (Grant+Mint): ${txHash.substring(0, 20)}...${COLORS.reset}`);

        // Wait for receipt (Robustness for now, can be Fire-And-Forget later i.e. wait=false if we update this fn signature)
        // For now user wants optimization, assuming <5s is fine.
        const receipt = await service.publicClient.waitForTransactionReceipt({ hash: txHash });

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'MintMeme', 'success', `+${amount} ${tokenSymbol}`, silent, duration);
        if (!silent) console.log(`${COLORS.fg.green}✓ Minted ${amount} ${tokenSymbol}! Block: ${receipt.blockNumber}${COLORS.reset}`);

        return { success: true, txHash: txHash, block: receipt.blockNumber, token: tokenSymbol, amount };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'MintMeme', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Mint failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function mintRandomMemeForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const { loadCreatedMemes } = await import('../utils/wallet.js');
    const memes = loadCreatedMemes()[ethers.getAddress(wallet.address)] || [];

    if (memes.length === 0) {
        if (!silent) console.log(`${COLORS.fg.red}No memes created by this wallet to mint.${COLORS.reset}`);
        return { success: false, reason: "No memes found" };
    }

    const meme = memes[Math.floor(Math.random() * memes.length)];
    const amount = getRandomInt(10, 100).toString();

    return await mintMemeForWallet(wallet, proxy, meme.token, meme.symbol, amount, workerId, walletIndex, silent);
}