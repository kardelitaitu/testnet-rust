import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import solc from 'solc';
import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, getGasWithMultiplier } from '../utils/helpers.js';

import crypto from 'crypto';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const TRACKER_FILE = path.join(ROOT_DIR, 'data', 'viral_faucets.json');
const BUILD_FILE = path.join(ROOT_DIR, 'data', 'ViralFaucet_build.json');

const ERC20_ABI = [
    "function balanceOf(address owner) view returns (uint256)",
    "function approve(address spender, uint256 amount) returns (bool)",
    "function symbol() view returns (string)",
    "function decimals() view returns (uint8)"
];

// Internal Cache for Compilation
let compiledCache = null;

function getSourceHash(content) {
    return crypto.createHash('md5').update(content).digest('hex');
}

function compileContract(silent = false) {
    const contractPath = path.join(ROOT_DIR, 'contracts', 'ViralFaucet.sol');
    if (!fs.existsSync(contractPath)) throw new Error("ViralFaucet.sol not found");
    const source = fs.readFileSync(contractPath, 'utf-8');
    const currentHash = getSourceHash(source);

    // Check Cache File
    if (fs.existsSync(BUILD_FILE)) {
        try {
            const cached = JSON.parse(fs.readFileSync(BUILD_FILE, 'utf-8'));
            if (cached.hash === currentHash && cached.abi && cached.bytecode) {
                if (!silent) console.log(`${COLORS.fg.green}✓ Loaded compiled ViralFaucet from cache${COLORS.reset}\n`);
                return { abi: cached.abi, bytecode: cached.bytecode };
            }
        } catch (e) { }
    }

    if (!silent) console.log(`${COLORS.fg.cyan}Compiling ViralFaucet.sol...${COLORS.reset}`);

    const input = {
        language: 'Solidity',
        sources: { 'ViralFaucet.sol': { content: source } },
        settings: {
            optimizer: { enabled: true, runs: 200 },
            outputSelection: { '*': { '*': ['abi', 'evm.bytecode'] } }
        }
    };

    try {
        const output = JSON.parse(solc.compile(JSON.stringify(input)));
        if (output.errors) {
            const errors = output.errors.filter(e => e.severity === 'error');
            if (errors.length > 0) throw new Error(errors[0].formattedMessage);
        }
        const contract = output.contracts['ViralFaucet.sol']['ViralFaucet'];
        const result = { abi: contract.abi, bytecode: contract.evm.bytecode.object };

        // Save to Cache
        fs.writeFileSync(BUILD_FILE, JSON.stringify({
            hash: currentHash,
            abi: result.abi,
            bytecode: result.bytecode,
            compiledAt: new Date().toISOString()
        }, null, 2));

        if (!silent) console.log(`${COLORS.fg.green}✓ ViralFaucet compiled & cached!${COLORS.reset}\n`);

        return result;
    } catch (error) {
        if (!silent) console.error(`${COLORS.fg.red}Compilation failed: ${error.message}${COLORS.reset}`);
        return null;
    }
}

export async function deployViralFaucetForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    if (!compiledCache) {
        try {
            compiledCache = compileContract(silent);
        } catch (e) {
            logWalletAction(workerId, walletIndex, wallet.address, 'DeployFaucet', 'failed', 'Compilation failed', silent, 0, proxy);
            return { success: false, reason: 'compilation_failed' };
        }
    }

    const startTime = Date.now();

    // 1. Find a Token to Fund (Stablecoin)
    let tokenAddress = null;
    let tokenSymbol = null;
    let tokenContract = null;
    let decimals = 18;
    let balance = 0n;

    if (!silent) console.log(`${COLORS.dim}Scanning for funded tokens to back the faucet...${COLORS.reset}`);

    const tokensToCheck = Object.entries(CONFIG.TOKENS);
    // Shuffle to avoid always picking the first one
    tokensToCheck.sort(() => Math.random() - 0.5);

    for (const [sym, addr] of tokensToCheck) {
        try {
            const c = new ethers.Contract(addr, ERC20_ABI, wallet);
            const bal = await c.balanceOf(wallet.address);
            if (bal >= ethers.parseUnits("50", 6)) { // Ensure enough balance (e.g. 50 USDC)
                tokenAddress = addr;
                tokenSymbol = sym;
                tokenContract = c;
                decimals = await c.decimals();
                balance = bal;
                if (!silent) console.log(`${COLORS.dim}Selected ${sym} (Bal: ${ethers.formatUnits(bal, decimals)})${COLORS.reset}`);
                break;
            }
        } catch (e) { }
    }

    if (!tokenAddress) {
        const duration = (Date.now() - startTime) / 1000;
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No sufficient stablecoin balance to fund faucet.${COLORS.reset}`);
        logWalletAction(workerId, walletIndex, wallet.address, 'DeployFaucet', 'skipped', 'No stablecoin balance', silent, duration, proxy);
        return { success: false, reason: 'no_stablecoin_balance' };
    }

    // 2. Deploy Faucet
    try {
        if (!silent) console.log(`${COLORS.fg.yellow}Deploying ViralFaucet...${COLORS.reset}`);
        const factory = new ethers.ContractFactory(compiledCache.abi, compiledCache.bytecode, wallet);
        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);

        const contract = await factory.deploy({ ...gasOverrides });
        await contract.waitForDeployment();
        const contractAddress = await contract.getAddress();

        if (!silent) console.log(`${COLORS.fg.green}✓ Deployed at: ${contractAddress}${COLORS.reset}`);

        // 3. Fund Faucet
        // Fund Amount: Random between 20 and 50% of balance, capped at 100
        let fundAmountVal = parseFloat(ethers.formatUnits(balance, decimals)) * (0.2 + Math.random() * 0.3);
        if (fundAmountVal > 100) fundAmountVal = 100; // Cap at 100 units
        if (fundAmountVal < 10) fundAmountVal = 10;   // Min 10 units

        const fundAmount = ethers.parseUnits(fundAmountVal.toFixed(2), decimals);

        if (!silent) console.log(`${COLORS.fg.cyan}Funding with ${fundAmountVal.toFixed(2)} ${tokenSymbol}...${COLORS.reset}`);

        // Approve
        const approveTx = await tokenContract.approve(contractAddress, fundAmount, { ...gasOverrides });
        await approveTx.wait();

        // Fund
        const faucet = new ethers.Contract(contractAddress, compiledCache.abi, wallet);
        const fundTx = await faucet.fund(tokenAddress, fundAmount, { ...gasOverrides });
        const receipt = await fundTx.wait();

        // 4. Save to Tracker
        const record = {
            address: contractAddress,
            deployer: wallet.address,
            token: tokenAddress,
            symbol: tokenSymbol,
            decimals: Number(decimals), // Fix: JSON cannot support BigInt
            deployedAt: new Date().toISOString()
        };

        let tracker = [];
        if (fs.existsSync(TRACKER_FILE)) {
            try { tracker = JSON.parse(fs.readFileSync(TRACKER_FILE, 'utf8')); } catch (e) { }
        }
        tracker.push(record);
        fs.writeFileSync(TRACKER_FILE, JSON.stringify(tracker, null, 2));

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'DeployFaucet', 'success', `Deployed & Funded (${fundAmountVal.toFixed(2)} ${tokenSymbol})`, silent, duration, proxy);

        return { success: true, txHash: receipt.hash, contractAddress, funded: fundAmountVal };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'DeployFaucet', 'failed', error.message.substring(0, 50), silent, duration, proxy);
        if (!silent) console.error(`${COLORS.fg.red}Deploy failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}
