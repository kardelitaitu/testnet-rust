import { ethers } from 'ethers';
import { getWalletFiles, getPrivateKeyFromFile, getWallet, loadCreatedTokens } from '../utils/wallet.js';
import { COLORS } from '../utils/constants.js';
import { askPassword } from '../utils/helpers.js';

const DEBUG_ABI = [
    "function grantRole(bytes32 role, address account)",
    "function hasRole(bytes32 role, address account) view returns (bool)",
    "function getRoleAdmin(bytes32 role) view returns (bytes32)",
    "function DEFAULT_ADMIN_ROLE() view returns (bytes32)"
];

async function deepDebugGrantRole() {
    console.log(`${COLORS.fg.magenta}🕵️  DEEP DEBUG: Grant Role${COLORS.reset}\n`);

    // 1. Password
    let password = process.env.WALLET_PASSWORD || "password";

    // We'll use getWalletFiles to search for a wallet with tokens faster
    const walletFiles = getWalletFiles();
    const tokens = loadCreatedTokens();
    let targetWallet = null;
    let targetToken = null;
    let targetIndex = 0;
    let targetProxy = null;

    console.log(`${COLORS.dim}Searching for a wallet with existing tokens...${COLORS.reset}`);

    for (let i = 0; i < walletFiles.length; i++) {
        const file = walletFiles[i];
        let pk;
        try {
            pk = getPrivateKeyFromFile(file, password);
        } catch (e) {
            // If default fails once, we might want to ask once, but for searching, we just skip or ask?
            // Usually searching all files with wrong password is slow. 
            // We'll skip for search, and only ask if we really need it.
            continue;
        }

        const { wallet, proxy } = await getWallet(0, pk);
        const addr = ethers.getAddress(wallet.address);
        const myTokens = tokens[addr] || [];
        if (myTokens.length > 0) {
            targetWallet = wallet;
            targetToken = myTokens[0];
            targetIndex = i;
            targetProxy = proxy;
            break;
        }
    }

    if (!targetWallet) {
        console.log(`${COLORS.fg.red}❌ No wallet with existing tokens found in wallets/ directory.${COLORS.reset}`);
        return;
    }

    console.log(`Wallet: ${targetWallet.address}`);
    console.log(`Token: ${targetToken.token} (${targetToken.symbol})`);
    console.log('────────────────────────────────────────────');

    const contract = new ethers.Contract(targetToken.token, DEBUG_ABI, targetWallet);

    const DEFAULT_ADMIN_ROLE = ethers.ZeroHash; // 0x00...00
    const ISSUER_ROLE = ethers.id("ISSUER_ROLE");
    const PAUSE_ROLE = ethers.id("PAUSE_ROLE");

    // 1. Check Admin Role
    try {
        const isAdmin = await contract.hasRole(DEFAULT_ADMIN_ROLE, targetWallet.address);
        console.log(`Has DEFAULT_ADMIN_ROLE? ${isAdmin ? '✅ YES' : '❌ NO'}`);
    } catch (e) {
        console.log(`Check DEFAULT_ADMIN_ROLE failed: ${e.message}`);
    }

    // 2. Check Role Admins
    try {
        const issuerAdmin = await contract.getRoleAdmin(ISSUER_ROLE);
        console.log(`Admin of ISSUER_ROLE: ${issuerAdmin} (Is Default? ${issuerAdmin === DEFAULT_ADMIN_ROLE})`);

        const pauseAdmin = await contract.getRoleAdmin(PAUSE_ROLE);
        console.log(`Admin of PAUSE_ROLE: ${pauseAdmin} (Is Default? ${pauseAdmin === DEFAULT_ADMIN_ROLE})`);
    } catch (e) {
        console.log(`Check Role Admin failed: ${e.message}`);
    }

    // 3. Simulate Grant Role (ISSUER_ROLE - we likely have this)
    console.log('\nSimulating Grant ISSUER_ROLE...');
    try {
        await contract.grantRole.staticCall(ISSUER_ROLE, targetWallet.address);
        console.log('✅ Grant Simulation Success');
    } catch (e) {
        console.log('❌ Grant Simulation Failed');
        console.log(`Reason: ${e.reason}`);
        console.log(`Code: ${e.code}`);
        if (e.invocation) console.log(`Method: ${e.invocation.method}`);
    }
}

deepDebugGrantRole().catch(console.error);
