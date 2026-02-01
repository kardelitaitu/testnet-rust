import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { ethers } from 'ethers';
import { COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { TempoSDKService } from '../utils/tempoService.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const TRACKER_FILE = path.join(ROOT_DIR, 'data', 'splitter-contract-tracker.json');
const CREATED_TOKENS_FILE = path.join(ROOT_DIR, 'data', 'created_tokens.json');

export async function distributeSharesStable(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    try {
        if (!silent) console.log(`${COLORS.fg.cyan}Task 41: Distribute Created Stable Token${COLORS.reset}`);

        // 1. Files Check
        if (!fs.existsSync(TRACKER_FILE) || !fs.existsSync(CREATED_TOKENS_FILE)) {
            const msg = "Tracker or Tokens file missing";
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) console.log(msg);
            logWalletAction(workerId, walletIndex, wallet.address, 'DistributeStable', 'failed', msg, silent, duration);
            return { success: false, reason: 'files_missing' };
        }

        const tracker = JSON.parse(fs.readFileSync(TRACKER_FILE));
        const allCreated = JSON.parse(fs.readFileSync(CREATED_TOKENS_FILE));
        const walletTokens = allCreated[wallet.address] || [];

        if (tracker.length === 0 || walletTokens.length === 0) {
            const msg = "No contracts or no created tokens";
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) console.log(msg);
            logWalletAction(workerId, walletIndex, wallet.address, 'DistributeStable', 'failed', msg, silent, duration);
            return { success: false, reason: 'no_data' };
        }

        // 2. Scan (Optimized: 5 Random)
        const tokensToScan = walletTokens.sort(() => 0.5 - Math.random()).slice(0, 5);
        const fundedTokens = [];

        for (const tInfo of tokensToScan) {
            try {
                const tContract = new ethers.Contract(tInfo.token, [
                    "function transfer(address to, uint256 amount) returns (bool)",
                    "function balanceOf(address account) view returns (uint256)",
                    "function decimals() view returns (uint8)",
                    "function symbol() view returns (string)"
                ], wallet);

                const bal = await tContract.balanceOf(wallet.address);
                if (bal === 0n) continue;

                let decimals = 18;
                try { decimals = await tContract.decimals(); } catch (e) { }

                const formatted = ethers.formatUnits(bal, decimals);
                // Threshold: 1000.0
                if (parseFloat(formatted) >= 1000.0) {
                    fundedTokens.push({ symbol: tInfo.symbol, address: tInfo.token, decimals, contract: tContract, balance: bal });
                }
            } catch (e) { }
        }

        if (fundedTokens.length === 0) {
            const msg = "No funded stable tokens (>1000)";
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) console.log(msg);
            logWalletAction(workerId, walletIndex, wallet.address, 'DistributeStable', 'failed', msg, silent, duration);
            return { success: false, reason: 'no_funds' };
        }

        // 3. Select
        const selectedToken = fundedTokens[Math.floor(Math.random() * fundedTokens.length)];
        const record = tracker[Math.floor(Math.random() * tracker.length)];
        const service = new TempoSDKService(wallet);

        // 4. Send 10%
        const amountToSend = selectedToken.balance / 10n; // BigInt division
        const formattedAmount = ethers.formatUnits(amountToSend, selectedToken.decimals);

        const transferData = selectedToken.contract.interface.encodeFunctionData('transfer', [record.address, amountToSend]);

        if (!silent) console.log(`${COLORS.dim}Sending 10% (${formattedAmount} ${selectedToken.symbol}) to ${record.address}${COLORS.reset}`);

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
        }], 'PathUSD', 10000000);

        const receipt = await wallet.provider.waitForTransaction(distRes.transactionHash);
        const duration = (Date.now() - startTime) / 1000;

        if (receipt.status === 1) {
            logWalletAction(workerId, walletIndex, wallet.address, 'DistributeStable', 'success', `Distributed ${formattedAmount} ${selectedToken.symbol}`, silent, duration);
            return { success: true, txHash: distRes.transactionHash, token: selectedToken.symbol, amount: formattedAmount };
        } else {
            logWalletAction(workerId, walletIndex, wallet.address, 'DistributeStable', 'failed', 'Tx Reverted', silent, duration);
            return { success: false, reason: 'reverted' };
        }

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        if (!silent) console.error(error);
        logWalletAction(workerId, walletIndex, wallet.address, 'DistributeStable', 'failed', error.message.substring(0, 50), silent, duration);
        return { success: false, reason: error.message };
    }
}
