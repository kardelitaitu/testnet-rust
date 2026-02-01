import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { loadCreatedTokens } from '../utils/wallet.js';
import { getRandomInt } from '../utils/helpers.js';
import { TempoSDKService } from '../utils/tempoService.js';
import { createRandomStableForWallet } from './4_createStable.js';

const MINT_ABI = [
    "function mint(address to, uint256 amount)",
    "function decimals() view returns (uint8)",
    "function symbol() view returns (string)",
    "function hasRole(bytes32 role, address account) view returns (bool)",
    "function grantRole(bytes32 role, address account)"
];

const ISSUER_ROLE = ethers.id("ISSUER_ROLE");

export async function batchMintStableForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const service = new TempoSDKService(wallet);

    // 1. Load Tokens
    let createdTokens = loadCreatedTokens();
    const walletAddress = ethers.getAddress(wallet.address);
    let myTokens = createdTokens[walletAddress] || createdTokens[walletAddress.toLowerCase()] || [];

    if (myTokens.length === 0) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No created tokens found. Creating one first...${COLORS.reset}`);
        const newStable = await createRandomStableForWallet(wallet, proxy, workerId, walletIndex, silent);
        if (newStable?.success && newStable?.tokenAddress) {
            myTokens = [{ token: newStable.tokenAddress, symbol: newStable.symbol }];
        } else {
            return { success: false, reason: 'auto_create_failed' };
        }
    }

    // 2. Select Token
    const tokenInfo = myTokens[Math.floor(Math.random() * myTokens.length)];
    const tokenAddress = tokenInfo.token;
    const tokenSymbol = tokenInfo.symbol || '???';
    const token = new ethers.Contract(tokenAddress, MINT_ABI, wallet);

    if (!silent) console.log(`${COLORS.fg.magenta}🚀 Batch Minting (Stable): ${tokenSymbol} (${tokenAddress.substring(0, 6)}...)${COLORS.reset}`);

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
        // Random amount between 10M and 20M
        const amountVal = getRandomInt(10000000, 20000000);
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
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchMintStable', 'success', `Batch: ${BATCH_SIZE}x Mint (${totalMinted} ${tokenSymbol})`, silent, duration, proxy);

        if (!silent) console.log(`${COLORS.fg.green}✓ Batch Success! Hash: ${result.transactionHash}${COLORS.reset}`);

        return { success: true, txHash: result.transactionHash, count: BATCH_SIZE };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'BatchMintStable', 'failed', error.message.substring(0, 50), silent, duration, proxy);

        // Check for specific nonce error to helpful message
        if (error.message.includes("nonce has already been used")) {
            console.log(`${COLORS.fg.red}⚠ Nonce Collision. The TempoSDKService fix should prevent this.${COLORS.reset}`);
        }

        if (!silent) console.log(`${COLORS.fg.red}✗ Batch Failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}
