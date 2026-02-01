import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet, saveCreatedMeme } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';
import { getRandomText } from '../utils/randomText.js';

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
    "function decimals() view returns (uint8)"
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
    if (!silent) console.log(`${COLORS.fg.yellow}Creating meme token: ${tokenSymbol}...${COLORS.reset}`);

    try {
        const factoryAddress = SYSTEM_CONTRACTS.TIP20_FACTORY;
        if (!factoryAddress) throw new Error("TIP20_FACTORY address not found");

        const factory = new ethers.Contract(factoryAddress, TIP20_FACTORY_ABI, wallet);

        const quoteToken = CONFIG.TOKENS.PathUSD;
        // Approve Factory and Fee Manager for PathUSD
        const spenders = [SYSTEM_CONTRACTS.TIP20_FACTORY, SYSTEM_CONTRACTS.FEE_MANAGER];
        const quoteCtx = new ethers.Contract(quoteToken, ERC20_ABI, wallet);

        for (const spender of spenders) {
            try {
                const allowance = await quoteCtx.allowance(wallet.address, spender);
                if (allowance < ethers.parseUnits("100", 6)) {
                    if (!silent) console.log(`${COLORS.dim}Approving ${spender === SYSTEM_CONTRACTS.FEE_MANAGER ? 'Fee Manager' : 'Factory'}...${COLORS.reset}`);

                    const txCreator = async () => {
                        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
                        return quoteCtx.approve(spender, ethers.MaxUint256, {
                            ...gasOverrides,
                            gasLimit: 300000,
                            feeCurrency: CONFIG.TOKENS.PathUSD
                        });
                    };

                    await sendTxWithRetry(wallet, txCreator);
                }
            } catch (e) {
                if (!silent) console.log(`${COLORS.dim}Warning: Failed to approve spender ${spender}: ${e.message}${COLORS.reset}`);
            }
        }

        // Create Token
        const txCreator = async () => {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            const salt = ethers.hexlify(ethers.randomBytes(32));
            return factory.createToken(tokenName, tokenSymbol, currency, quoteToken, wallet.address, salt, {
                gasLimit: 3000000,
                ...gasOverrides,
                feeCurrency: CONFIG.TOKENS.PathUSD
            });
        };

        const { receipt } = await sendTxWithRetry(wallet, txCreator);

        // Find TokenCreated event
        let tokenAddress = null;
        for (const log of receipt.logs) {
            try {
                const parsedLog = factory.interface.parseLog(log);
                if (parsedLog && parsedLog.name === 'TokenCreated') {
                    tokenAddress = parsedLog.args.token;
                    break;
                }
            } catch (e) { }
        }

        if (!tokenAddress) throw new Error("Could not parse TokenCreated event");

        if (!silent) console.log(`${COLORS.fg.green}✓ Meme Token Created: ${tokenAddress}${COLORS.reset}`);

        // Wait for token propagation
        await sleep(3000);

        const pendingTxs = [];
        // Grant roles
        const rolesGranted = await grantRoles(wallet, tokenAddress, quoteToken, pendingTxs, silent);

        // Mint initial supply
        if (!silent) console.log(`${COLORS.dim}Minting initial supply...${COLORS.reset}`);
        const tokenContract = new ethers.Contract(tokenAddress, MINT_ABI, wallet);
        const mintAmount = ethers.parseUnits('100000', 6);

        const mintTxCreator = async () => {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            return tokenContract.mint(wallet.address, mintAmount, {
                gasLimit: 300000,
                ...gasOverrides,
                feeCurrency: CONFIG.TOKENS.PathUSD
            });
        };
        // We'll execute this immediately, not push to pendingTxs for manual waiting, 
        // because sendTxWithRetry waits for receipt.
        await sendTxWithRetry(wallet, mintTxCreator);

        // Remove pendingTxs logic since we await directly now for robustness
        // pendingTxs.push(mintTx);

        // Legacy pendingTxs loop removal (since we await above now)
        // if (!silent) console.log(`${COLORS.dim}Waiting for ${pendingTxs.length} transactions to confirm...${COLORS.reset}`);
        // for (let i = 0; i < pendingTxs.length; i++) { ... }

        if (rolesGranted && !silent) console.log(`${COLORS.fg.green}✓ Roles Granted & Minted Initial Supply${COLORS.reset}`);

        saveCreatedMeme(wallet.address, tokenAddress, tokenSymbol, tokenName, receipt.blockNumber);

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'CreateMeme', 'success', `${tokenSymbol}`, silent, duration);
        return { success: true, txHash: receipt.transactionHash, tokenAddress, symbol: tokenSymbol, tokenName, rolesGranted };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'CreateMeme', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Creation failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

async function grantRoles(wallet, tokenAddress, quoteTokenAddress, pendingTxs, silent = false) {
    try {
        if (!silent) console.log(`${COLORS.fg.cyan}Granting roles...${COLORS.reset}`);
        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);

        // 1. Approve Fee Token
        const feeToken = new ethers.Contract(quoteTokenAddress, ERC20_ABI, wallet);
        const approveTxCreator = async () => {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            return feeToken.approve(tokenAddress, ethers.MaxUint256, {
                ...gasOverrides,
                gasLimit: 300000,
                feeCurrency: CONFIG.TOKENS.PathUSD
            });
        };
        await sendTxWithRetry(wallet, approveTxCreator);

        // 2. Grant ISSUER_ROLE
        const ISSUER_ROLE = ethers.id("ISSUER_ROLE");
        const tokenContract = new ethers.Contract(tokenAddress, ERC20_ABI, wallet);
        const grantTxCreator = async () => {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            return tokenContract.grantRole(ISSUER_ROLE, wallet.address, {
                ...gasOverrides,
                gasLimit: 300000,
                feeCurrency: CONFIG.TOKENS.PathUSD
            });
        };
        await sendTxWithRetry(wallet, grantTxCreator);

        return true;
    } catch (error) {
        if (!silent) console.log(`${COLORS.fg.red}⚠ Role granting setup failed: ${error.message}${COLORS.reset}`);
        return false;
    }
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