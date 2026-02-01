import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet, loadCreatedMemes } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier } from '../utils/helpers.js';
import { createRandomMemeForWallet } from './21_createMeme.js';

const TRANSFER_ABI = [
    "function transfer(address to, uint256 amount) returns (bool)",
    "function balanceOf(address owner) view returns (uint256)",
    "function decimals() view returns (uint8)",
    "function symbol() view returns (string)"
];

// Update export to match
export async function transferRandomMemeForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false, feeData = null) {
    // Load created memes
    let createdMemes = loadCreatedMemes();
    const walletAddress = ethers.getAddress(wallet.address);
    let myMemes = createdMemes[walletAddress] || createdMemes[walletAddress.toLowerCase()] || [];

    // ... (logic to create meme if empty) ...
    // Auto-create if no memes exist
    if (myMemes.length === 0) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No memes - creating one first...${COLORS.reset}`);

        const createResult = await createRandomMemeForWallet(wallet, proxy, workerId, walletIndex, silent);

        if (createResult?.success) {
            // Wait for blockchain propagation
            await new Promise(r => setTimeout(r, 3000));
            createdMemes = loadCreatedMemes();
            myMemes = createdMemes[ethers.getAddress(wallet.address)] || [];

            if (myMemes.length > 0) {
                if (!silent) console.log(`${COLORS.fg.green}✓ Meme created and tracked${COLORS.reset}`);
            } else {
                if (!silent) console.log(`${COLORS.fg.red}✗ Meme created but not loaded${COLORS.reset}`);
                return { success: false, reason: 'meme_not_loaded' };
            }
        } else {
            if (!silent) console.log(`${COLORS.fg.red}✗ Failed to create meme${COLORS.reset}`);
            return { success: false, reason: 'failed_to_create_meme' };
        }
    }

    // Random meme selection
    const memeInfo = myMemes[Math.floor(Math.random() * myMemes.length)];
    if (!silent) console.log(`${COLORS.fg.cyan}Selected Meme: ${memeInfo.symbol} (${memeInfo.token})${COLORS.reset}`);

    // Random amount (20-100)
    const amount = getRandomInt(20, 100);

    // Random recipient wallet
    const privateKeys = getPrivateKeys();
    if (privateKeys.length < 2) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ Need at least 2 wallets for transfer${COLORS.reset}`);
        return { success: false, reason: 'not_enough_wallets' };
    }

    // Select random wallet (not self)
    let recipientIndex;
    do {
        recipientIndex = Math.floor(Math.random() * privateKeys.length);
    } while (recipientIndex === walletIndex && privateKeys.length > 1);

    const { wallet: recipientWallet } = await getWallet(recipientIndex, privateKeys[recipientIndex]);

    return await transferMemeForWallet(wallet, proxy, memeInfo.token, memeInfo.symbol, recipientWallet.address, amount, workerId, walletIndex, silent, feeData);
}

export async function transferMemeForWallet(wallet, proxy, tokenAddress, tokenSymbol, recipient, amount, workerId = 1, walletIndex = 0, silent = false, feeData = null) {
    const startTime = Date.now();
    if (!silent) console.log(`${COLORS.fg.yellow}Transferring ${amount} ${tokenSymbol} to ${recipient.substring(0, 10)}...${COLORS.reset}`);

    try {
        const tokenContract = new ethers.Contract(tokenAddress, TRANSFER_ABI, wallet);

        let decimals = 6;
        try { decimals = await tokenContract.decimals(); } catch (e) { }

        const amountWei = ethers.parseUnits(amount.toString(), decimals);

        // Check balance
        const balance = await tokenContract.balanceOf(wallet.address);
        const balanceFormatted = ethers.formatUnits(balance, decimals);

        if (balance < amountWei) {
            // Validating if we can mint more
            if (!silent) console.log(`${COLORS.fg.yellow}⚠ Insufficient balance - attempting to mint more...${COLORS.reset}`);

            try {
                const MINT_ABI = ["function mint(address to, uint256 amount)"];
                const mintContract = new ethers.Contract(tokenAddress, MINT_ABI, wallet);
                // Mint 1000 more
                const gasOverridesMint = await getGasWithMultiplier(wallet.provider, undefined, wallet);
                const mintAmount = ethers.parseUnits("1000", decimals);
                const txMint = await mintContract.mint(wallet.address, mintAmount, {
                    gasLimit: 200000,
                    ...gasOverridesMint
                });
                await txMint.wait();
                if (!silent) console.log(`${COLORS.fg.green}✓ Minted additional 1000 tokens${COLORS.reset}`);

                // Update balance
                const newBalance = await tokenContract.balanceOf(wallet.address);
                if (newBalance < amountWei) {
                    // Still not enough?
                    throw new Error("Still insufficient after minting");
                }
            } catch (mintCheckErr) {
                if (!silent) console.log(`${COLORS.fg.yellow}⚠ Failed to mint more (Role missing?). Creating FRESH meme token...${COLORS.reset}`);

                // Fallback: Create a brand new meme for this wallet
                const createResult = await createRandomMemeForWallet(wallet, proxy, workerId, walletIndex, silent);
                if (createResult && createResult.success) {
                    // Now try to transfer this new token
                    return await transferMemeForWallet(wallet, proxy, createResult.tokenAddress, createResult.symbol, recipient, amount, workerId, walletIndex, silent);
                }

                const duration = (Date.now() - startTime) / 1000;
                logWalletAction(workerId, walletIndex, wallet.address, 'TransferMeme', 'skipped', `Insufficient balance & cant mint`, silent, duration);
                return { success: false, reason: 'insufficient_balance_and_mint_failed' };
            }
        }

        if (!silent) console.log(`${COLORS.dim}Balance: ${balanceFormatted}${COLORS.reset}`);

        // Ensure PathUSD is set as the fee token globally for this user
        if (!silent) console.log(`${COLORS.dim}Setting fee token preference to PathUSD...${COLORS.reset}`);
        try {
            const FEE_MANAGER_ABI = ["function setUserToken(address token)"];
            const feeManager = new ethers.Contract(SYSTEM_CONTRACTS.FEE_MANAGER, FEE_MANAGER_ABI, wallet);
            const gasOverridesFee = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            const setTx = await feeManager.setUserToken(CONFIG.TOKENS.PathUSD, {
                gasLimit: 100000,
                ...gasOverridesFee
            });
            await setTx.wait();
            if (!silent) console.log(`${COLORS.fg.green}✓ Global fee token set to PathUSD${COLORS.reset}`);
        } catch (feeManagerErr) {
            if (!silent) console.log(`${COLORS.fg.yellow}⚠ Warning: Could not set global fee token preference: ${feeManagerErr.message}${COLORS.reset}`);
        }

        // Simple transfer using contract method
        if (!silent) console.log(`${COLORS.fg.cyan}Transferring ${amount} ${tokenSymbol}...${COLORS.reset}`);

        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
        // FORCE PathUSD as feeCurrency as requested
        const tx = await tokenContract.transfer(recipient, amountWei, {
            gasLimit: 300000,
            gasPrice: gasOverrides.gasPrice || gasOverrides.maxFeePerGas, // Use price from provider
            feeCurrency: CONFIG.TOKENS.PathUSD, // Redundant but safe
        });

        if (!silent) console.log(`${COLORS.dim}Tx: ${CONFIG.EXPLORER_URL}/tx/${tx.hash}${COLORS.reset}`);
        const receipt = await tx.wait();

        const balanceAfter = await tokenContract.balanceOf(wallet.address);
        const balanceAfterFormatted = ethers.formatUnits(balanceAfter, decimals);

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'TransferMeme', 'success', `-${amount} ${tokenSymbol}`, silent, duration);
        if (!silent) console.log(`${COLORS.fg.green}✓ Transferred ${amount} ${tokenSymbol}! Block: ${receipt.blockNumber}${COLORS.reset}`);
        if (!silent) console.log(`${COLORS.dim}New balance: ${balanceAfterFormatted}${COLORS.reset}`);

        return { success: true, txHash: tx.hash, block: receipt.blockNumber, token: tokenSymbol, amount };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'TransferMeme', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Transfer failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function runTransferMemeMenu() {
    console.log(`\n  ${COLORS.fg.magenta}💸  TRANSFER MEME TOKEN MODULE${COLORS.reset}\n`);
    console.log(`${COLORS.fg.yellow}Transfer meme tokens to random wallets (20-100)${COLORS.reset}\n`);

    const privateKeys = getPrivateKeys();
    console.log(`${COLORS.fg.cyan}Found ${privateKeys.length} wallet(s)${COLORS.reset}\n`);

    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);

        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}: ${wallet.address}${COLORS.reset}`);

        await transferRandomMemeForWallet(wallet, proxy, 1, i);

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(CONFIG.MIN_DELAY_BETWEEN_WALLETS, CONFIG.MAX_DELAY_BETWEEN_WALLETS), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ Meme transfers completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}