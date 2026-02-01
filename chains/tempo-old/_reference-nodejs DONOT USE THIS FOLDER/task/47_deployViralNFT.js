import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import solc from 'solc';
import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { getGasWithMultiplier } from '../utils/helpers.js';
import crypto from 'crypto';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const TRACKER_FILE = path.join(ROOT_DIR, 'data', 'viral_nfts.json');
const BUILD_FILE = path.join(ROOT_DIR, 'data', 'ViralNFT_build.json');

// Internal Cache for Compilation
let compiledCache = null;

function getSourceHash(content) {
    return crypto.createHash('md5').update(content).digest('hex');
}

function compileContract(silent = false) {
    const contractPath = path.join(ROOT_DIR, 'contracts', 'ViralNFT.sol');
    if (!fs.existsSync(contractPath)) throw new Error("ViralNFT.sol not found");
    const source = fs.readFileSync(contractPath, 'utf-8');
    const currentHash = getSourceHash(source);

    // Check Cache File
    if (fs.existsSync(BUILD_FILE)) {
        try {
            const cached = JSON.parse(fs.readFileSync(BUILD_FILE, 'utf-8'));
            if (cached.hash === currentHash && cached.abi && cached.bytecode) {
                if (!silent) console.log(`${COLORS.fg.green}✓ Loaded compiled ViralNFT from cache${COLORS.reset}\n`);
                return { abi: cached.abi, bytecode: cached.bytecode };
            }
        } catch (e) { }
    }

    if (!silent) console.log(`${COLORS.fg.cyan}Compiling ViralNFT.sol...${COLORS.reset}`);

    const input = {
        language: 'Solidity',
        sources: { 'ViralNFT.sol': { content: source } },
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
        const contract = output.contracts['ViralNFT.sol']['ViralNFT'];
        const result = { abi: contract.abi, bytecode: contract.evm.bytecode.object };

        // Save to Cache
        fs.writeFileSync(BUILD_FILE, JSON.stringify({
            hash: currentHash,
            abi: result.abi,
            bytecode: result.bytecode,
            compiledAt: new Date().toISOString()
        }, null, 2));

        if (!silent) console.log(`${COLORS.fg.green}✓ ViralNFT compiled & cached!${COLORS.reset}\n`);

        return result;
    } catch (error) {
        if (!silent) console.error(`${COLORS.fg.red}Compilation failed: ${error.message}${COLORS.reset}`);
        return null;
    }
}

export async function deployViralNFTForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    if (!compiledCache) {
        try {
            compiledCache = compileContract(silent);
        } catch (e) {
            logWalletAction(workerId, walletIndex, wallet.address, 'DeployNFT', 'failed', 'Compilation failed', silent, 0, proxy);
            return { success: false, reason: 'compilation_failed' };
        }
    }

    const startTime = Date.now();

    try {
        if (!silent) console.log(`${COLORS.fg.yellow}Deploying ViralNFT...${COLORS.reset}`);
        const factory = new ethers.ContractFactory(compiledCache.abi, compiledCache.bytecode, wallet);
        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);

        // Randomize Metadata
        const ADJECTIVES = [
            'Viral', 'Based', 'Tempo', 'Cyber', 'Rare', 'Epic', 'Degen', 'Alpha', 'Chad',
            'Flash', 'Quantum', 'Glitch', 'Neon', 'Pixel', 'Sonic', 'Hyper', 'Mega', 'Giga',
            'Ultra', 'Super', 'Prime', 'Elite', 'Legendary', 'Mythic', 'Divine', 'Shadow', 'Ghost',
            'Liquid', 'Solid', 'Ether', 'Void', 'Nebula', 'Cosmic', 'Astro', 'Lunar', 'Solar',
            'Mystic', 'Arcane', 'Ancient', 'Future', 'Holo', 'Meta', 'Crypto', 'Techno', 'Nano'
        ];
        const NOUNS = [
            'Doge', 'Pepe', 'Punk', 'Cat', 'Ape', 'Wojak', 'Moon', 'Gem', 'Artifact', 'Coin',
            'Inu', 'Kitten', 'Bot', 'Droid', 'Mecha', 'Dragon', 'Phoenix', 'Wizard', 'Knight', 'Ninja',
            'Samurai', 'Alien', 'UFO', 'Rocket', 'Star', 'Comet', 'Planet', 'World', 'Land', 'City',
            'Tower', 'Castle', 'Sword', 'Shield', 'Scroll', 'Potion', 'Orb', 'Crystal', 'Diamond', 'Gold',
            'Token', 'Pass', 'Key', 'Ticket', 'Card', 'Badge', 'Relic', 'Totem', 'Statue', 'Monument'
        ];

        const adj = ADJECTIVES[Math.floor(Math.random() * ADJECTIVES.length)];
        const noun = NOUNS[Math.floor(Math.random() * NOUNS.length)];
        const name = `${adj} ${noun}`;
        const symbol = `${adj[0]}${noun}`.toUpperCase(); // e.g., "VPepe"

        const contract = await factory.deploy(name, symbol, { ...gasOverrides });
        await contract.waitForDeployment();
        const contractAddress = await contract.getAddress();

        if (!silent) console.log(`${COLORS.fg.green}✓ Deployed ${name} (${symbol}) at: ${contractAddress}${COLORS.reset}`);

        // Save to Tracker
        const record = {
            address: contractAddress,
            name: name,
            symbol: symbol,
            deployer: wallet.address,
            deployedAt: new Date().toISOString()
        };

        let tracker = [];
        if (fs.existsSync(TRACKER_FILE)) {
            try { tracker = JSON.parse(fs.readFileSync(TRACKER_FILE, 'utf8')); } catch (e) { }
        }
        tracker.push(record);
        fs.writeFileSync(TRACKER_FILE, JSON.stringify(tracker, null, 2));

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'DeployNFT', 'success', `Deployed ${name}`, silent, duration, proxy);

        return { success: true, txHash: contract.deploymentTransaction().hash, contractAddress };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'DeployNFT', 'failed', error.message.substring(0, 50), silent, duration, proxy);
        if (!silent) console.error(`${COLORS.fg.red}Deploy failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}
