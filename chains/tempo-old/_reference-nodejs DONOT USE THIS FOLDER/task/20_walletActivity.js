import fs from 'fs';
import path from 'path';
import { COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { countdown, runWithRetry } from '../utils/helpers.js';
import { getTaskCount } from '../utils/wallet.js';

export async function runWalletActivityMenu() {
    console.log(`\n  ${COLORS.fg.magenta}📝  WALLET ACTIVITY MODULE${COLORS.reset}\n`);
    console.log(`${COLORS.fg.yellow}View wallet activity and transaction logs${COLORS.reset}\n`);
    console.log(`${COLORS.fg.cyan}This module is for viewing data only.${COLORS.reset}`);
    console.log(`${COLORS.dim}Not suitable for the automatic runner.${COLORS.reset}\n`);
    console.log(`${COLORS.dim}Features:${COLORS.reset}`);
    console.log(`${COLORS.dim}  - Recent transactions${COLORS.reset}`);
    console.log(`${COLORS.dim}  - Activity timeline${COLORS.reset}`);
    console.log(`${COLORS.dim}  - Success/failure rates${COLORS.reset}`);
    console.log(`${COLORS.dim}  - Detailed logs${COLORS.reset}\n`);

}

// Placeholder function for automatic runner compatibility
// This module is read-only and doesn't perform transactions
export async function walletActivityForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    let blockchainTx = 0;
    let taskRuns = 0;

    try {
        // 1. Get task runs from wallet.js
        taskRuns = getTaskCount(wallet.address);

        // 2. Get actual on-chain transaction count (Nonce)
        // 2. Get actual on-chain transaction count (Nonce)
        // Wrapped in retry to handle 429
        blockchainTx = await runWithRetry(async () => {
            return await wallet.provider.getTransactionCount(wallet.address);
        });

        // Optional: We can still scan logs if needed, but on-chain is the source of truth
        // Keeping this variable clean for the report


        if (!silent) {
            console.log(`${COLORS.dim}[Activity] Wallet: ${wallet.address.substring(0, 10)}...${COLORS.reset}`);
            console.log(`${COLORS.dim}[Activity] Task Runs: ${taskRuns}${COLORS.reset}`);
            console.log(`${COLORS.dim}[Activity] Blockchain Transactions: ${blockchainTx}${COLORS.reset}`);
        }

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'Activity', 'success', `Task runs: ${taskRuns} Blockchain: ${blockchainTx} Tx`, silent, duration);

        return {
            success: true,
            taskRuns,
            blockchainTx,
            reason: 'read_only_activity'
        };
    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'Activity', 'failed', error.message.substring(0, 50), silent, duration);
        return { success: false, reason: error.message };
    }
}

// Not exported to automatic runner - read-only activity logs
