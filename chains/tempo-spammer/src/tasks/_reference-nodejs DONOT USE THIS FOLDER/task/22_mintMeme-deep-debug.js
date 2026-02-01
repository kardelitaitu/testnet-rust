import { ethers } from 'ethers';
import { getWalletFiles, getPrivateKeyFromFile, getWallet, loadCreatedMemes } from '../utils/wallet.js';
import { COLORS } from '../utils/constants.js';
import { askPassword } from '../utils/helpers.js';

const DEBUG_ABI = [
    "function mint(address to, uint256 amount)",
    "function hasRole(bytes32 role, address account) view returns (bool)",
    "function getRoleAdmin(bytes32 role) view returns (bytes32)",
    "function owner() view returns (address)",
    "function decimals() view returns (uint8)"
];

async function deepDebugMint() {
    console.log(`${COLORS.fg.magenta}🕵️  DEEP DEBUG: Mint Meme${COLORS.reset}\n`);

    // 1. Password
    let password = process.env.WALLET_PASSWORD || "password";

    // Search strategy
    const walletFiles = getWalletFiles();
    const memes = loadCreatedMemes();
    let targetWallet = null;
    let targetMeme = null;
    let targetIndex = 0;
    let targetProxy = null;

    console.log(`${COLORS.dim}Searching for a wallet with existing memes...${COLORS.reset}`);

    for (let i = 0; i < walletFiles.length; i++) {
        const file = walletFiles[i];
        let pk;
        try {
            pk = getPrivateKeyFromFile(file, password);
        } catch (e) {
            continue;
        }

        const { wallet, proxy } = await getWallet(0, pk);
        const addr = ethers.getAddress(wallet.address);
        const myMemes = memes[addr] || [];
        if (myMemes.length > 0) {
            targetWallet = wallet;
            targetMeme = myMemes[0];
            targetIndex = i;
            targetProxy = proxy;
            break;
        }
    }

    if (!targetWallet) {
        console.log(`${COLORS.fg.red}❌ No wallet with existing memes found in wallets/ directory.${COLORS.reset}`);
        return;
    }

    console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
    console.log(`${COLORS.fg.magenta}WALLET: ${targetWallet.address}${COLORS.reset}`);
    if (targetProxy) console.log(`${COLORS.dim}Proxy: ${targetProxy}${COLORS.reset}`);
    console.log(`Meme: ${targetMeme.token} (${targetMeme.symbol})`);
    console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);

    const contract = new ethers.Contract(targetMeme.token, DEBUG_ABI, targetWallet);

    // 1. Check Owner
    try {
        const owner = await contract.owner();
        console.log(`Owner: ${owner}`);
        console.log(`Is Wallet Owner? ${owner === targetWallet.address}`);
    } catch (e) {
        console.log(`Owner check failed (likely not Ownable): ${e.code || e.message}`);
    }

    // 2. Check Roles
    const ROLES = {
        "DEFAULT_ADMIN": ethers.ZeroHash,
        "MINTER_ROLE": ethers.id("MINTER_ROLE"),
        "ISSUER_ROLE": ethers.id("ISSUER_ROLE"),
        "PAUSER_ROLE": ethers.id("PAUSER_ROLE") // Just in case
    };

    for (const [name, hash] of Object.entries(ROLES)) {
        try {
            const has = await contract.hasRole(hash, targetWallet.address);
            console.log(`Role ${name}: ${has ? '✅ YES' : '❌ NO'}`);
            if (!has) {
                try {
                    const adminRole = await contract.getRoleAdmin(hash);
                    console.log(`  -> Admin Role: ${adminRole}`);
                } catch (e) { }
            }
        } catch (e) {
            console.log(`Role ${name} check failed: ${e.code || e.message}`);
        }
    }

    // 3. Simulate Mint
    console.log('\nSimulating Mint...');
    try {
        await contract.mint.staticCall(targetWallet.address, ethers.parseUnits("1", 6));
        console.log('✅ Mint Simulation Success');
    } catch (e) {
        console.log('❌ Mint Simulation Failed');
        console.log(`Reason: ${e.reason}`);
        console.log(`Data: ${e.data}`);
    }

    // 4. Actual execution if user wants or just end?
    // Deep debug usually just simulates. But we can add a report for the simulation if we had a tx.
    // Since deep-debug is mainly simulation, we'll keep it as is or add an optional execution.
    // For now, no actual tx in deep-debug for 22.
}

deepDebugMint().catch(console.error);
