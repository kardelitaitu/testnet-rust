import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import solc from 'solc';
import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier } from '../utils/helpers.js';

import crypto from 'crypto';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const TRACKER_FILE = path.join(ROOT_DIR, 'data', 'splitter-contract-tracker.json');
const ADDRESS_FILE = path.join(ROOT_DIR, 'data', 'address.txt');
const MNEMONIC_FILE = path.join(ROOT_DIR, 'utils', 'mnemonic.txt');
const BUILD_FILE = path.join(ROOT_DIR, 'data', 'TempoSplitter_build.json');

// Internal Cache
let compiledCache = null;

function generateShares(count) {
    let shares = [];
    let remaining = 10000; // 100.00%
    for (let i = 0; i < count - 1; i++) {
        // Ensure at least 10 basis points per person, and leave enough for others
        const max = remaining - ((count - 1 - i) * 10);
        const share = getRandomInt(10, Math.min(max, 3000)); // Cap single share at 30% to distribute better
        shares.push(share);
        remaining -= share;
    }
    shares.push(remaining);
    return shares.sort(() => Math.random() - 0.5);
}

function getSourceHash(content) {
    return crypto.createHash('md5').update(content).digest('hex');
}

export function compileContract(silent = false) {
    const contractPath = path.join(ROOT_DIR, 'contracts', 'TempoSplitter.sol');
    if (!fs.existsSync(contractPath)) throw new Error("TempoSplitter.sol not found");
    const source = fs.readFileSync(contractPath, 'utf-8');
    const currentHash = getSourceHash(source);

    // Check Cache File
    if (fs.existsSync(BUILD_FILE)) {
        try {
            const cached = JSON.parse(fs.readFileSync(BUILD_FILE, 'utf-8'));
            if (cached.hash === currentHash && cached.abi && cached.bytecode) {
                if (!silent) console.log(`${COLORS.fg.green}✓ Loaded compiled contract from cache${COLORS.reset}\n`);
                return { abi: cached.abi, bytecode: cached.bytecode };
            }
        } catch (e) {
            // Ignore cache error, recompile
        }
    }

    if (!silent) console.log(`${COLORS.fg.cyan}Compiling TempoSplitter.sol...${COLORS.reset}`);

    const input = {
        language: 'Solidity',
        sources: {
            'TempoSplitter.sol': {
                content: source
            }
        },
        settings: {
            optimizer: {
                enabled: true,
                runs: 200
            },
            evmVersion: 'paris',
            outputSelection: {
                '*': {
                    '*': ['abi', 'evm.bytecode']
                }
            }
        }
    };

    try {
        const output = JSON.parse(solc.compile(JSON.stringify(input)));

        if (output.errors) {
            const errors = output.errors.filter(e => e.severity === 'error');
            if (errors.length > 0) {
                throw new Error(errors[0].formattedMessage);
            }
        }

        const contract = output.contracts['TempoSplitter.sol']['TempoSplitter'];
        const result = {
            abi: contract.abi,
            bytecode: contract.evm.bytecode.object
        };

        // Save to Cache
        fs.writeFileSync(BUILD_FILE, JSON.stringify({
            hash: currentHash,
            abi: result.abi,
            bytecode: result.bytecode,
            compiledAt: new Date().toISOString()
        }, null, 2));

        if (!silent) console.log(`${COLORS.fg.green}✓ Contract compiled & cached!${COLORS.reset}\n`);

        return result;
    } catch (error) {
        if (!silent) console.error(`${COLORS.fg.red}Compilation failed: ${error.message}${COLORS.reset}`);
        return null;
    }
}

export async function deployRandomContract(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    // 1. Compile/Get Cache
    if (!compiledCache) {
        compiledCache = compileContract(silent);
    }
    if (!compiledCache) {
        logWalletAction(workerId, walletIndex, wallet.address, 'DeploySplitter', 'failed', 'Compilation failed', silent);
        return { success: false, reason: 'compilation_failed' };
    }

    // 2. Deploy One Contract
    return await deployForWallet(wallet, proxy, 1, compiledCache.abi, compiledCache.bytecode, workerId, walletIndex, silent);
}

export async function deployForWallet(wallet, proxy, deployCount, abi, bytecode, workerId = 1, walletIndex = 0, silent = false) {
    let successCount = 0;
    let failCount = 0;
    let lastBlockNumber = 'unknown';
    let lastTxHash = null;

    // Load Addresses & Words
    let allAddrs = [];
    if (fs.existsSync(ADDRESS_FILE)) {
        allAddrs = fs.readFileSync(ADDRESS_FILE, 'utf-8').split('\n').map(l => l.trim()).filter(l => l && ethers.isAddress(l));
    }
    if (allAddrs.length < 20) {
        const msg = "Not enough addresses in data/address.txt";
        if (!silent) console.log(msg);
        logWalletAction(workerId, walletIndex, wallet.address, 'DeploySplitter', 'failed', msg, silent);
        return { success: false, reason: 'missing_addresses' };
    }

    let words = ["ALPHA", "BETA", "GAMMA"];
    if (fs.existsSync(MNEMONIC_FILE)) {
        words = fs.readFileSync(MNEMONIC_FILE, 'utf-8').split('\n').map(w => w.trim()).filter(w => w.length > 0);
    }

    for (let j = 0; j < deployCount; j++) {
        const startTime = Date.now();

        try {
            // Check Balance
            const balance = await wallet.provider.getBalance(wallet.address);
            if (balance === 0n) {
                const duration = (Date.now() - startTime) / 1000;
                if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'DeploySplitter', 'skipped', 'Insufficient Balance', silent, duration);
                failCount++;
                continue;
            }

            // Prepare Data
            const payeeCount = getRandomInt(20, 50);
            const payees = allAddrs.sort(() => 0.5 - Math.random()).slice(0, payeeCount);
            const shares = generateShares(payeeCount);

            // Random Dividend Name
            const dividendName = words[Math.floor(Math.random() * words.length)].toUpperCase();
            const memos = payees.map((_, i) => `${dividendName} shares ${(shares[i] / 100).toFixed(2)}%`);

            // Deploy
            const factory = new ethers.ContractFactory(abi, bytecode, wallet);
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);

            const contract = await factory.deploy(payees, shares, memos, {
                ...gasOverrides
            });

            lastTxHash = contract.deploymentTransaction().hash;
            if (!silent) console.log(`${COLORS.dim}Tx Sent: ${CONFIG.EXPLORER_URL}/tx/${lastTxHash}${COLORS.reset}`);

            await contract.waitForDeployment();
            const contractAddress = await contract.getAddress();
            const receipt = await contract.deploymentTransaction().wait();
            lastBlockNumber = receipt.blockNumber;

            // Save to Tracker (Simple Append logic)
            const record = {
                address: contractAddress,
                deployer: wallet.address,
                deployedAt: new Date().toISOString(),
                payees: payees,
                shares: shares,
                memos: memos
            };

            let tracker = [];
            if (fs.existsSync(TRACKER_FILE)) {
                try { tracker = JSON.parse(fs.readFileSync(TRACKER_FILE)); } catch (e) { }
            }
            tracker.push(record);
            fs.writeFileSync(TRACKER_FILE, JSON.stringify(tracker, null, 2));

            const duration = (Date.now() - startTime) / 1000;
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'DeploySplitter', 'success', `Addr: ${contractAddress} (${payeeCount} Payees)`, silent, duration);
            if (!silent) console.log(`${COLORS.fg.green}✓ Deployed at: ${CONFIG.EXPLORER_URL}/address/${contractAddress}${COLORS.reset}`);

            successCount++;

        } catch (error) {
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'DeploySplitter', 'failed', error.message.substring(0, 50), silent, duration);
            failCount++;
        }

        if (j < deployCount - 1 && !silent) {
            await countdown(getRandomInt(CONFIG.MIN_DELAY_BETWEEN_DEPLOYS, CONFIG.MAX_DELAY_BETWEEN_DEPLOYS), 'Next deployment');
        } else if (j < deployCount - 1 && silent) {
            await sleep(2000);
        }
    }

    if (successCount > 0) {
        return { success: true, deployed: successCount, failed: failCount, block: lastBlockNumber, txHash: lastTxHash };
    } else {
        return { success: false, reason: 'all_deployments_failed' };
    }
}

export async function deployContract() {
    console.log(`\n  ${COLORS.fg.magenta}📦  TEMPO SPLITTER DEPLOYER${COLORS.reset}\n`);

    const { abi, bytecode } = compileContract();

    // Get input
    let deployCount = 1;
    try {
        const answer = await askQuestion(`${COLORS.fg.cyan}How many contracts per wallet? (Default: 1) ${COLORS.reset}`);
        if (answer.trim()) {
            deployCount = parseInt(answer);
            if (isNaN(deployCount) || deployCount < 1) deployCount = 1;
        }
    } catch (e) {
        deployCount = 1;
    }

    const privateKeys = getPrivateKeys();
    console.log(`${COLORS.fg.cyan}Found ${privateKeys.length} wallet(s)${COLORS.reset}\n`);

    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);

        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}: ${wallet.address}${COLORS.reset}`);
        if (proxy) console.log(`${COLORS.dim}Proxy: ${proxy}${COLORS.reset}`);

        await deployForWallet(wallet, proxy, deployCount, abi, bytecode, 1, i);

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(CONFIG.MIN_DELAY_BETWEEN_WALLETS, CONFIG.MAX_DELAY_BETWEEN_WALLETS), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ All deployments completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}
