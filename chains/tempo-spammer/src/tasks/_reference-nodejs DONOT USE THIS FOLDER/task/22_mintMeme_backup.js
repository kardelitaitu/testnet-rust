import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet, loadCreatedMemes } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';
import { createRandomMemeForWallet } from './21_createMeme.js';

const MINT_ABI = [
    "function mint(address to, uint256 amount)",
    "function hasRole(bytes32 role, address account) view returns (bool)",
    "function grantRole(bytes32 role, address account)",
    "function decimals() view returns (uint8)",
    "function symbol() view returns (string)",
    "function balanceOf(address owner) view returns (uint256)"
];

const ISSUER_ROLE = ethers.id("ISSUER_ROLE");

export async function mintRandomMemeForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    // Load created memes
    let createdMemes = loadCreatedMemes();
    const walletAddress = ethers.getAddress(wallet.address);
    let myMemes = createdMemes[walletAddress] || createdMemes[walletAddress.toLowerCase()] || [];

    // Auto-create if no memes exist
    if (myMemes.length === 0) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No memes - creating one first...${COLORS.reset}`);

        const createResult = await createRandomMemeForWallet(wallet, proxy, workerId, walletIndex, silent);

        if (createResult?.success) {
            await sleep(2000);
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

    // Random amount (100k-1M)
    const amount = getRandomInt(100000, 1000000);

    return await mintMemeForWallet(wallet, proxy, memeInfo.token, memeInfo.symbol, amount, workerId, walletIndex, silent);
}

export async function mintMemeForWallet(wallet, proxy, tokenAddress, tokenSymbol, amount, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    if (!silent) console.log(`${COLORS.fg.yellow}Minting ${amount} ${tokenSymbol}...${COLORS.reset}`);

    try {
        const tokenContract = new ethers.Contract(tokenAddress, MINT_ABI, wallet);

        // Check Role (Best effort)
        let hasRole = false;
        try {
            hasRole = await tokenContract.hasRole(ISSUER_ROLE, wallet.address);

            if (!hasRole) {
                if (!silent) console.log(`${COLORS.dim}Granting Role...${COLORS.reset}`);

                try {
                    const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
                    await sendTxWithRetry(wallet, async () => {
                        return tokenContract.grantRole(ISSUER_ROLE, wallet.address, {
                            gasLimit: 300000,
                            ...gasOverrides,
                            feeCurrency: CONFIG.TOKENS.PathUSD
                        });
                    });
                    if (!silent) console.log(`${COLORS.fg.green}✓ Role Granted${COLORS.reset}`);
                    await sleep(3000);
                } catch (grantErr) {
                    if (!silent) console.log(`${COLORS.fg.yellow}⚠ Role grant skipped/failed: ${grantErr.message.substring(0, 50)}${COLORS.reset}`);
                    return { success: false, reason: `role_grant_failed: ${grantErr.message}` };
                }


                // Final check
                const verified = await tokenContract.hasRole(ISSUER_ROLE, wallet.address);
                if (!verified) {
                    if (!silent) console.log(`${COLORS.fg.red}✗ Stuck without ISSUER_ROLE. Aborting mint.${COLORS.reset}`);
                    return { success: false, reason: 'missing_issuer_role' };
                }
                hasRole = true;
            }
        } catch (roleErr) {
            if (!silent) console.log(`${COLORS.dim}Role check reverted (${roleErr.code}). Assuming role needed...${COLORS.reset}`);
            // If hasRole reverts, we assume we don't have it (or contract is weird) and try to grant it.
            // If the contract is truly broken, grantRole will also fail, which is handled below.
        }

        // Grant logic (runs if !hasRole OR if hasRole reverted)
        if (typeof hasRole === 'undefined' || !hasRole) {
            if (!silent) console.log(`${COLORS.dim}Granting Role...${COLORS.reset}`);
            try {
                const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
                await sendTxWithRetry(wallet, async () => {
                    return tokenContract.grantRole(ISSUER_ROLE, wallet.address, {
                        gasLimit: 300000,
                        ...gasOverrides,
                        feeCurrency: CONFIG.TOKENS.PathUSD
                    });
                });
                if (!silent) console.log(`${COLORS.fg.green}✓ Role Granted${COLORS.reset}`);
                await sleep(3000);
            } catch (grantErr) {
                // If grant also fails, the contract is likely broken or network is issue
                if (!silent) console.log(`${COLORS.fg.yellow}⚠ Role grant failed: ${grantErr.message.substring(0, 50)}...${COLORS.reset}`);
                return { success: false, reason: `contract_interaction_failed` };
            }
        }

        let decimals = 6;
        try { decimals = await tokenContract.decimals(); } catch (e) { }

        const amountWei = ethers.parseUnits(amount.toString(), decimals);

        // Check balance before
        const balBefore = await tokenContract.balanceOf(wallet.address);
        if (!silent) console.log(`${COLORS.dim}Balance before: ${ethers.formatUnits(balBefore, decimals)}${COLORS.reset}`);

        // Mint
        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);

        const result = await sendTxWithRetry(wallet, async () => {
            return tokenContract.mint(wallet.address, amountWei, {
                gasLimit: 300000,
                ...gasOverrides,
                feeCurrency: CONFIG.TOKENS.PathUSD
            });
        });

        const receipt = result.receipt;
        if (!silent) console.log(`${COLORS.dim}Tx: ${CONFIG.EXPLORER_URL}/tx/${result.hash}${COLORS.reset}`);

        const balAfter = await tokenContract.balanceOf(wallet.address);
        const minted = balAfter - balBefore;
        const mintedFormatted = ethers.formatUnits(minted, decimals);

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'MintMeme', 'success', `+${mintedFormatted} ${tokenSymbol}`, silent, duration);
        if (!silent) console.log(`${COLORS.fg.green}✓ Minted ${mintedFormatted} ${tokenSymbol}! Block: ${receipt.blockNumber}${COLORS.reset}`);

        return { success: true, txHash: result.hash, block: receipt.blockNumber, token: tokenSymbol, amount: mintedFormatted };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'MintMeme', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Mint failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function runMintMemeMenu() {
    console.log(`\n  ${COLORS.fg.magenta}🏭  MINT MEME TOKEN MODULE${COLORS.reset}\n`);
    console.log(`${COLORS.fg.yellow}Mint additional meme tokens (100k-1M)${COLORS.reset}\n`);

    const privateKeys = getPrivateKeys();
    console.log(`${COLORS.fg.cyan}Found ${privateKeys.length} wallet(s)${COLORS.reset}\n`);

    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);

        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}: ${wallet.address}${COLORS.reset}`);

        await mintRandomMemeForWallet(wallet, proxy, 1, i);

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(CONFIG.MIN_DELAY_BETWEEN_WALLETS, CONFIG.MAX_DELAY_BETWEEN_WALLETS), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ Meme minting completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}