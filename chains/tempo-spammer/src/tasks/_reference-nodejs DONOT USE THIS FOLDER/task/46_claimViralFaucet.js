import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const TRACKER_FILE = path.join(ROOT_DIR, 'data', 'viral_faucets.json');

const FAUCET_ABI = [
    "function claim(address token, uint256 amount)",
    "function getBalance(address token) view returns (uint256)"
];

const ERC20_ABI = [
    "function decimals() view returns (uint8)",
    "function symbol() view returns (string)",
    "function balanceOf(address owner) view returns (uint256)"
];

// Helper to load faucets
function loadFaucets() {
    if (!fs.existsSync(TRACKER_FILE)) return [];
    try {
        return JSON.parse(fs.readFileSync(TRACKER_FILE, 'utf8'));
    } catch (e) { return []; }
}

export async function claimViralFaucetForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();

    // 1. Load Faucets
    const faucets = loadFaucets();
    if (faucets.length === 0) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No viral faucets found. Deploy one first!${COLORS.reset}`);
        return { success: false, reason: 'no_faucets_found' };
    }

    // 2. Pick a Random Faucet
    // We can prioritize newer ones or random ones. Random is fine.
    const faucetInfo = faucets[Math.floor(Math.random() * faucets.length)];
    const contractAddress = faucetInfo.address;
    const tokenAddress = faucetInfo.token;
    const tokenSymbol = faucetInfo.symbol || '???';

    if (!contractAddress || !tokenAddress) {
        return { success: false, reason: 'invalid_faucet_data' };
    }

    if (!silent) console.log(`${COLORS.fg.magenta}🚰 Claiming from Faucet: ${contractAddress.substring(0, 8)}... (${tokenSymbol})${COLORS.reset}`);

    // 3. Check Faucet Balance & Decimals
    const faucetContract = new ethers.Contract(contractAddress, FAUCET_ABI, wallet);
    const tokenContract = new ethers.Contract(tokenAddress, ERC20_ABI, wallet);

    try {
        let decimals = faucetInfo.decimals;
        if (!decimals) {
            try { decimals = await tokenContract.decimals(); } catch (e) { decimals = 18; }
        }

        // Optimize: Fire gas estimation in background if we are confident, or just check balance first
        const balance = await faucetContract.getBalance(tokenAddress);
        const claimAmount = ethers.parseUnits("1", decimals); // Claim 1 unit

        if (balance < claimAmount) {
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) console.log(`${COLORS.fg.red}✗ Faucet Empty: ${ethers.formatUnits(balance, decimals)} ${tokenSymbol}${COLORS.reset}`);
            // Optional: Cleanup empty faucet from file? No, maybe later.
            logWalletAction(workerId, walletIndex, wallet.address, 'ClaimFaucet', 'skipped', `Faucet Empty (${tokenSymbol})`, silent, duration, proxy);
            return { success: false, reason: 'faucet_empty' };
        }

        // 4. Claim
        if (!silent) console.log(`${COLORS.fg.yellow}Claiming 1 ${tokenSymbol}...${COLORS.reset}`);

        // USER REQUEST: Double the gas price (Handled by Global Config)
        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);

        // Wrap in sendTxWithRetry for robust nonce handling
        const result = await sendTxWithRetry(wallet, async () => {
            return faucetContract.claim(tokenAddress, claimAmount, {
                gasLimit: 300000,
                ...gasOverrides
            });
        });

        const txHash = result.hash;
        if (!silent) console.log(`${COLORS.dim}Tx: ${CONFIG.EXPLORER_URL}/tx/${txHash}${COLORS.reset}`);

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'ClaimFaucet', 'success', `Claimed 1 ${tokenSymbol}`, silent, duration, proxy);
        if (!silent) console.log(`${COLORS.fg.green}✓ Claimed Successfully!${COLORS.reset}`);

        return { success: true, txHash: result.hash, claimed: 1, symbol: tokenSymbol };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;

        // Handle Cooldown
        if (error.message.includes("Cooldown active")) {
            if (!silent) console.log(`${COLORS.dim}Cooldown active for this faucet.${COLORS.reset}`);
            logWalletAction(workerId, walletIndex, wallet.address, 'ClaimFaucet', 'skipped', 'Cooldown active', silent, duration, proxy);
            return { success: false, reason: 'cooldown_active' };
        }

        logWalletAction(workerId, walletIndex, wallet.address, 'ClaimFaucet', 'failed', error.message.substring(0, 50), silent, duration, proxy);
        if (!silent) console.log(`${COLORS.fg.red}✗ Claim failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}
