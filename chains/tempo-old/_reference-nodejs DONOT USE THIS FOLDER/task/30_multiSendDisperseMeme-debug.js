import { multiSendDisperseMemeForWallet } from './30_multiSendDisperseMeme.js';
import { getWalletFiles, getPrivateKeyFromFile, getWallet } from '../utils/wallet.js';
import { COLORS } from '../utils/constants.js';
import { TempoInspector } from '../utils/tempoInspector.js';
import { askPassword } from '../utils/helpers.js';

async function main() {
    console.log(`  ${COLORS.fg.magenta}🐛  DEBUG MODE: Task 30 - MultiSend Disperse Meme${COLORS.reset}\n`);

    // 1. Get Random Wallet File
    const walletFiles = getWalletFiles();
    if (walletFiles.length === 0) {
        console.error("No wallet files found in wallets/ directory!");
        return;
    }

    const randomIndex = Math.floor(Math.random() * walletFiles.length);
    const selectedFile = walletFiles[randomIndex];
    let password = process.env.WALLET_PASSWORD || "password";
    let privateKey;

    console.log(`${COLORS.fg.cyan}Selected Random Wallet: ${selectedFile} (Index ${randomIndex + 1}/${walletFiles.length})${COLORS.reset}`);

    // Decrypt ONLY this wallet
    try {
        privateKey = getPrivateKeyFromFile(selectedFile, password);
    } catch (e) {
        console.log(`${COLORS.dim}Default password failed. Asking...${COLORS.reset}`);
        password = await askPassword("Enter encryption password: ");
        privateKey = getPrivateKeyFromFile(selectedFile, password);
    }

    if (!privateKey) throw new Error("Failed to decrypt private key");

    const { wallet, proxy } = await getWallet(0, privateKey);

    console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
    console.log(`${COLORS.fg.magenta}WALLET: ${wallet.address}${COLORS.reset}`);
    if (proxy) console.log(`${COLORS.dim}Proxy: ${proxy}${COLORS.reset}`);

    // 2. Execute Task Once
    const result = await multiSendDisperseMemeForWallet(wallet, proxy, 'DEBUG', 0, false);

    if (result?.success && result?.txHash) {
        await TempoInspector.logReport(result.txHash, { proxy });
    }

    console.log(`\n${COLORS.fg.green}✓ Debug task completed.${COLORS.reset}`);
}

main().catch(console.error);
