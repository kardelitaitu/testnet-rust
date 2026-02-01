import { ethers } from 'ethers';
import { Transaction } from 'viem/tempo';
import { CONFIG } from './constants.js';

const ERC20_ABI = [
    "function balanceOf(address owner) view returns (uint256)",
    "function decimals() view returns (uint8)",
    "function transfer(address to, uint256 amount) returns (bool)",
    "function symbol() view returns (string)"
];

/**
 * Service to handle native Tempo Transaction Type 0x76 (Delegation/Batch/Scheduled).
 */
export class TempoSDKService {
    // Static cache for persistence across instances
    static nonceCache = new Map();

    constructor(wallet) {
        this.wallet = wallet;
    }

    // Helper to get last nonce
    getLastNonce() {
        return TempoSDKService.nonceCache.get(this.wallet.address) || -1n;
    }

    // Helper to set last nonce
    setLastNonce(nonce) {
        TempoSDKService.nonceCache.set(this.wallet.address, nonce);
    }

    /**
     * Creates and sends a scheduled transfer using Transaction Type 0x76.
     */
    async createScheduledTransfer(tokenAddress, amountWei, recipientAddress, executeAt, feeTokenSymbol = 'PathUSD') {
        const chainId = Number(CONFIG.CHAIN_ID || 42431);
        const validAfter = Math.floor(executeAt.getTime() / 1000);
        const validBefore = validAfter + 300; // 5 minute window

        // Resolve Fee Token Address
        const feeToken = CONFIG.TOKENS[feeTokenSymbol] || CONFIG.TOKENS['PathUSD'];

        // 1. Encode TIP-20 transfer data
        const tokenInterface = new ethers.Interface(ERC20_ABI);
        const data = tokenInterface.encodeFunctionData('transfer', [recipientAddress, amountWei]);

        // 2. Prepare Transaction Object for Viem/Tempo
        let maxFee = ethers.parseUnits('10', 'gwei');
        let priorityFee = ethers.parseUnits('1', 'gwei');

        try {
            const feeData = await this.wallet.provider.getFeeData();
            if (feeData.maxFeePerGas) {
                // Apply GAS_PRICE_MULTIPLIER if available
                const multiplier = BigInt(Math.floor((CONFIG.GAS_PRICE_MULTIPLIER || 1.5) * 100));
                maxFee = (feeData.maxFeePerGas * multiplier) / 100n;
            }
            if (feeData.maxPriorityFeePerGas) {
                priorityFee = feeData.maxPriorityFeePerGas;
            }
        } catch (e) {
            // Fallback to defaults already set
        }

        const gas = BigInt(CONFIG.GAS_LIMIT || 500000);

        // Use a random nonceKey to allow parallel transactions without replacement errors
        const nonceKey = BigInt(Math.floor(Math.random() * 1000000));

        // For Tempo Parallel Nonces (nonceKey > 0), the typical start is 0
        // If we use wallet.getNonce(), it returns the nonce for nonceKey 0
        let nonce = 0n;
        if (nonceKey === 0n) {
            // Logic for NonceKey 0 if used
            let networkNonce = BigInt(await this.wallet.getNonce());
            const lastLocal = this.getLastNonce();

            if (lastLocal >= networkNonce) {
                nonce = lastLocal + 1n;
            } else {
                nonce = networkNonce;
            }
            this.setLastNonce(nonce);
        }

        const tx = {
            chainId,
            maxPriorityFeePerGas: priorityFee,
            maxFeePerGas: maxFee,
            gas,
            feeToken,
            calls: [
                {
                    to: tokenAddress,
                    value: 0n,
                    data: data
                }
            ],
            nonce: Number(nonce),
            nonceKey,
            validBefore,
            validAfter
        };

        // 3. Obtain Signature Hash
        const serializedUnsigned = await Transaction.serialize(tx);
        const msgHash = ethers.keccak256(serializedUnsigned);

        const signature = this.wallet.signingKey.sign(msgHash);
        const sig = {
            r: signature.r,
            s: signature.s,
            yParity: signature.v === 28 ? 1 : 0
        };

        // 4. Final Serialized with Signature
        const signedTx = await Transaction.serialize(tx, sig);

        // 5. Send Raw Transaction
        const txHash = await this.wallet.provider.send('eth_sendRawTransaction', [signedTx]);

        return {
            scheduleId: `tempo-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
            transactionHash: txHash
        };
    }
    /**
     * Sends an atomic batch transaction.
     * @param {Array<{to: string, value: bigint, data: string}>} calls - Array of call objects
     * @param {string} feeTokenSymbol - Symbol of token to pay gas with
     */
    async sendBatchTransaction(calls, feeTokenSymbol = 'PathUSD', gasLimit = null) {
        const chainId = Number(CONFIG.CHAIN_ID || 42431);
        const feeToken = CONFIG.TOKENS[feeTokenSymbol] || CONFIG.TOKENS['PathUSD'];

        let maxFee = ethers.parseUnits('10', 'gwei');
        let priorityFee = ethers.parseUnits('1', 'gwei');

        try {
            const feeData = await this.wallet.provider.getFeeData();
            if (feeData.maxFeePerGas) {
                const multiplier = BigInt(Math.floor((CONFIG.GAS_PRICE_MULTIPLIER || 1.5) * 100));
                maxFee = (feeData.maxFeePerGas * multiplier) / 100n;
            }
            if (feeData.maxPriorityFeePerGas) priorityFee = feeData.maxPriorityFeePerGas;
        } catch (e) { }

        const nonceKey = 0n; // Use sequential nonce for stability

        let retries = 5;
        let attempt = 0;

        while (attempt < retries) {
            attempt++;
            try {
                // 🛠️ NONCE FIX: Handle Race Condition
                let networkNonce = BigInt(await this.wallet.provider.getTransactionCount(this.wallet.address, 'pending'));
                let nonce;
                const lastLocal = this.getLastNonce();

                if (lastLocal >= networkNonce) {
                    nonce = lastLocal + 1n;
                } else {
                    nonce = networkNonce;
                }
                this.setLastNonce(nonce);

                // Ensure calls are properly formatted
                const formattedCalls = calls.map(c => ({
                    to: c.to,
                    value: c.value || 0n,
                    data: c.data || '0x'
                }));

                const now = Math.floor(Date.now() / 1000);

                // Estimate Gas or Fallback
                let gas = 200000n + (100000n * BigInt(calls.length));
                if (gasLimit) {
                    gas = BigInt(gasLimit);
                }

                const tx = {
                    chainId,
                    maxPriorityFeePerGas: priorityFee,
                    maxFeePerGas: maxFee,
                    gas,
                    feeToken,
                    calls: formattedCalls,
                    nonce: Number(nonce),
                    nonceKey,
                    validBefore: now + 3600,
                    validAfter: now - 60
                };

                const serializedUnsigned = await Transaction.serialize(tx);
                const msgHash = ethers.keccak256(serializedUnsigned);
                const signature = this.wallet.signingKey.sign(msgHash);
                const sig = { r: signature.r, s: signature.s, yParity: signature.v === 28 ? 1 : 0 };

                const signedTx = await Transaction.serialize(tx, sig);
                const txHash = await this.wallet.provider.send('eth_sendRawTransaction', [signedTx]);

                return { transactionHash: txHash };

            } catch (error) {
                const msg = error?.message?.toLowerCase() || '';
                // Check specific retryable errors
                if (msg.includes('nonce') || msg.includes('underpriced') || msg.includes('already known')) {
                    if (attempt === retries) throw error;
                    // Force a small delay to let network settle
                    await new Promise(r => setTimeout(r, 1000 * attempt));
                    continue;
                }
                throw error;
            }
        }
    }
}
