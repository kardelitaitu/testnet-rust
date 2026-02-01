import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, getRandomInt } from '../utils/helpers.js';
import { claimFaucetForWallet } from './2_claimFaucet.js';

export class SmartWallet {
    constructor(index, privateKey, useProxy = true) {
        this.index = index;
        this.privateKey = privateKey;
        this.useProxy = useProxy;
        // Wallet initialization is async in our utils, but constructor cannot be async.
        // We'll init lazily or require an async init method.
        this.wallet = null; // ethers.Wallet instance
        this.provider = null;
        this.address = null;
        this.proxy = null;
        this.initialized = false;

        this.balances = {
            ETH: 0,
            PathUSD: 0,
            AlphaUSD: 0
        };

        this.history = {
            createdTokens: [],
            lastAction: null
        };
        this.shouldRecoverFaucet = false;
    }

    async init() {
        if (this.initialized) return;
        const { wallet, proxy } = await getWallet(this.index, this.privateKey);
        this.wallet = wallet;
        this.provider = wallet.provider;
        this.address = wallet.address;
        this.proxy = this.useProxy ? proxy : null;
        this.initialized = true;
    }

    async updateBalances() {
        try {
            const ethBalance = await this.provider.getBalance(this.address);
            this.balances.ETH = parseFloat(ethers.formatEther(ethBalance));

            // TODO: check token balances
        } catch (error) {
            // console.warn(`Balance check failed: ${error.message}`);
            // Silent fail or log debug
            this.balances.ETH = 0;
        }
    }

    async decideNextAction() {
        // Recovery Mode
        if (this.shouldRecoverFaucet) {
            this.shouldRecoverFaucet = false;
            return {
                name: 'Faucet Claim (Recovery)',
                fn: () => claimFaucetForWallet(this.wallet, this.proxy, 1, this.index + 1, this.index),
                reason: "Recovering from Insufficient Funds"
            };
        }

        await this.updateBalances();

        const eth = this.balances.ETH;

        // Priority 1: Faucet if low ETH
        if (eth < 2.0) {
            return {
                name: 'Faucet Claim (Low Balance)',
                fn: () => claimFaucetForWallet(this.wallet, this.proxy, 1, this.index + 1, this.index),
                reason: `Low ETH: ${eth.toFixed(4)} < 2.0`
            };
        }

        // Priority 2: Random (Simulation of other activities)
        // Since we only have Faucet implemented:
        const roll = Math.random();

        if (roll < 0.3) {
            return {
                name: 'Faucet Claim (Activity)',
                fn: () => claimFaucetForWallet(this.wallet, this.proxy, 1, this.index + 1, this.index),
                reason: "Activity generation"
            };
        }

        return {
            name: 'Idle / Sleep',
            fn: async () => { await sleep(2000); return { success: true, result: 'slept' }; },
            reason: "Cooling down"
        };
    }
}
