import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { TempoSDKService } from '../utils/tempoService.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const TIP20_ABI = [
    "function transfer(address to, uint256 amount) returns (bool)",
    "function balanceOf(address account) view returns (uint256)",
    "function decimals() view returns (uint8)"
];

// Configurable Ranges
const RECIPIENT_COUNT_RANGE = { min: 10, max: 20 };
const TRANSFER_AMOUNT_RANGE = { min: 300.0, max: 2000.0 };

function getRandomInt(min, max) { return Math.floor(Math.random() * (max - min + 1)) + min; }

/**
 * Task 34: Native Batch Transactions (System Tokens)
 * Uses Tempo's native protocol-level batching (calls vector) for atomic execution via TempoSDKService.
 */
export async function batchSendTransactionForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    let actionName = '34_batchSendTransaction.js';

    try {
        const address = wallet.address;

        // 1. Select Funded System Token
        const availableTokens = Object.entries(CONFIG.TOKENS);
        const balanceChecks = await Promise.all(availableTokens.map(async ([name, addr]) => {
            try {
                const contract = new ethers.Contract(addr, ["function balanceOf(address) view returns (uint256)", "function decimals() view returns (uint8)"], wallet);
                const bal = await contract.balanceOf(address);
                let dec = 18;
                try { dec = Number(await contract.decimals()); } catch (e) { }
                return { name, addr, balance: bal, decimals: dec };
            } catch (e) { return { name, addr, balance: 0n, decimals: 18 }; }
        }));

        const fundedTokens = balanceChecks.filter(t => t.balance > 0n);
        if (fundedTokens.length === 0) throw new Error("No system tokens with balance found.");

        const selected = fundedTokens[Math.floor(Math.random() * fundedTokens.length)];
        const { name: tokenName, addr: tokenAddress, decimals: tokenDecimals, balance: tokenBalance } = selected;

        if (!silent) console.log(`${COLORS.fg.cyan}[Worker ${workerId}] Native Batch using ${tokenName} (Balance: ${ethers.formatUnits(tokenBalance, tokenDecimals)})${COLORS.reset}`);

        // 2. Load Recipients
        const txtPath = path.join(__dirname, '28_multiSendDisperse.txt');
        let allAddresses = [];
        if (fs.existsSync(txtPath)) {
            allAddresses = fs.readFileSync(txtPath, 'utf-8').split(/\r?\n/).map(l => l.trim()).filter(l => l && ethers.isAddress(l));
        }
        if (allAddresses.length === 0) {
            for (let i = 0; i < 20; i++) allAddresses.push(ethers.Wallet.createRandom().address);
        }

        const targetCount = getRandomInt(RECIPIENT_COUNT_RANGE.min, RECIPIENT_COUNT_RANGE.max);
        const selectedRecipients = allAddresses.sort(() => 0.5 - Math.random()).slice(0, targetCount);

        // 3. Prepare Batch Calls
        const iface = new ethers.Interface(TIP20_ABI);

        // Generate random amounts first
        let amountsArr = selectedRecipients.map(() => {
            return Math.random() * (TRANSFER_AMOUNT_RANGE.max - TRANSFER_AMOUNT_RANGE.min) + TRANSFER_AMOUNT_RANGE.min;
        });

        // Sum and Check Balance
        const totalNeededVal = amountsArr.reduce((a, b) => a + b, 0);
        let totalNeededWei = ethers.parseUnits(totalNeededVal.toFixed(tokenDecimals > 6 ? 2 : 0), tokenDecimals);

        // Safety Buffer: Use 95% of balance to avoid dust issues or gas overlap
        const safeBalance = (tokenBalance * 95n) / 100n;

        // Scale Down if needed
        if (totalNeededWei > safeBalance && safeBalance > 0n) {
            if (!silent) console.log(`${COLORS.fg.yellow}⚠ Scaling batch amounts to fit balance (${ethers.formatUnits(safeBalance, tokenDecimals)})...${COLORS.reset}`);
            const scaleFactor = Number(safeBalance) / Number(totalNeededWei); // lossy but fine for logic
            amountsArr = amountsArr.map(a => a * scaleFactor);
        } else if (safeBalance === 0n) {
            // Should be caught by earlier check, but safe guard
            throw new Error("Insufficient balance for any transfer");
        }

        const payments = selectedRecipients.map((to, idx) => {
            const amountVal = amountsArr[idx];
            // decimals > 6 check for toFixed logic from original
            const amount = ethers.parseUnits(amountVal.toFixed(tokenDecimals > 6 ? 6 : 0), tokenDecimals);

            return {
                to: tokenAddress,
                data: iface.encodeFunctionData('transfer', [to, amount]),
                value: 0n
            };
        });

        // 4. Send Atomic Batch Transaction via TempoSDKService
        if (!silent) console.log(`${COLORS.fg.magenta}🚀 Sending ${payments.length} calls in ONE atomic transaction...${COLORS.reset}`);

        const tempoService = new TempoSDKService(wallet);
        const { transactionHash } = await tempoService.sendBatchTransaction(payments, 'PathUSD');

        if (!silent) console.log(`${COLORS.dim}Transaction Hash: ${transactionHash}${COLORS.reset}`);

        // 5. Wait for Receipt (Polling to avoid Ethers parsing error on Type 0x76)
        if (!silent) console.log(`${COLORS.dim}Waiting for confirmation...${COLORS.reset}`);
        const provider = wallet.provider;
        let receipt = null;
        let attempts = 0;
        while (!receipt && attempts < 60) { // Wait up to 2 mins
            try {
                receipt = await provider.getTransactionReceipt(transactionHash);
            } catch (e) { }
            if (!receipt) {
                await new Promise(r => setTimeout(r, 2000));
                attempts++;
            }
        }

        if (!receipt) throw new Error("Transaction timed out");

        const duration = (Date.now() - startTime) / 1000;

        if (receipt.status === 1) {
            logWalletAction(workerId, walletIndex, address, actionName, 'success', `Batched ${payments.length} txs (${duration.toFixed(1)}s)`, silent, duration);
            if (!silent) console.log(`${COLORS.fg.green}✓ Success! Atomic batch confirmed.${COLORS.reset}`);
            return { success: true, txHash: transactionHash, count: payments.length, duration: duration.toFixed(1) };
        } else {
            throw new Error("Transaction reverted");
        }

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, actionName, 'failed', error.message.substring(0, 50), silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Batch Failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message, duration: duration.toFixed(1) };
    }
}