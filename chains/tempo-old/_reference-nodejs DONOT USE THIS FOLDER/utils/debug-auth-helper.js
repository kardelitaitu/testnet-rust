
import { COLORS } from './constants.js';
import { getWallet, getWalletFiles, getPrivateKeyFromFile } from './wallet.js';
import { askPassword, askQuestion } from './helpers.js';
import { getValidProxies } from './proxies.js';

export async function getDebugAuth() {
    // 1. Wallet Selection
    const walletFiles = getWalletFiles();
    if (walletFiles.length === 0) {
        console.log(`${COLORS.fg.red}No wallet files found!${COLORS.reset}`);
        process.exit(1);
    }

    // 2. Password Logic
    let password = 'password';
    let privateKey;
    let selectedFile;
    let selectedIndex;

    // Try default password first with a random file just to test password validity? 
    // Or just proceed to selection. If password is wrong, getPrivateKeyFromFile will throw or return null usually, 
    // but our wrapper might catch it.

    // We'll trust 'password' is correct for now or handle failure downstream.

    // 3. User Choice (CLI Argument or Random)
    // Check for --index=X or -i X
    let useIndex = -1;
    const args = process.argv.slice(2);
    for (let i = 0; i < args.length; i++) {
        if (args[i].startsWith('--index=')) {
            useIndex = parseInt(args[i].split('=')[1]);
        } else if (args[i] === '--index' || args[i] === '-i') {
            useIndex = parseInt(args[i + 1]);
        }
    }

    if (useIndex >= 0 && useIndex < walletFiles.length) {
        selectedIndex = useIndex;
        console.log(`${COLORS.fg.cyan}Using wallet index from CLI: ${selectedIndex}${COLORS.reset}`);
    } else {
        // Default to random
        selectedIndex = Math.floor(Math.random() * walletFiles.length);
        console.log(`${COLORS.dim}No valid index flag provided. Using random wallet index: ${selectedIndex}${COLORS.reset}`);
    }

    selectedFile = walletFiles[selectedIndex];

    try {
        privateKey = getPrivateKeyFromFile(selectedFile, password);
    } catch (e) {
        // Fallback if default password fails
        console.log(`${COLORS.fg.yellow}Default password failed. Please enter password manually.${COLORS.reset}`);
        password = await askPassword();
        privateKey = getPrivateKeyFromFile(selectedFile, password);
    }

    // 4. Proxy Selection
    const proxies = getValidProxies();
    const proxy = proxies.length > 0 ? proxies[Math.floor(Math.random() * proxies.length)] : null;
    const { wallet } = await getWallet(selectedIndex, privateKey, proxy);

    console.log(`[DEBUG] Initialized Wallet: ${wallet.address}`);
    console.log(`[DEBUG] Proxy: ${proxy || 'Direct'}`);

    return {
        wallet,
        proxy,
        workerId: 1,
        walletIndex: selectedIndex
    };
}
