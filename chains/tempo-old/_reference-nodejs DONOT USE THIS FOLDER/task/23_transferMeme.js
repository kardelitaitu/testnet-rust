
import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier } from '../utils/helpers.js';
import { createRandomMemeForWallet } from './21_createMeme.js';
import { ConcurrentService } from '../utils/tempoConcurrent.js';

const TRANSFER_ABI = [
    "function transfer(address to, uint256 amount) returns (bool)",
    "function balanceOf(address owner) view returns (uint256)",
    "function decimals() view returns (uint8)"
];

export async function transferMemeForWallet(wallet, proxy, tokenAddress, tokenSymbol, recipient, amount, workerId = 1, walletIndex = 0, silent = false, feeData = null) {
    const startTime = Date.now();
    if (!silent) console.log(`${COLORS.fg.yellow}Transferring ${amount} ${tokenSymbol} to ${recipient.substring(0, 10)} (Atomic Batch)...${COLORS.reset}`);

    try {
        const tokenContract = new ethers.Contract(tokenAddress, TRANSFER_ABI, wallet);
        let decimals = 6;
        try { decimals = await tokenContract.decimals(); } catch (e) { }

        const amountWei = ethers.parseUnits(amount.toString(), decimals);
        const calls = [];

        // 1. Check Balance
        let balance = 0n;
        try { balance = await tokenContract.balanceOf(wallet.address); } catch (e) { }

        if (balance < amountWei) {
            // Add Mint Call
            const MINT_ABI = ["function mint(address to, uint256 amount)"];
            const mintContract = new ethers.Contract(tokenAddress, MINT_ABI, wallet);
            // Mint 1000 buffer
            const mintAmount = ethers.parseUnits("1000", decimals);
            const mintData = mintContract.interface.encodeFunctionData("mint", [wallet.address, mintAmount]);
            calls.push({ to: tokenAddress, data: mintData, value: 0n });
        }

        // 2. Set Fee Token Preference (Optimal: Include always or skip? Including ensures reliability)
        // Actually, let's include it. It's cheap in atomic batch.
        const FEE_MANAGER_ABI = ["function setUserToken(address token)"];
        const feeManager = new ethers.Contract(SYSTEM_CONTRACTS.FEE_MANAGER, FEE_MANAGER_ABI, wallet);
        const setFeeData = feeManager.interface.encodeFunctionData("setUserToken", [CONFIG.TOKENS.PathUSD]);
        calls.push({ to: SYSTEM_CONTRACTS.FEE_MANAGER, data: setFeeData, value: 0n });

        // 3. Transfer Call
        const transferData = tokenContract.interface.encodeFunctionData("transfer", [recipient, amountWei]);
        calls.push({ to: tokenAddress, data: transferData, value: 0n });

        // Send Batch
        const service = new ConcurrentService(wallet.privateKey, proxy);
        const txHash = await service.sendAtomicBatch(calls, Date.now(), CONFIG.TOKENS.PathUSD);

        if (!silent) console.log(`${COLORS.dim}Batch Sent (Mint?+SetFee+Transfer): ${txHash.substring(0, 20)}...${COLORS.reset}`);

        const receipt = await service.publicClient.waitForTransactionReceipt({ hash: txHash });

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'TransferMeme', 'success', `-${amount} ${tokenSymbol}`, silent, duration);
        if (!silent) console.log(`${COLORS.fg.green}✓ Transferred ${amount} ${tokenSymbol}! Block: ${receipt.blockNumber}${COLORS.reset}`);

        return { success: true, txHash: txHash, block: receipt.blockNumber, token: tokenSymbol, amount };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'TransferMeme', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Transfer failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function transferRandomMemeForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const { loadCreatedMemes } = await import('../utils/wallet.js');
    const memes = loadCreatedMemes()[ethers.getAddress(wallet.address)] || [];

    if (memes.length === 0) {
        if (!silent) console.log(`${COLORS.fg.red}No memes created by this wallet to transfer.${COLORS.reset}`);
        return { success: false, reason: "No memes found" };
    }

    const meme = memes[Math.floor(Math.random() * memes.length)];
    // Transfer back to self for testing usually, or random address?
    // In original code it was using random hex.
    const recipient = ethers.Wallet.createRandom().address;
    const amount = getRandomInt(1, 10).toString();

    return await transferMemeForWallet(wallet, proxy, meme.token, meme.symbol, recipient, amount, workerId, walletIndex, silent);
}