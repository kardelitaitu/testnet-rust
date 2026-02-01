import { ethers } from 'ethers';
import { getWalletFiles, getPrivateKeyFromFile, getWallet } from '../utils/wallet.js';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { TempoInspector } from '../utils/tempoInspector.js';
import { askPassword } from '../utils/helpers.js';

const STABLECOIN_DEX_ABI = [
    "function place(address token, uint128 amount, bool isBid, int16 tick) returns (uint128 orderId)",
    "event OrderPlaced(uint128 indexed orderId, address indexed user, address indexed token, uint128 amount, bool isBid, int16 tick)"
];

const ERC20_ABI = [
    "function allowance(address owner, address spender) view returns (uint256)",
    "function approve(address spender, uint256 amount) returns (bool)",
    "function decimals() view returns (uint8)",
    "function balanceOf(address owner) view returns (uint256)"
];

async function deepDebugLimitOrder() {
    console.log(`${COLORS.fg.magenta}🕵️  DEEP DEBUG: Limit Order${COLORS.reset}\n`);

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
    const dexAddress = SYSTEM_CONTRACTS.STABLECOIN_DEX;

    console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
    console.log(`${COLORS.fg.magenta}WALLET: ${wallet.address}${COLORS.reset}`);
    if (proxy) console.log(`${COLORS.dim}Proxy: ${proxy}${COLORS.reset}`);
    console.log(`DEX: ${dexAddress}`);

    const dex = new ethers.Contract(dexAddress, STABLECOIN_DEX_ABI, wallet);

    // Pick a token (PathUSD or other)
    const tokenEntries = Object.entries(CONFIG.TOKENS).filter(([s]) => s !== 'PathUSD');
    if (tokenEntries.length === 0) { console.log('No tokens'); return; }
    const [tokenSymbol, tokenAddress] = tokenEntries[0];
    const pathUsdAddress = CONFIG.TOKENS.PathUSD;

    console.log(`Trading: ${tokenSymbol} (${tokenAddress})`);

    // Try BID (Buy Token with PathUSD) - Tick 0
    const isBid = true;
    const amount = '1.0';
    const tick = 0;

    console.log(`\n--- Simulating BID 1.0 @ Tick 0 ---`);

    // Check PathUSD allowance
    const pathUsd = new ethers.Contract(pathUsdAddress, ERC20_ABI, wallet);
    const allow = await pathUsd.allowance(wallet.address, dexAddress);
    console.log(`Allowance PathUSD: ${allow}`);
    const bal = await pathUsd.balanceOf(wallet.address);
    console.log(`Balance PathUSD: ${bal}`);

    if (bal === BigInt(0)) {
        console.log('Skipping BID simulation (No PathUSD)');
    } else {
        if (allow < ethers.parseUnits("1.0", 6)) {
            console.log('Approving PathUSD...');
            await (await pathUsd.approve(dexAddress, ethers.MaxUint256)).wait();
        }

        try {
            const amountWei = ethers.parseUnits(amount, 6);
            // Execute Real Transaction
            const tx = await dex.place(tokenAddress, amountWei, isBid, tick, { gasLimit: 3000000 });
            console.log(`Tx Sent: ${tx.hash}`);
            await tx.wait();
            console.log('✅ BID Executed SUCCESS');
            await TempoInspector.logReport(tx.hash);
        } catch (e) {
            console.log('❌ BID Simulation FAILED');
            console.log(`Reason: ${e.reason}`);
            console.log(`Data: ${e.data}`);
        }
    }

    // Try ASK (Sell Token)
    console.log(`\n--- Simulating ASK 1.0 @ Tick 0 ---`);
    const token = new ethers.Contract(tokenAddress, ERC20_ABI, wallet);
    const allowT = await token.allowance(wallet.address, dexAddress);
    console.log(`Allowance ${tokenSymbol}: ${allowT}`);
    const balT = await token.balanceOf(wallet.address);
    console.log(`Balance ${tokenSymbol}: ${balT}`);

    if (balT === BigInt(0)) {
        // Mint if possible? No, requires other tasks.
        console.log('Skipping ASK simulation (No Balance)');
    } else {
        if (allowT < ethers.parseUnits("1.0", 6)) {
            console.log(`Approving ${tokenSymbol}...`);
            await (await token.approve(dexAddress, ethers.MaxUint256)).wait();
        }

        try {
            const amountWei = ethers.parseUnits(amount, 6);
            // Execute Real ASK
            const tx = await dex.place(tokenAddress, amountWei, false, tick, { gasLimit: 3000000 });
            console.log(`ASK Tx Sent: ${tx.hash}`);
            await tx.wait();
            console.log('✅ ASK Executed SUCCESS');
            await TempoInspector.logReport(tx.hash);
        } catch (e) {
            console.log('❌ ASK Execution FAILED');
            console.log(`Reason: ${e.reason}`);
            console.log(`Data: ${e.data}`);
        }
    }

}

deepDebugLimitOrder().catch(console.error);
