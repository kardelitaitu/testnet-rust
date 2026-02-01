
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { ethers } from 'ethers';
import { COLORS, CONFIG } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { ConcurrentService } from '../utils/tempoConcurrent.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const TRACKER_FILE = path.join(ROOT_DIR, 'data', 'splitter-contract-tracker.json');
const CREATED_MEMES_FILE = path.join(ROOT_DIR, 'data', 'created_memes.json');

export async function distributeSharesMeme(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    try {
        if (!silent) console.log(`${COLORS.fg.cyan}Task 42: Distribute Created Meme Token (Atomic)${COLORS.reset}`);

        // 1. Files Check
        if (!fs.existsSync(TRACKER_FILE) || !fs.existsSync(CREATED_MEMES_FILE)) {
            const msg = "Tracker or Memes file missing";
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) console.log(msg);
            logWalletAction(workerId, walletIndex, wallet.address, 'DistributeMeme', 'failed', msg, silent, duration);
            return { success: false, reason: 'files_missing' };
        }

        const tracker = JSON.parse(fs.readFileSync(TRACKER_FILE));
        const allCreated = JSON.parse(fs.readFileSync(CREATED_MEMES_FILE));
        const walletTokens = allCreated[wallet.address] || [];

        if (tracker.length === 0 || walletTokens.length === 0) {
            const msg = "No contracts or no created memes";
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) console.log(msg);
            logWalletAction(workerId, walletIndex, wallet.address, 'DistributeMeme', 'failed', msg, silent, duration);
            return { success: false, reason: 'no_data' };
        }

        // 2. Scan (Optimized: Parallel Scan of 5 Random Candidates)
        const tokensToScan = walletTokens.sort(() => 0.5 - Math.random()).slice(0, 5);

        // Parallel Balance Checks
        const balancePromises = tokensToScan.map(async (tInfo) => {
            try {
                const tContract = new ethers.Contract(tInfo.token, [
                    "function balanceOf(address account) view returns (uint256)",
                    "function decimals() view returns (uint8)"
                ], wallet);
                const bal = await tContract.balanceOf(wallet.address);
                if (bal > 0n) {
                    let decimals = 18;
                    try { decimals = await tContract.decimals(); } catch (e) { }
                    const formatted = ethers.formatUnits(bal, decimals);
                    if (parseFloat(formatted) >= 1000.0) {
                        return {
                            symbol: tInfo.symbol,
                            address: tInfo.token,
                            decimals,
                            balance: bal
                        };
                    }
                }
            } catch (e) { }
            return null;
        });

        const results = await Promise.all(balancePromises);
        const fundedTokens = results.filter(r => r !== null);

        if (fundedTokens.length === 0) {
            const msg = "No funded meme tokens (>1000)";
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) console.log(msg);
            logWalletAction(workerId, walletIndex, wallet.address, 'DistributeMeme', 'failed', msg, silent, duration);
            return { success: false, reason: 'no_funds' };
        }

        // 3. Select Random Target
        const selectedToken = fundedTokens[Math.floor(Math.random() * fundedTokens.length)];
        const record = tracker[Math.floor(Math.random() * tracker.length)];

        // 4. Prepare Atomic Batch
        const service = new ConcurrentService(wallet.privateKey, proxy);

        // Calculate 10%
        const amountToSend = selectedToken.balance / 10n;
        const formattedAmount = ethers.formatUnits(amountToSend, selectedToken.decimals);

        if (!silent) console.log(`${COLORS.dim}Distributing ${formattedAmount} ${selectedToken.symbol} to ${record.address}${COLORS.reset}`);

        // Call 1: Transfer Token -> Splitter
        const tokenInterface = new ethers.Interface(["function transfer(address to, uint256 amount)"]);
        const transferData = tokenInterface.encodeFunctionData('transfer', [record.address, amountToSend]);

        // Call 2: Trigger Distribute on Splitter
        const splitterInterface = new ethers.Interface(["function distribute(address token)"]);
        const distData = splitterInterface.encodeFunctionData('distribute', [selectedToken.address]);

        const calls = [
            { to: selectedToken.address, data: transferData, value: 0n },
            { to: record.address, data: distData, value: 0n }
        ];

        // Execute Atomic Batch
        // Use CONFIG.TOKENS.PathUSD as fee token typically, or allow default
        const txHash = await service.sendAtomicBatch(calls, Date.now(), CONFIG.TOKENS.PathUSD, { gas: 1000000n });

        // Wait for confirmation
        const receipt = await service.publicClient.waitForTransactionReceipt({ hash: txHash });

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'DistributeMeme', 'success', `Distributed ${formattedAmount} ${selectedToken.symbol}`, silent, duration);

        return { success: true, txHash: txHash, token: selectedToken.symbol, amount: formattedAmount };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        if (!silent) console.error(error);
        logWalletAction(workerId, walletIndex, wallet.address, 'DistributeMeme', 'failed', error.message.substring(0, 50), silent, duration);
        return { success: false, reason: error.message };
    }
}
