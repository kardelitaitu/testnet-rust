import { CONFIG } from './constants.js';

export class RateLimiter {
    constructor() {
        const config = CONFIG.RATE_LIMIT || {
            maxConcurrentRequests: 10,
            maxRequestsPerMinute: 60,
            interval: 60000
        };

        this.maxConcurrent = config.maxConcurrentRequests;
        this.interval = 60000; // 1 minute
        this.intervalCap = config.maxRequestsPerMinute;

        this.activeRequests = 0;
        this.queue = [];
        this.requestTimestamps = []; // For interval cap

        this.walletStats = new Map(); // address -> { minute: [timestamps], hour: [timestamps] }

        // Success tracking for adaptive delays
        this.totalRequests = 0;
        this.failedRequests = 0;
    }

    async schedule(fn, walletAddress = null) {
        return new Promise((resolve, reject) => {
            this.queue.push({ fn, walletAddress, resolve, reject });
            this.process();
        });
    }

    async process() {
        // 1. Cleanup old timestamps
        const now = Date.now();
        this.requestTimestamps = this.requestTimestamps.filter(t => now - t < this.interval);

        // 2. Check Global Limits
        if (this.activeRequests >= this.maxConcurrent) return;
        if (this.requestTimestamps.length >= this.intervalCap) {
            // Wait until oldest slot frees up
            if (this.requestTimestamps.length > 0) {
                const oldest = this.requestTimestamps[0];
                const wait = (oldest + this.interval) - now;
                if (wait > 0) {
                    setTimeout(() => this.process(), wait);
                    return;
                }
            }
        }

        if (this.queue.length === 0) return;

        // 3. Find next runnable item (checking per-wallet limits)
        let itemIndex = -1;

        for (let i = 0; i < this.queue.length; i++) {
            const item = this.queue[i];
            if (!item.walletAddress) {
                itemIndex = i;
                break;
            }
            if (this.canWalletProceed(item.walletAddress)) {
                itemIndex = i;
                break;
            }
        }

        if (itemIndex === -1) {
            // All items blocked by wallet limits, try again later
            setTimeout(() => this.process(), 1000);
            return;
        }

        const { fn, walletAddress, resolve, reject } = this.queue.splice(itemIndex, 1)[0];

        this.activeRequests++;
        this.requestTimestamps.push(Date.now());
        if (walletAddress) this.trackWalletRequest(walletAddress);

        try {
            this.totalRequests++;
            const result = await fn();
            resolve(result);
        } catch (error) {
            this.failedRequests++;
            reject(error);
        } finally {
            this.activeRequests--;
            this.process();
        }
    }

    canWalletProceed(address) {
        const config = CONFIG.RATE_LIMIT;
        if (!config) return true;

        const stats = this.walletStats.get(address);
        if (!stats) return true;

        const now = Date.now();

        // Cleanup
        stats.minute = stats.minute.filter(t => now - t < 60000);
        stats.hour = stats.hour.filter(t => now - t < 3600000);

        if (stats.minute.length >= config.maxRequestsPerWalletPerMinute) return false;
        if (stats.hour.length >= config.maxRequestsPerWalletPerHour) return false;

        return true;
    }

    trackWalletRequest(address) {
        if (!address) return;

        if (!this.walletStats.has(address)) {
            this.walletStats.set(address, { minute: [], hour: [] });
        }

        const stats = this.walletStats.get(address);
        const now = Date.now();
        stats.minute.push(now);
        stats.hour.push(now);
    }

    getSuccessRate() {
        if (this.totalRequests === 0) return 1;
        return (this.totalRequests - this.failedRequests) / this.totalRequests;
    }

    getAdaptiveDelayMultiplier() {
        const rate = this.getSuccessRate();
        const config = CONFIG.RATE_LIMIT;
        if (rate < 0.7 && config) {
            return config.adaptiveMultiplier || 1.5;
        }
        return 1.0;
    }
}

// Global instance
export const globalLimiter = new RateLimiter();
