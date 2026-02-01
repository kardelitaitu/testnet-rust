import { parseUnits, encodeFunctionData, keccak256, toHex, createWalletClient, custom, publicActions } from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import { tempoModerato } from 'viem/chains';
import { tempoActions } from './tempo/index.js';
import { CONFIG } from './constants.js';
import fs from 'fs';

// Minimal TIP20 ABI for scheduling
const TIP20_ABI = [
    {
        name: 'transfer',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [{ name: 'to', type: 'address' }, { name: 'amount', type: 'uint256' }],
        outputs: [{ name: '', type: 'bool' }]
    },
    {
        name: 'transferWithMemo',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [{ name: 'to', type: 'address' }, { name: 'amount', type: 'uint256' }, { name: 'memo', type: 'bytes32' }],
        outputs: [{ name: '', type: 'bool' }]
    }
];

function stringToBytes32(str) {
    if (!str) return '0x0000000000000000000000000000000000000000000000000000000000000000';
    return toHex(str, { size: 32 });
}

class ValidationError extends Error {
    constructor(message) {
        super(message);
        this.name = 'ValidationError';
    }
}

/**
 * Service for managing scheduled payments on the Tempo blockchain.
 */
export class ScheduleService {
    constructor(wallet) {
        this.schedules = new Map();
        this.idCounter = 0;

        // Use the private key from the ethers wallet
        const account = privateKeyToAccount(wallet.privateKey);

        // Bridge Ethers Provider to Viem Transport
        // This leverages the robust proxy/rotation logic in utils/wallet.js
        const transport = custom({
            async request({ method, params }) {
                try {
                    return await wallet.provider.send(method, params ?? []);
                } catch (error) {
                    throw error;
                }
            }
        });

        const baseClient = createWalletClient({
            account,
            chain: tempoModerato,
            transport
        }).extend(publicActions);

        this.client = baseClient.extend(tempoActions);
        this.account = account;
    }

    /**
     * Create a scheduled payment.
     */
    async createSchedule(params) {
        console.log("DEBUG: Entered createSchedule");
        try {
            const {
                tokenAddress,
                tokenSymbol,
                decimals = 18,
                to,
                amount,
                memo,
                executeAt, // Date or string
                validFrom,
                validUntil,
                recurring = false
            } = params;

            if (!(executeAt instanceof Date)) {
                throw new Error('executeAt must be a Date object');
            }

            const amountWei = parseUnits(amount.toString(), decimals);

            const data = memo
                ? encodeFunctionData({
                    abi: TIP20_ABI,
                    functionName: 'transferWithMemo',
                    args: [to, amountWei, stringToBytes32(memo)],
                })
                : encodeFunctionData({
                    abi: TIP20_ABI,
                    functionName: 'transfer',
                    args: [to, amountWei],
                });

            const executeTimestamp = BigInt(Math.floor(executeAt.getTime() / 1000));
            const validFromTimestamp = validFrom ? BigInt(Math.floor(new Date(validFrom).getTime() / 1000)) : executeTimestamp;

            // Build Tempo Type 0x76 Transaction (Batch/Scheduled)
            const txParams = {
                account: this.account,
                type: 118, // 0x76
                calls: [{
                    to: tokenAddress,
                    data: data,
                    value: 0n
                }],
                validAfter: validFromTimestamp
            };

            if (validUntil) {
                txParams.validBefore = BigInt(Math.floor(new Date(validUntil).getTime() / 1000));
            }

            console.log(`DEBUG: Sending Type 118 Tx: validAfter=${txParams.validAfter}`);

            const transactionHash = await this.client.sendTransaction(txParams);

            if (!transactionHash) {
                throw new Error("Failed to send scheduled transaction");
            }

            // Generate unique schedule ID
            const scheduleId = this.generateScheduleId(transactionHash);

            const record = {
                id: scheduleId,
                tokenAddress,
                tokenSymbol,
                to,
                amount,
                memo,
                executeAt,
                validFrom,
                validUntil,
                recurring,
                status: 'scheduled', // On-chain status might be 'pending' until validAfter
                createdAt: new Date(),
                transactionHash
            };

            this.schedules.set(scheduleId, record);
            return record;
        } catch (error) {
            console.error('Schedule creation failed:', error);
            throw error;
        }
    }

    async getScheduleStatus(scheduleId) {
        const record = this.schedules.get(scheduleId);
        if (!record) throw new ValidationError(`Schedule not found: ${scheduleId}`);

        try {
            const receipt = await this.client.getTransactionReceipt({ hash: record.transactionHash });
            if (receipt && receipt.status === 'success') {
                record.status = 'executed';
                return 'executed';
            }
        } catch { /* ignore */ }
        return record.status;
    }

    getAllSchedules() {
        return Array.from(this.schedules.values());
    }

    getSchedule(scheduleId) {
        return this.schedules.get(scheduleId);
    }

    async cancelSchedule(scheduleId) {
        const record = this.schedules.get(scheduleId);
        if (record) record.status = 'cancelled';
        return true;
    }

    generateScheduleId(transactionHash) {
        this.idCounter += 1;
        const hashPart = transactionHash.slice(2, 10);
        return `sched_${hashPart}_${this.idCounter}`;
    }
}
