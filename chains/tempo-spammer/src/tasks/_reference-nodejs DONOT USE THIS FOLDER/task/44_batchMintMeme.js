import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { loadCreatedMemes } from '../utils/wallet.js';
import { getRandomInt } from '../utils/helpers.js';
import { TempoSDKService } from '../utils/tempoService.js';
import { createRandomMemeForWallet } from './21_createMeme.js';

const MINT_ABI = [
    "function mint(address to, uint256 amount)",
    "function decimals() view returns (uint8)",
    "function symbol() view returns (string)",
    "function hasRole(bytes32 role, address account) view returns (bool)",
    "function grantRole(bytes32 role, address account)"
];

// Some Meme tokens might use different role constants or Just owner, but standard is ISSUER/MINTER
const ISSUER_ROLE = ethers.id("ISSUER_ROLE");

export async function batchMintMemeForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const service = new TempoSDKService(wallet);

    // 1. Load Memes
    let createdMemes = loadCreatedMemes();
    const walletAddress = ethers.getAddress(wallet.address);
    let myMemes = createdMemes[walletAddress] || createdMemes[walletAddress.toLowerCase()] || [];

    if (myMemes.length === 0) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No created memes found. Creating one first...${COLORS.reset}`);
        const newMeme = await createRandomMemeForWallet(wallet, proxy, workerId, walletIndex, silent);
        if (newMeme?.success && newMeme?.tokenAddress) {
            myMemes = [{ token: newMeme.tokenAddress, symbol: newMeme.symbol }];
        } else {
            return { success: false, reason: 'auto_create_meme_failed' };
        }
    }

    // 2. Select Meme
    const tokenInfo = myMemes[Math.floor(Math.random() * myMemes.length)];
    const tokenAddress = tokenInfo.token;
    const tokenSymbol = tokenInfo.symbol || '???';
    const token = new ethers.Contract(tokenAddress, MINT_ABI, wallet);

    if (!silent) console.log(`${COLORS.fg.magenta}🚀 Batch Minting (Meme): ${tokenSymbol} (${tokenAddress.substring(0, 6)}...)${COLORS.reset}`);

    // 3. Check Role (Single Check)
    try {
        const hasRole = await token.hasRole(ISSUER_ROLE, wallet.address);
        if (!hasRole) {
            if (!silent) console.log(`${COLORS.dim}Granting ISSUER_ROLE first...${COLORS.reset}`);
            const tx = await token.grantRole(ISSUER_ROLE, wallet.address, { feeCurrency: CONFIG.TOKENS.PathUSD });
            await tx.wait();
        }
    } catch (e) { /* Ignore if not AccessControl */ }

    // 4. Construct Batch
    const BATCH_SIZE = 10;
    const calls = [];
    let totalMinted = 0;

    let decimals = 18;
    try { decimals = await token.decimals(); } catch (e) { }

    for (let i = 0; i < BATCH_SIZE; i++) {
        // Random amount between 1M and 5M ~ Memes often have higher supply
        const amountVal = getRandomInt(1000000, 5000000);
        const amountWei = ethers.parseUnits(amountVal.toString(), decimals);

        // Encode 'mint' call
        const data = token.interface.encodeFunctionData('mint', [wallet.address, amountWei]);

        calls.push({
            to: tokenAddress,
            value: 0n,
            data: data
        });
        totalMinted += amountVal;
    }

    if (!silent) console.log(`${COLORS.fg.yellow}📦 Batching ${BATCH_SIZE} mint operations (${totalMinted} total)...${COLORS.reset}`);

    try {
        // 5. Send Atomic Batch
        // Use a System Token for gas (randomly selected or PathUSD)
        const systemTokenKeys = Object.keys(CONFIG.TOKENS);
        const feeTokenSymbol = systemTokenKeys[Math.floor(Math.random() * systemTokenKeys.length)];

        if (!silent) console.log(`${COLORS.dim}Gas Token: ${feeTokenSymbol}${COLORS.reset}`);

        const result = await service.sendBatchTransaction(calls, feeTokenSymbol);

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchMintMeme', 'success', `Batch: ${BATCH_SIZE}x Mint (${totalMinted} ${tokenSymbol})`, silent, duration, proxy);

        if (!silent) console.log(`${COLORS.fg.green}✓ Batch Success! Hash: ${result.transactionHash}${COLORS.reset}`);

        return { success: true, txHash: result.transactionHash, count: BATCH_SIZE };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchMintMeme', 'failed', error.message.substring(0, 50), silent, duration, proxy);

        if (error.message.includes("nonce has already been used")) {
            console.log(`${COLORS.fg.red}⚠ Nonce Collision. The TempoSDKService fix should prevent this.${COLORS.reset}`);
        }

        if (!silent) console.log(`${COLORS.fg.red}✗ Batch Failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}
