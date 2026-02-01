import fs from 'fs';
import path from 'path';
import { createPublicClient, createWalletClient, http, encodeFunctionData, parseUnits, isAddress } from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import { tempoModerato } from 'viem/chains';
import { tempoActions } from 'viem/tempo';
import { CONFIG, COLORS } from './constants.js';

const TRACKER_FILE = path.resolve(process.cwd(), 'data', 'nonce-tracker.json');
const transactionDelayMs = 500;

import { getProxyAgent } from './proxies.js';
import axios from 'axios';
import { getGasWithMultiplier } from './helpers.js';

export class ConcurrentService {
    constructor(privateKey, proxy = null) {
        if (!privateKey) throw new Error("Private key is required for ConcurrentService");

        this.account = privateKeyToAccount(privateKey.startsWith('0x') ? privateKey : `0x${privateKey}`);

        // Setup Chain
        const feeToken = CONFIG.TOKENS?.PathUSD || CONFIG.TOKENS?.AlphaUSD || "0x20c0000000000000000000000000000000000001";
        this.chain = {
            ...tempoModerato,
            id: Number(CONFIG.CHAIN_ID)
        };

        // Custom Fetch for Proxy Support
        const customFetch = async (url, options) => {
            const agent = proxy ? getProxyAgent(proxy) : null;
            const response = await axios({
                method: options.method || 'POST',
                url: url.toString(),
                data: options.body,
                headers: JSON.parse(options.headers || '{}'),
                httpsAgent: agent,
                httpAgent: agent,
                timeout: 30000,
                validateStatus: () => true
            });

            return {
                ok: response.status >= 200 && response.status < 300,
                status: response.status,
                statusText: response.statusText,
                headers: new Headers(response.headers),
                json: async () => response.data,
                text: async () => JSON.stringify(response.data)
            };
        };

        const transport = http(CONFIG.RPC_URL, { fetch: customFetch });

        // Setup Clients
        this.publicClient = createPublicClient({
            chain: this.chain,
            transport
        });

        this.walletClient = createWalletClient({
            account: this.account,
            chain: this.chain,
            transport
        }).extend(tempoActions());

        this.feeToken = feeToken;
    }

    getAddress() {
        return this.account.address;
    }

    async getNonceForKey(nonceKey) {
        if (nonceKey === 0) {
            const nonce = await this.publicClient.getTransactionCount({
                address: this.getAddress(),
                blockTag: 'pending',
            });
            return BigInt(nonce);
        }
        return 0n; // Parallel keys usually start fresh or managed manually
    }

    async sendConcurrentTransaction(params) {
        // Fetch optimal gas if not provided
        let gasOverrides = {};
        if (!params.maxFeePerGas && !params.gasPrice) {
            try {
                // Simplified simplified fetch matching logic in helpers but using viem
                const feeData = await this.publicClient.estimateFeesPerGas();
                const multiplier = CONFIG.GAS_PRICE_MULTIPLIER || 1.1; // Default 1.1 check constants if needed

                if (feeData.maxFeePerGas) {
                    gasOverrides.maxFeePerGas = (feeData.maxFeePerGas * BigInt(Math.floor(multiplier * 100))) / 100n;
                    gasOverrides.maxPriorityFeePerGas = (feeData.maxPriorityFeePerGas * BigInt(Math.floor(multiplier * 100))) / 100n;
                } else if (feeData.gasPrice) {
                    gasOverrides.gasPrice = (feeData.gasPrice * BigInt(Math.floor(multiplier * 100))) / 100n;
                }
            } catch (e) {
                // metrics failed, ignore
            }
        }

        // Prepare The Transaction Request
        const txRequest = {
            account: this.account,
            chain: this.chain,
            type: 0x76, // Tempo Type
            to: params.to,
            data: params.data,
            value: params.value || 0n,
            nonce: params.nonce, // 0 is valid for fresh nonceKey
            nonceKey: BigInt(params.nonceKey),
            feeToken: params.feeToken || this.feeToken,
            gas: 500000n, // Base gas default
            calls: params.calls, // Atomic Batch support
            ...gasOverrides,
            ...params
        };

        // If batching, gas needs to be higher
        if (params.calls && params.calls.length > 0) {
            // 500k per call + 500k base buffer should be safe for almost anything
            const calculatedGas = BigInt(500000 * params.calls.length + 500000);

            // If params.gas is provided (manual override), use it. Otherwise use calculated.
            if (!params.gas) {
                txRequest.gas = calculatedGas;
            }
        }

        // Sign It (Bypasses "nonce too low" pre-checks if any)
        const signedTx = await this.walletClient.signTransaction(txRequest);

        // Broadcast Raw
        return this.publicClient.sendRawTransaction({
            serializedTransaction: signedTx
        });
    }

    async sendAtomicBatch(calls, nonceKey, feeToken = null, extraParams = {}) {
        // Ensure nonceKey is set
        const finalNonceKey = nonceKey || BigInt(Date.now());
        const nonce = await this.getNonceForKey(finalNonceKey);

        return this.sendConcurrentTransaction({
            to: "0x0000000000000000000000000000000000000000", // Batch often uses 0x0 or special address, but 0x76 usually ignores top-level 'to' if 'calls' is present
            data: "0x",
            nonceKey: finalNonceKey,
            nonce: Number(nonce),
            calls: calls,
            feeToken: feeToken,
            ...extraParams
        });
    }

    async sendConcurrentPayments(payments, startNonceKey = 1, waitForReceipts = true) {
        const submissions = [];
        const nonces = [];

        // 1. Prepare Nonces
        for (let i = 0; i < payments.length; i++) {
            nonces.push(await this.getNonceForKey(startNonceKey + i));
        }

        // 2. Pre-fetch Gas Fees (Optimization)
        let gasOverrides = {};
        try {
            const feeData = await this.publicClient.estimateFeesPerGas();
            const multiplier = CONFIG.GAS_PRICE_MULTIPLIER || 1.1;

            if (feeData.maxFeePerGas) {
                gasOverrides.maxFeePerGas = (feeData.maxFeePerGas * BigInt(Math.floor(multiplier * 100))) / 100n;
                gasOverrides.maxPriorityFeePerGas = (feeData.maxPriorityFeePerGas * BigInt(Math.floor(multiplier * 100))) / 100n;
            } else if (feeData.gasPrice) {
                gasOverrides.gasPrice = (feeData.gasPrice * BigInt(Math.floor(multiplier * 100))) / 100n;
            }
        } catch (e) {
            // If optimization fails, fallback to per-tx fetching by leaving overrides empty
        }

        // 3. Submit Sequential with small delay
        for (let i = 0; i < payments.length; i++) {
            const payment = payments[i];
            const nonceKey = startNonceKey + i;
            const nonce = nonces[i];

            if (i > 0) await new Promise(r => setTimeout(r, transactionDelayMs));

            try {
                const data = payment.data; // Pre-encoded
                // If payment has explicit feeToken, use it, otherwise use default
                const result = await this.sendConcurrentTransaction({
                    to: payment.to,
                    data,
                    nonceKey,
                    nonce: Number(nonce),
                    // If payment overrides feeToken, pass it
                    ...(payment.feeToken ? { feeToken: payment.feeToken } : {}),
                    // Pass pre-calculated gas overrides
                    ...gasOverrides
                });

                // Result is hash string if using sendRawTransaction?
                // viem sendRawTransaction returns `Hash` (string).
                // My wrapper `sendConcurrentTransaction` returns the result of `sendRawTransaction`.
                // So it is string.

                const hash = result; // It is a string
                submissions.push({ nonceKey, hash, status: 'pending' });
            } catch (error) {
                submissions.push({ nonceKey, hash: null, status: 'failed', error: error.message || 'Unknown error' });
            }
        }

        // 3. Wait for confirmations (Optional)
        if (!waitForReceipts) {
            return submissions.map(s => ({
                ...s,
                status: s.status === 'pending' ? 'broadcasted' : 'failed'
            }));
        }

        const results = await Promise.all(submissions.map(async (sub) => {
            if (sub.status === 'failed') return sub;
            try {
                await this.publicClient.waitForTransactionReceipt({ hash: sub.hash, timeout: 60000 });
                return { ...sub, status: 'confirmed' };
            } catch (e) {
                return { ...sub, status: 'failed', error: e.message || 'Timeout' };
            }
        }));

        return results;
    }
}

export function loadNonceKey(address) {
    if (!fs.existsSync(TRACKER_FILE)) return 1;
    try {
        const data = JSON.parse(fs.readFileSync(TRACKER_FILE, 'utf-8'));
        return data[address] || 1;
    } catch (e) {
        return 1;
    }
}

export function saveNonceKey(address, nextKey) {
    let data = {};
    if (!fs.existsSync(path.dirname(TRACKER_FILE))) fs.mkdirSync(path.dirname(TRACKER_FILE), { recursive: true });
    if (fs.existsSync(TRACKER_FILE)) {
        try {
            data = JSON.parse(fs.readFileSync(TRACKER_FILE, 'utf-8'));
        } catch (e) { }
    }
    data[address] = nextKey;
    fs.writeFileSync(TRACKER_FILE, JSON.stringify(data, null, 2));
}
