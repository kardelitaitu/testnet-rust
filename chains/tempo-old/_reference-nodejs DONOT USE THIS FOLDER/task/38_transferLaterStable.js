import { ethers } from 'ethers';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, getRandomInt } from '../utils/helpers.js';
import { mintRandomTokenForWallet } from './7_mintStable.js';
import { TempoSDKService } from '../utils/tempoService.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/* CONFIGURATION */
const AMOUNT_MIN = 100;
const AMOUNT_MAX = 500;
const DELAY_MIN = 10;
const DELAY_MAX = 3500;

const TIP20_ABI = [
    "function balanceOf(address account) view returns (uint256)",
    "function decimals() view returns (uint8)",
    "function symbol() view returns (string)",
    "function transfer(address to, uint256 amount) returns (bool)"
];

export async function transferLaterStableForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const actionName = '38_transferLaterStable.js';

    try {
        // 1. Load Stable Tokens for this wallet
        const dataPath = path.join(process.cwd(), 'data', 'created_tokens.json');
        let availableTokens = [];

        if (fs.existsSync(dataPath)) {
            const data = JSON.parse(fs.readFileSync(dataPath, 'utf-8'));
            const myTokens = data[wallet.address] || [];
            availableTokens = myTokens.map(t => [t.symbol, t.token]);
        }

        if (availableTokens.length === 0) {
            if (!silent) console.log(`${COLORS.fg.yellow}⚠ No stable tokens found. Auto-creating/minting first...${COLORS.reset}`);
            await mintRandomTokenForWallet(wallet, proxy, workerId, walletIndex, true);
            return { success: false, reason: 'no_stables_minting_now' };
        }

        // 2. Select Funded Token
        const balanceChecks = await Promise.all(availableTokens.slice(0, 10).map(async ([name, addr]) => {
            try {
                const contract = new ethers.Contract(addr, TIP20_ABI, wallet);
                const bal = await contract.balanceOf(wallet.address);
                return { name, addr, balance: bal };
            } catch (e) { return { name, addr, balance: 0n }; }
        }));

        const fundedTokens = balanceChecks.filter(t => t.balance > 0n);

        if (fundedTokens.length === 0) {
            if (!silent) console.log(`${COLORS.fg.yellow}⚠ No funded stables. Auto-minting...${COLORS.reset}`);
            await mintRandomTokenForWallet(wallet, proxy, workerId, walletIndex, true);
            throw new Error("No funded stable tokens. Minting initiated, retry next loop.");
        }

        const selected = fundedTokens[Math.floor(Math.random() * fundedTokens.length)];
        const [tokenName, tokenAddress] = [selected.name, selected.addr];

        const tokenContract = new ethers.Contract(tokenAddress, TIP20_ABI, wallet);
        let decimals = 18;
        try { decimals = await tokenContract.decimals(); } catch (e) { }

        // 3. Prepare Transfer Details
        const amount = (Math.random() * (AMOUNT_MAX - AMOUNT_MIN) + AMOUNT_MIN).toFixed(2);
        const amountWei = ethers.parseUnits(amount, decimals);
        const recipientAddress = ethers.Wallet.createRandom().address;
        const delaySeconds = getRandomInt(DELAY_MIN, DELAY_MAX);
        const executeAt = new Date(Date.now() + delaySeconds * 1000);

        if (!silent) {
            console.log(`${COLORS.fg.yellow}🎲 Selected Stable: ${tokenName}${COLORS.reset}`);
            console.log(`${COLORS.fg.yellow}Scheduling ${amount} ${tokenName} to ${recipientAddress.substring(0, 10)}... for ${executeAt.toLocaleString()}...${COLORS.reset}`);
        }

        // 4. Detect Fee Token
        const SYSTEM_TOKENS = ['PathUSD', 'AlphaUSD', 'BetaUSD', 'ThetaUSD'];
        let selectedFeeToken = 'PathUSD';
        for (const sym of SYSTEM_TOKENS) {
            try {
                const fa = CONFIG.TOKENS[sym];
                if (!fa) continue;
                const fc = new ethers.Contract(fa, ["function balanceOf(address) view returns (uint256)"], wallet);
                if (await fc.balanceOf(wallet.address) > 0n) {
                    selectedFeeToken = sym;
                    break;
                }
            } catch (e) { }
        }

        // 5. Execute Schedule
        const tempoService = new TempoSDKService(wallet);
        const result = await tempoService.createScheduledTransfer(
            tokenAddress,
            amountWei,
            recipientAddress,
            executeAt,
            selectedFeeToken
        );

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, actionName, 'success', `${amount} ${tokenName} scheduled in ${delaySeconds}s`, silent, duration, proxy);

        return {
            success: true,
            txHash: result.transactionHash,
            symbol: tokenName,
            tokenAddress,
            amount,
            executeAt
        };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, actionName, 'failed', error.message.substring(0, 50), silent, duration, proxy);
        if (!silent) console.log(`${COLORS.fg.red}✗ Failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}
