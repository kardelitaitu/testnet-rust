import fs from 'fs';
import path from 'path';
import { ethers, FetchRequest } from 'ethers';
import { CONFIG } from './constants.js';
import { getProxyForIndex, getProxyAgent, formatProxy, getProxyIp } from './proxies.js';

const __dirname = path.dirname(new URL(import.meta.url).pathname).substring(1); // Fix for Windows paths in ES modules if needed, or simply:
// Better robust ES module path resolution:
import { fileURLToPath } from 'url';
const __filename = fileURLToPath(import.meta.url);
const _dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(_dirname, '..');

import { decrypt } from './crypto.js';
import axios from 'axios';
import { globalLimiter } from './rateLimiter.js';
import { logProxyAttempt } from './proxyMonitor.js';

const PRIVATE_KEYS_FILE = path.join(ROOT_DIR, 'pv.txt');
const WALLETS_DIR = path.join(ROOT_DIR, 'wallets');
const PROXY_FILE = path.join(ROOT_DIR, 'proxy.txt');
const CREATED_TOKENS_FILE = path.join(ROOT_DIR, 'data', 'created_tokens.json');
const CREATED_MEMES_FILE = path.join(ROOT_DIR, 'data', 'created_memes.json');
const TASK_COUNTS_FILE = path.join(ROOT_DIR, 'data', 'wallet_task_counts.json');

export function getWalletFiles() {
    if (!fs.existsSync(WALLETS_DIR)) return [];
    return fs.readdirSync(WALLETS_DIR)
        .filter(file => file.endsWith('.json'))
        .sort();
}

const privateKeyCache = new Map();

export function getPrivateKeyFromFile(filename, password) {
    if (privateKeyCache.has(filename)) {
        return privateKeyCache.get(filename);
    }

    try {
        const filePath = path.join(WALLETS_DIR, filename);
        const fileContent = JSON.parse(fs.readFileSync(filePath, 'utf-8'));

        if (fileContent.encrypted) {
            const pass = password || process.env.WALLET_PASSWORD;
            if (!pass) {
                throw new Error('Password required');
            }

            const decryptedContent = decrypt(fileContent, pass);
            let privateKey = decryptedContent;

            try {
                if (decryptedContent.trim().startsWith('{')) {
                    const parsed = JSON.parse(decryptedContent);
                    if (parsed.evm_private_key) privateKey = parsed.evm_private_key;
                    else if (parsed.privateKey) privateKey = parsed.privateKey;
                }
            } catch (e) { }

            privateKeyCache.set(filename, privateKey);
            return privateKey;
        } else if (fileContent.privateKey) {
            privateKeyCache.set(filename, fileContent.privateKey);
            return fileContent.privateKey;
        }
    } catch (err) {
        throw new Error(`Failed to load ${filename}: ${err.message}`);
    }
    return null;
}


export function getPrivateKeys(password) {
    try {
        if (!fs.existsSync(WALLETS_DIR)) {
            // Fallback to pv.txt if wallets dir doesn't exist
            if (!fs.existsSync(PRIVATE_KEYS_FILE)) return [];
            console.log('Wallets directory not found, falling back to pv.txt');
            const content = fs.readFileSync(PRIVATE_KEYS_FILE, 'utf-8');
            return content
                .split('\n')
                .map(line => line.trim())
                .filter(line => line && !line.startsWith('#'));
        }

        const files = fs.readdirSync(WALLETS_DIR)
            .filter(file => file.endsWith('.json'))
            .sort(); // Ensure consistent order (0001.json, 0002.json...)

        const keys = [];
        for (const file of files) {
            try {
                const filePath = path.join(WALLETS_DIR, file);
                const fileContent = JSON.parse(fs.readFileSync(filePath, 'utf-8'));

                if (fileContent.encrypted) {
                    const pass = password || process.env.WALLET_PASSWORD;
                    if (!pass) {
                        throw new Error('Password required to decrypt wallet ' + file);
                    }

                    const decryptedContent = decrypt(fileContent, pass);

                    let privateKey = decryptedContent;
                    try {
                        // Try to parse as JSON if it looks like one
                        if (decryptedContent.trim().startsWith('{')) {
                            const parsed = JSON.parse(decryptedContent);
                            // Prioritize EVM private key
                            if (parsed.evm_private_key) {
                                privateKey = parsed.evm_private_key;
                            } else if (parsed.privateKey) {
                                privateKey = parsed.privateKey;
                            }
                            // If it's another format (e.g. only mnemonic), we might need to handle it, 
                            // but for now, we assume evm_private_key is present as per user error log.
                        }
                    } catch (e) {
                        // If parsing fails, use the content as is (maybe it was just a raw key)
                        // console.debug('Failed to parse decrypted content as JSON, using raw string');
                    }

                    keys.push(privateKey);
                } else if (fileContent.privateKey) {
                    // Unencrypted (fallback/legacy)
                    keys.push(fileContent.privateKey);
                }
            } catch (err) {
                console.error(`Error loading wallet ${file}: ${err.message}`);
                // Continue to next wallet
            }
        }
        return keys;
    } catch (error) {
        console.error(`Error loading private keys: ${error.message}`);
        return [];
    }
}

const FINGERPRINTS = [
    {
        ua: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        platform: '"Windows"',
        sec_ua: '"Not_A Brand";v="8", "Chromium";v="120", "Google Chrome";v="120"'
    },
    {
        ua: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36",
        platform: '"macOS"',
        sec_ua: '"Not A(Brand";v="99", "Google Chrome";v="121", "Chromium";v="121"'
    },
    {
        ua: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        platform: '"Linux"',
        sec_ua: '"Not_A Brand";v="8", "Chromium";v="120", "Google Chrome";v="120"'
    }
];

const providerCache = new Map();

export function createProvider(proxyUrl = null) {
    const cacheKey = proxyUrl || 'direct';
    if (providerCache.has(cacheKey)) {
        return providerCache.get(cacheKey);
    }

    const rpcUrl = CONFIG.RPC_URL;
    const chainId = CONFIG.CHAIN_ID || 42429;

    // Zero-Leak Configuration: Explicitly define network to prevent Ethers from 
    // calling eth_chainId/eth_blockNumber to auto-detect the network.
    const network = ethers.Network.from({
        chainId: chainId,
        name: 'tempo-testnet',
        ensAddress: null
    });

    const fetchReq = new FetchRequest(rpcUrl);

    if (proxyUrl) {
        // SELECT FINGERPRINT ONCE PER PROVIDER (Session Consistency)
        const fp = FINGERPRINTS[Math.floor(Math.random() * FINGERPRINTS.length)];

        // Imports cached at top level
        // Pre-calculate unique RPC pool
        const rpcPool = [CONFIG.RPC_URL, ...(CONFIG.RPC_LIST || [])];
        const uniqueRpcPool = [...new Set(rpcPool)].filter(u => u && u.startsWith('http'));

        fetchReq.getUrlFunc = async (req, signal) => {
            const agent = getProxyAgent(proxyUrl);
            const axiosInstance = axios.default || axios;

            // Extract wallet address for rate limiting (lightweight check)
            let walletAddress = null;
            if (req.body) {
                try {
                    // Quick string scan before heavy JSON parse if possible, or just parse
                    // Optimization: We could potentially skip this if not strictly needed for rate limiting per-wallet
                    // But sticking to safety:
                    const bodyStr = new TextDecoder().decode(req.body);
                    const body = JSON.parse(bodyStr);
                    const firstReq = Array.isArray(body) ? body[0] : body;
                    if (firstReq?.params?.[0]?.from) {
                        walletAddress = firstReq.params[0].from;
                    }
                } catch (e) { /* ignore parse error */ }
            }

            const MAX_RETRIES = (CONFIG.RATE_LIMIT && CONFIG.RATE_LIMIT.maxRetries) || 5;
            let attempt = 0;
            let lastError = null;

            while (attempt < MAX_RETRIES) {
                const targetRpcUrl = uniqueRpcPool[attempt % uniqueRpcPool.length];

                try {
                    const response = await globalLimiter.schedule(async () => {
                        return axiosInstance({
                            method: req.method,
                            url: targetRpcUrl,
                            data: req.body,
                            headers: {
                                ...req.headers,
                                'Host': new URL(targetRpcUrl).host,
                                'Content-Type': 'application/json',
                                'User-Agent': fp.ua, // Consistent UA
                                'Accept': 'application/json, text/plain, */*',
                                'Accept-Language': 'en-US,en;q=0.9',
                                'Accept-Encoding': 'gzip, deflate, br',
                                'Origin': CONFIG.EXPLORER_URL || 'https://tempo.xyz',
                                'Referer': CONFIG.EXPLORER_URL || 'https://tempo.xyz/',
                                'Connection': 'keep-alive',
                                'Sec-Ch-Ua': fp.sec_ua,
                                'Sec-Ch-Ua-Mobile': '?0',
                                'Sec-Ch-Ua-Platform': fp.platform,
                                'Sec-Fetch-Site': 'same-origin',
                                'Sec-Fetch-Mode': 'cors',
                                'Sec-Fetch-Dest': 'empty',
                                'Priority': 'u=1, i'
                            },
                            httpsAgent: agent,
                            httpAgent: agent,
                            timeout: 10000 + (attempt * 5000),
                            validateStatus: () => true
                        });
                    }, walletAddress);

                    if (response.status === 429 || response.status === 403 || (response.status >= 500 && response.status < 600)) {
                        if (response.status === 429 && response.headers['retry-after']) {
                            const retrySecs = parseInt(response.headers['retry-after'], 10) || 60;
                            await new Promise(r => setTimeout(r, retrySecs * 1000));
                        }
                        throw new Error(`RPC Error: ${response.status} ${response.statusText}`);
                    }

                    const isTx = req.body && req.body.includes('eth_sendRawTransaction');
                    let txHash = null;
                    if (isTx && response.data?.result) {
                        txHash = response.data.result;
                    }

                    logProxyAttempt(proxyUrl, 'rpc_call', true, txHash, null);

                    return {
                        statusCode: response.status,
                        statusMessage: response.statusText,
                        headers: response.headers,
                        body: new Uint8Array(Buffer.from(JSON.stringify(response.data)))
                    };

                } catch (error) {
                    lastError = error;
                    attempt++;

                    if (attempt < MAX_RETRIES) {
                        const multiplier = globalLimiter.getAdaptiveDelayMultiplier();
                        const baseDelay = (CONFIG.RATE_LIMIT ? CONFIG.RATE_LIMIT.baseDelay : 1000) * Math.pow(2, attempt - 1);
                        const jitter = (Math.random() - 0.5) * (CONFIG.RATE_LIMIT ? CONFIG.RATE_LIMIT.jitterRange : 500);
                        let delay = Math.max(0, (baseDelay * multiplier) + jitter);
                        if (CONFIG.RATE_LIMIT && CONFIG.RATE_LIMIT.maxDelay) {
                            delay = Math.min(delay, CONFIG.RATE_LIMIT.maxDelay);
                        }
                        await new Promise(r => setTimeout(r, delay));
                        continue;
                    }
                    break;
                }
            }

            logProxyAttempt(proxyUrl, 'rpc_call', false, null, lastError ? lastError.message : 'Unknown Error');
            throw lastError || new Error('Request failed');
        };
    }

    fetchReq.timeout = 30000;
    const provider = new ethers.JsonRpcProvider(fetchReq, network, {
        staticNetwork: network,
        batchMaxCount: 5 // ENABLE BATCHING for 5x less requests
    });

    providerCache.set(cacheKey, provider);
    return provider;
}


const walletCache = new Map(); // Stores provider-less wallets (key pairs)
const baseWalletCache = new Map(); // Maps privateKey -> ethers.Wallet (no provider)

export async function getWallet(index, privateKey) {

    // 1. Get/Create Base Wallet (Expensive Crypto Ops happen here)
    let baseWallet;
    if (baseWalletCache.has(privateKey)) {
        baseWallet = baseWalletCache.get(privateKey);
    } else {
        baseWallet = new ethers.Wallet(privateKey);
        baseWalletCache.set(privateKey, baseWallet);
    }

    // 2. Get Random Proxy & Provider
    const { getRandomProxy } = await import('./proxies.js');
    const proxy = getRandomProxy();
    const proxyIp = getProxyIp(proxy);
    const provider = createProvider(proxy);

    // 3. Connect Provider (Cheap Op)
    const wallet = baseWallet.connect(provider);

    const result = { wallet, proxy, proxyIp, index };
    return result;
}

export function loadCreatedTokens() {
    try {
        if (fs.existsSync(CREATED_TOKENS_FILE)) {
            const data = JSON.parse(fs.readFileSync(CREATED_TOKENS_FILE, 'utf8'));
            return data || {};
        }
    } catch (e) {
        console.error('Error loading created tokens:', e.message);
        return {};
    }
    return {};
}

export function saveCreatedToken(walletAddress, tokenAddress, symbol, name, blockNumber) {
    try {
        // Ensure data directory exists
        const dataDir = path.dirname(CREATED_TOKENS_FILE);
        if (!fs.existsSync(dataDir)) {
            fs.mkdirSync(dataDir, { recursive: true });
        }

        // Load existing tokens
        const allTokens = loadCreatedTokens();

        // Use checksum address
        const checksumAddress = ethers.getAddress(walletAddress);

        // Initialize array if not exists
        if (!allTokens[checksumAddress]) {
            allTokens[checksumAddress] = [];
        }

        // Add new token (prevent duplicates)
        const exists = allTokens[checksumAddress].some(t =>
            t.token.toLowerCase() === tokenAddress.toLowerCase()
        );

        if (!exists) {
            allTokens[checksumAddress].push({
                token: ethers.getAddress(tokenAddress),
                symbol: symbol || 'UNKNOWN',
                name: name || 'Unknown Token',
                timestamp: Date.now(),
                blockNumber: blockNumber ? blockNumber.toString() : '0'
            });

            // Save to file
            fs.writeFileSync(
                CREATED_TOKENS_FILE,
                JSON.stringify(allTokens, null, 2),
                'utf8'
            );

            return true;
        }

        return false; // Already exists
    } catch (e) {
        console.error('Error saving created token:', e.message);
        return false;
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MEME TOKEN TRACKING (Separate from Stablecoins)
// ═══════════════════════════════════════════════════════════════════════════════

export function loadCreatedMemes() {
    try {
        if (fs.existsSync(CREATED_MEMES_FILE)) {
            const data = JSON.parse(fs.readFileSync(CREATED_MEMES_FILE, 'utf8'));
            return data || {};
        }
    } catch (e) {
        console.error('Error loading created memes:', e.message);
        return {};
    }
    return {};
}

// ══════════════════════════════════════════════════════════════════════
// TASK COUNT TRACKING
// ══════════════════════════════════════════════════════════════════════

export function loadTaskCounts() {
    try {
        if (fs.existsSync(TASK_COUNTS_FILE)) {
            const data = JSON.parse(fs.readFileSync(TASK_COUNTS_FILE, 'utf8'));
            return data || {};
        }
    } catch (e) {
        console.error('Error loading task counts:', e.message);
        return {};
    }
    return {};
}

export function incrementTaskCount(walletAddress) {
    try {
        const dataDir = path.dirname(TASK_COUNTS_FILE);
        if (!fs.existsSync(dataDir)) {
            fs.mkdirSync(dataDir, { recursive: true });
        }

        const allCounts = loadTaskCounts();
        const checksumAddress = ethers.getAddress(walletAddress);

        if (!allCounts[checksumAddress]) {
            allCounts[checksumAddress] = 0;
        }

        allCounts[checksumAddress]++;

        fs.writeFileSync(
            TASK_COUNTS_FILE,
            JSON.stringify(allCounts, null, 2),
            'utf8'
        );

        return allCounts[checksumAddress];
    } catch (e) {
        console.error('Error incrementing task count:', e.message);
        return null;
    }
}

export function getTaskCount(walletAddress) {
    const allCounts = loadTaskCounts();
    const checksumAddress = ethers.getAddress(walletAddress);
    return allCounts[checksumAddress] || 0;
}

export function saveCreatedMeme(walletAddress, tokenAddress, symbol, name, blockNumber) {
    try {
        // Ensure data directory exists
        const dataDir = path.dirname(CREATED_MEMES_FILE);
        if (!fs.existsSync(dataDir)) {
            fs.mkdirSync(dataDir, { recursive: true });
        }

        // Load existing memes
        const allMemes = loadCreatedMemes();

        // Use checksum address
        const checksumAddress = ethers.getAddress(walletAddress);

        // Initialize array if not exists
        if (!allMemes[checksumAddress]) {
            allMemes[checksumAddress] = [];
        }

        // Add new meme (prevent duplicates)
        const exists = allMemes[checksumAddress].some(m =>
            m.token.toLowerCase() === tokenAddress.toLowerCase()
        );

        if (!exists) {
            allMemes[checksumAddress].push({
                token: ethers.getAddress(tokenAddress),
                symbol: symbol || 'UNKNOWN',
                name: name || 'Unknown Meme',
                timestamp: Date.now(),
                blockNumber: blockNumber ? blockNumber.toString() : '0'
            });

            // Save to file
            fs.writeFileSync(
                CREATED_MEMES_FILE,
                JSON.stringify(allMemes, null, 2),
                'utf8'
            );

            return true;
        }

        return false; // Already exists
    } catch (e) {
        console.error('Error saving created meme:', e.message);
        return false;
    }
}

export function validateProxyConfig() {
    const keys = getPrivateKeys();
    const proxies = require('./proxies.js').getProxies(); // Dynamic import to avoid circular dependency issues if any

    return {
        keysCount: keys.length,
        proxiesCount: proxies.length,
        match: proxies.length >= keys.length
    };
}
