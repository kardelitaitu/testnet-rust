import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getWalletFiles, getPrivateKeyFromFile, getWallet } from '../utils/wallet.js';
import { askPassword } from '../utils/helpers.js';

// Correct ABI based on Python reference and fixed main task
const TIP403_REGISTRY_ABI = [
    "function createPolicy(address admin, uint8 policyType) returns (uint64)",
    "function createPolicyWithAccounts(address admin, uint8 policyType, address[] accounts) returns (uint64)",
    "function isAuthorized(uint64 policyId, address user) view returns (bool)"
];

async function deepDebugTIP403() {
    console.log(`${COLORS.fg.magenta}🕵️  DEEP DEBUG: TIP-403 Policies${COLORS.reset}\n`);

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
    const address = SYSTEM_CONTRACTS.TIP403_REGISTRY;

    console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
    console.log(`${COLORS.fg.magenta}WALLET: ${wallet.address}${COLORS.reset}`);
    if (proxy) console.log(`${COLORS.dim}Proxy: ${proxy}${COLORS.reset}`);
    console.log(`Contract: ${address}`);

    try {
        // 1. Check if contract exists
        const code = await wallet.provider.getCode(address);
        console.log(`Contract Code Length: ${code.length}`);
        if (code === '0x') {
            console.log(`${COLORS.fg.red}❌ CRITICAL: No code at contract address!${COLORS.reset}`);
            return;
        } else {
            console.log(`${COLORS.fg.green}✓ Contract exists${COLORS.reset}`);
        }

        const contract = new ethers.Contract(address, TIP403_REGISTRY_ABI, wallet);

        // 2. Estimate Gas for createPolicy
        try {
            console.log('\nEstimating Gas for createPolicy(0)...'); // 0 = Whitelist
            const policyType = 0;
            const gas = await contract.createPolicy.estimateGas(wallet.address, policyType);
            console.log(`${COLORS.fg.green}✓ Estimate Gas: ${gas.toString()}${COLORS.reset}`);
        } catch (e) {
            console.log(`${COLORS.fg.red}❌ Gas Estimation Failed${COLORS.reset}`);
            console.log(e.message);
        }

        // 3. Try simulate createPolicy
        console.log('\nSimulating createPolicy(0)...');
        try {
            const policyType = 0;
            // Static Call
            await contract.createPolicy.staticCall(wallet.address, policyType);
            console.log(`${COLORS.fg.green}✓ Simulation Success: Transaction should work${COLORS.reset}`);
        } catch (e) {
            console.log(`${COLORS.fg.red}❌ Simulation Failed (StaticCall)${COLORS.reset}`);
            console.log(`Reason: ${e.reason}`);
        }

    } catch (error) {
        console.error('Unexpected Global Error:', error);
    }
}

deepDebugTIP403().catch(console.error);
