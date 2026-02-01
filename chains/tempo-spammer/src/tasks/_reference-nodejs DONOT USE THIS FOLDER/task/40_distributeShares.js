import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { TempoSDKService } from '../utils/tempoService.js';
import { getRandomInt } from '../utils/helpers.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const TRACKER_FILE = path.join(ROOT_DIR, 'data', 'splitter-contract-tracker.json');

export async function distributeShares(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    try {
        if (!silent) console.log(`${COLORS.fg.cyan}Task 40: Distribute System Token${COLORS.reset}`);

        // 1. Load Tracker
        if (!fs.existsSync(TRACKER_FILE)) {
            const msg = "Tracker file not found";
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) console.log(msg);
            logWalletAction(workerId, walletIndex, wallet.address, 'DistributeSystem', 'failed', msg, silent, duration);
            return { success: false, reason: 'tracker_missing' };
        }

        const tracker = JSON.parse(fs.readFileSync(TRACKER_FILE));
        if (tracker.length === 0) {
            const msg = "No deployed contracts in tracker";
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) console.log(msg);
            logWalletAction(workerId, walletIndex, wallet.address, 'DistributeSystem', 'failed', msg, silent, duration);
            return { success: false, reason: 'tracker_empty' };
        }

        // 2. Scan for Funded System Tokens (Minimal ABI)
        const fundedTokens = [];

        // Scan a shuffled subset to be efficient? No, system tokens are few, scan all.
        for (const [symbol, address] of Object.entries(CONFIG.TOKENS)) {
            try {
                // We only need minimal ABI to check balance
                const tContract = new ethers.Contract(address, [
                    "function balanceOf(address account) view returns (uint256)",
                    "function decimals() view returns (uint8)"
                ], wallet);

                const bal = await tContract.balanceOf(wallet.address);
                if (bal === 0n) continue;

                let decimals = 18;
                try { decimals = await tContract.decimals(); } catch (e) { }

                const formatted = ethers.formatUnits(bal, decimals);
                // Threshold: 10.0
                if (parseFloat(formatted) >= 10.0) {
                    fundedTokens.push({ symbol, address, decimals, contract: tContract, balance: formatted });
                }
            } catch (e) {
                // Ignore failures (bad RPC, etc)
            }
        }

        if (fundedTokens.length === 0) {
            const msg = "No funded system tokens (>10.0)";
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) console.log(msg);
            logWalletAction(workerId, walletIndex, wallet.address, 'DistributeSystem', 'failed', msg, silent, duration);
            return { success: false, reason: 'no_funds' };
        }

        // 3. Select Token & Contract
        const selectedToken = fundedTokens[Math.floor(Math.random() * fundedTokens.length)];
        const record = tracker[Math.floor(Math.random() * tracker.length)];
        const service = new TempoSDKService(wallet);

        // 4. Send Funds (10k - 30k)
        const randomAmount = (getRandomInt(0, 200) * 100) + 10000; // 10000, 10100... 30000
        const amountToDistribute = ethers.parseUnits(randomAmount.toString(), selectedToken.decimals);

        // minimal interface for transfer
        const transferData = new ethers.Interface(["function transfer(address to, uint256 amount)"]).encodeFunctionData('transfer', [record.address, amountToDistribute]);

        if (!silent) console.log(`${COLORS.dim}Sending ${randomAmount} ${selectedToken.symbol} to ${record.address}${COLORS.reset}`);

        // We use batching to do Fund + Distribute? No, separate for safety/clarity, or combine?
        // Separate ensures contract has funds before distribute call, though atomic batch works too.
        // Let's do Standard Send First.

        const fundRes = await service.sendBatchTransaction([{
            to: selectedToken.address,
            value: 0n,
            data: transferData
        }], 'PathUSD');

        await wallet.provider.waitForTransaction(fundRes.transactionHash);

        // 5. Trigger Distribute
        const splitterInterface = new ethers.Interface(["function distribute(address token)"]);
        const distData = splitterInterface.encodeFunctionData('distribute', [selectedToken.address]);

        const distRes = await service.sendBatchTransaction([{
            to: record.address,
            value: 0n,
            data: distData
        }], 'PathUSD', 10000000); // High Gas Limit

        const receipt = await wallet.provider.waitForTransaction(distRes.transactionHash);
        const duration = (Date.now() - startTime) / 1000;

        if (receipt.status === 1) {
            logWalletAction(workerId, walletIndex, wallet.address, 'DistributeSystem', 'success', `Distributed ${randomAmount} ${selectedToken.symbol}`, silent, duration);
            return { success: true, txHash: distRes.transactionHash, token: selectedToken.symbol, amount: randomAmount };
        } else {
            logWalletAction(workerId, walletIndex, wallet.address, 'DistributeSystem', 'failed', 'Tx Reverted', silent, duration);
            return { success: false, reason: 'reverted' };
        }

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        if (!silent) console.error(error);
        logWalletAction(workerId, walletIndex, wallet.address, 'DistributeSystem', 'failed', error.message.substring(0, 50), silent, duration);
        return { success: false, reason: error.message };
    }
}
