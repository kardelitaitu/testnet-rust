
import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet, saveCreatedMeme } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';
import { getRandomText } from '../utils/randomText.js';
import { ConcurrentService } from '../utils/tempoConcurrent.js';

const TIP20_FACTORY_ABI = [
    "function createToken(string name, string symbol, string currency, address quoteToken, address admin, bytes32 salt) returns (address)",
    "event TokenCreated(address indexed token, string name, string symbol, string currency, address quoteToken, address admin, bytes32 salt)"
];

const ERC20_ABI = [
    "function allowance(address owner, address spender) view returns (uint256)",
    "function approve(address spender, uint256 amount) returns (bool)",
    "function grantRole(bytes32 role, address account)"
];

const MINT_ABI = [
    "function mint(address to, uint256 amount)",
    "function balanceOf(address owner) view returns (uint256)",
    "function decimals() view returns (uint8)",
    "function grantRole(bytes32 role, address account)"
];

import fs from 'fs';
import path from 'path';

function getMnemonicName() {
    try {
        const filePath = path.join(process.cwd(), 'utils', 'mnemonic.txt');
        if (fs.existsSync(filePath)) {
            const content = fs.readFileSync(filePath, 'utf8');
            const lines = content.split('\n').map(l => l.trim()).filter(l => l.length > 0);
            if (lines.length > 0) {
                const word = lines[Math.floor(Math.random() * lines.length)];
                // Capitalize first letter
                return word.charAt(0).toUpperCase() + word.slice(1);
            }
        }
    } catch (e) {
        // Fallback
    }
    return generateRandomMemeName(); // Fallback if file fails
}

function generateRandomMemeName() {
    const prefixes = ['Moon', 'Doge', 'Pepe', 'Shib', 'Floki', 'Safe', 'Baby', 'Meta', 'Elon', 'Rocket'];
    const suffixes = ['Coin', 'Token', 'Finance', 'Swap', 'Protocol', 'Network', 'DAO', 'Chain', 'Verse', 'Game'];

    const prefix = prefixes[Math.floor(Math.random() * prefixes.length)];
    const suffix = suffixes[Math.floor(Math.random() * suffixes.length)];

    return `${prefix}${suffix}`;
}

function generateRandomMemeSymbol(name) {
    if (name) {
        // Try to generate symbol from name (e.g. "Apple" -> "APPL" or "APPLE")
        const upper = name.toUpperCase().replace(/[^A-Z]/g, '');
        if (upper.length >= 3 && upper.length <= 6) return upper;
        if (upper.length > 6) return upper.substring(0, 4);
    }

    // Random fallback
    const length = getRandomInt(3, 5);
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ';
    let symbol = '';
    for (let i = 0; i < length; i++) {
        symbol += chars[Math.floor(Math.random() * chars.length)];
    }
    return symbol;
}

export async function createRandomMemeForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const tokenName = getMnemonicName();
    const tokenSymbol = generateRandomMemeSymbol(tokenName);
    const currency = 'USD'; // Use USD for compatibility (symbol differentiates it as meme)
    const quoteToken = CONFIG.TOKENS.PathUSD;

    return await createMemeForWallet(wallet, proxy, tokenName, tokenSymbol, currency, quoteToken, workerId, walletIndex, silent);
}

export async function createMemeForWallet(wallet, proxy, tokenName, tokenSymbol, currency, quoteToken, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    if (!silent) console.log(`${COLORS.fg.yellow}Creating meme token: ${tokenSymbol} (Atomic Batch)...${COLORS.reset}`);

    try {
        const factoryAddress = SYSTEM_CONTRACTS.TIP20_FACTORY;
        if (!factoryAddress) throw new Error("TIP20_FACTORY address not found");

        const calls = [];
        const quoteCtx = new ethers.Contract(quoteToken, ERC20_ABI, wallet);
        const factory = new ethers.Contract(factoryAddress, TIP20_FACTORY_ABI, wallet);

        // 1. Approve Factory and Fee Manager
        const spenders = [SYSTEM_CONTRACTS.TIP20_FACTORY, SYSTEM_CONTRACTS.FEE_MANAGER];
        for (const spender of spenders) {
            const data = quoteCtx.interface.encodeFunctionData("approve", [spender, ethers.MaxUint256]);
            calls.push({ to: quoteToken, data, value: 0n });
        }

        // 2. Create Token
        const salt = ethers.hexlify(ethers.randomBytes(32));
        const createData = factory.interface.encodeFunctionData("createToken", [
            tokenName, tokenSymbol, currency, quoteToken, wallet.address, salt
        ]);
        calls.push({ to: factoryAddress, data: createData, value: 0n });

        // --- STEP 1: Approve + Create ---
        const service = new ConcurrentService(wallet.privateKey, proxy);
        const txHash1 = await service.sendAtomicBatch(calls, Date.now(), CONFIG.TOKENS.PathUSD, { gas: 5000000n });

        if (!silent) console.log(`${COLORS.dim}Batch 1 sent (Approve+Create): ${txHash1.substring(0, 20)}...${COLORS.reset}`);

        const receipt1 = await service.publicClient.waitForTransactionReceipt({ hash: txHash1 });

        // Find Token Address
        let tokenAddress = null;
        for (const log of receipt1.logs) {
            try {
                const parsedLog = factory.interface.parseLog(log);
                if (parsedLog && parsedLog.name === 'TokenCreated') {
                    tokenAddress = parsedLog.args.token;
                    break;
                }
            } catch (e) { }
        }
        if (!tokenAddress) throw new Error("Could not parse TokenCreated event from Batch 1");

        if (!silent) console.log(`${COLORS.fg.green}✓ Created: ${tokenAddress}. Sending Batch 2...${COLORS.reset}`);

        // --- STEP 2: Grant + Mint ---
        const calls2 = [];
        const tokenContract = new ethers.Contract(tokenAddress, MINT_ABI, wallet);

        const ISSUER_ROLE = ethers.id("ISSUER_ROLE");
        const grantData = tokenContract.interface.encodeFunctionData("grantRole", [ISSUER_ROLE, wallet.address]);
        calls2.push({ to: tokenAddress, data: grantData, value: 0n });

        const mintAmount = ethers.parseUnits('100000', 6);
        const mintData = tokenContract.interface.encodeFunctionData("mint", [wallet.address, mintAmount]);
        calls2.push({ to: tokenAddress, data: mintData, value: 0n });

        const txHash2 = await service.sendAtomicBatch(calls2, Date.now() + 1, CONFIG.TOKENS.PathUSD);

        const receipt2 = await service.publicClient.waitForTransactionReceipt({ hash: txHash2 });

        if (!silent) console.log(`${COLORS.fg.green}✓ Roles & Minted! Block: ${receipt2.blockNumber}${COLORS.reset}`);

        saveCreatedMeme(wallet.address, tokenAddress, tokenSymbol, tokenName, receipt1.blockNumber);

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'CreateMeme', 'success', `${tokenSymbol}`, silent, duration);
        return { success: true, txHash: receipt1.transactionHash, tokenAddress, symbol: tokenSymbol, tokenName, rolesGranted: true };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'CreateMeme', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Creation failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

async function grantRoles(wallet, tokenAddress, quoteTokenAddress, pendingTxs, silent = false) {
    // Deprecated in atomic batch optimized version
    return true;
}

export async function runCreateMemeMenu() {
    console.log(`\n  ${COLORS.fg.magenta}🎨  CREATE MEME TOKEN MODULE${COLORS.reset}\n`);
    const privateKeys = getPrivateKeys();
    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);
        await createRandomMemeForWallet(wallet, proxy, 1, i);
        if (i < privateKeys.length - 1) await sleep(5000);
    }
}