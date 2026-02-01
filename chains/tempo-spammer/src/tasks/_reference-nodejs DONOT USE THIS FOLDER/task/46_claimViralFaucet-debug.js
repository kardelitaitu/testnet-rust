import { claimViralFaucetForWallet } from './46_claimViralFaucet.js';
import { getWalletFiles, getPrivateKeyFromFile, getWallet } from '../utils/wallet.js';
import { COLORS } from '../utils/constants.js';
import { TempoInspector } from '../utils/tempoInspector.js';
import { askPassword } from '../utils/helpers.js';

async function main() {
    console.log(`  ${COLORS.fg.magenta}🐛  DEBUG MODE: Task 46 - Claim Viral Faucet${COLORS.reset}\n`);

    // 1. Get Random Wallet File
    const walletFiles = getWalletFiles();
    if (walletFiles.length === 0) {
        console.error("No wallet files found in wallets/ directory!");
        return;
    }

    const MAX_RETRIES = 10;
    let attempts = 0;

    while (attempts < MAX_RETRIES) {
        attempts++;
        const randomIndex = Math.floor(Math.random() * walletFiles.length);
        const selectedFile = walletFiles[randomIndex];
        let password = process.env.WALLET_PASSWORD || "password";
        let privateKey;

        console.log(`\n${COLORS.fg.cyan}Attempt ${attempts}/${MAX_RETRIES}: Checking Wallet ${selectedFile}${COLORS.reset}`);

        // Decrypt ONLY this wallet
        try {
            privateKey = getPrivateKeyFromFile(selectedFile, password);
        } catch (e) {
            console.log(`${COLORS.dim}Default password failed. Asking...${COLORS.reset}`);
            password = await askPassword("Enter encryption password: ");
            privateKey = getPrivateKeyFromFile(selectedFile, password);
        }

        if (!privateKey) {
            console.log("Failed to decrypt, skipping...");
            continue;
        }

        const { wallet, proxy } = await getWallet(0, privateKey);
        console.log(`${COLORS.dim}Address: ${wallet.address}${COLORS.reset}`);
        if (proxy) console.log(`${COLORS.dim}Proxy: ${proxy}${COLORS.reset}`);

        // 2. Execute Task
        const result = await claimViralFaucetForWallet(wallet, proxy, 'DEBUG', 0, false);

        if (result?.success) {
            if (result.txHash) await TempoInspector.logReport(result.txHash, { proxy });
            console.log(`\n${COLORS.fg.green}✓ Debug task completed successfully on attempt ${attempts}.${COLORS.reset}`);
            return;
        } else {
            console.log(`${COLORS.fg.yellow}⚠ Task skipped/failed: ${result.reason}${COLORS.reset}`);
            if (result.reason === 'no_faucets_found') {
                console.error("No viral faucets deployed yet. Please run Task 45 first.");
                return;
            }
        }
    }

    console.log(`\n${COLORS.fg.red}✗ Could not claim faucet after ${MAX_RETRIES} attempts.${COLORS.reset}`);
}

main().catch(console.error);
