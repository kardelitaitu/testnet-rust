import fs from 'fs';
import path from 'path';

/**
 * specialized BatchWriter to handle high-frequency writes by buffering them in memory
 * and flushing to disk periodically or when buffer limit is reached.
 */
export class BatchWriter {
    constructor(filePath, options = {}) {
        this.filePath = filePath;
        this.flushInterval = options.flushInterval || 30000; // 30 seconds default
        this.maxBufferSize = options.maxBufferSize || 100;
        this.buffer = {};
        this.timer = null;
        this.isFlushing = false;

        // Ensure directory exists
        const dir = path.dirname(filePath);
        if (!fs.existsSync(dir)) {
            fs.mkdirSync(dir, { recursive: true });
        }

        // Initial load
        this.load();
    }

    load() {
        try {
            if (fs.existsSync(this.filePath)) {
                const data = fs.readFileSync(this.filePath, 'utf8');
                this.buffer = JSON.parse(data);
            }
        } catch (e) {
            this.buffer = {};
            console.error(`Error loading ${path.basename(this.filePath)}: ${e.message}`);
        }
    }

    /**
     * Update a key in the buffer. 
     * @param {string} key 
     * @param {any} value 
     * @param {boolean} merge - if true, assumes value is object and merges it
     */
    update(key, value, merge = false) {
        if (merge && this.buffer[key] && typeof value === 'object') {
            this.buffer[key] = { ...this.buffer[key], ...value };
        } else {
            this.buffer[key] = value;
        }

        this.scheduleFlush();
    }

    /**
     * Update nested metrics specifically for task counts
     */
    updateMetric(taskName, isSuccess, duration) {
        if (!this.buffer[taskName]) {
            this.buffer[taskName] = {
                success: 0,
                failed: 0,
                total: 0,
                totalDuration: 0,
                lastRun: 0
            };
        }

        this.buffer[taskName].total++;
        if (isSuccess) {
            this.buffer[taskName].success++;
            this.buffer[taskName].totalDuration += duration || 0;
        } else {
            this.buffer[taskName].failed++;
        }
        this.buffer[taskName].lastRun = Date.now();

        this.scheduleFlush();
    }

    /**
     * Special handler for arrays (e.g. created tokens list)
     * @param {string} key - Wallet Address
     * @param {Object} item - Token Object
     */
    appendToArray(key, item) {
        if (!this.buffer[key]) {
            this.buffer[key] = [];
        }

        // Check for duplicates in memory buffer
        // Note: This assumes buffer contains ALL data. If file is huge, this approach needs DB.
        // For ~5000 lines JSON it's fine.
        const exists = this.buffer[key].some(t =>
            t.token && item.token && t.token.toLowerCase() === item.token.toLowerCase()
        );

        if (!exists) {
            this.buffer[key].push(item);
            this.scheduleFlush();
            return true;
        }
        return false;
    }

    /**
    * Increment a counter
    * @param {string} key 
    */
    increment(key) {
        if (!this.buffer[key]) this.buffer[key] = 0;
        this.buffer[key]++;
        this.scheduleFlush();
        return this.buffer[key];
    }

    get(key) {
        return this.buffer[key];
    }

    scheduleFlush() {
        if (this.timer) return;
        this.timer = setTimeout(() => this.flush(), this.flushInterval);
    }

    async flush() {
        if (this.isFlushing) return;
        this.isFlushing = true;

        try {
            // Atomic write
            const tempFile = `${this.filePath}.tmp`;
            const data = JSON.stringify(this.buffer, null, 2);
            await fs.promises.writeFile(tempFile, data, 'utf8');
            await fs.promises.rename(tempFile, this.filePath);

            // console.log(`[BatchWriter] Flushed ${path.basename(this.filePath)}`);
        } catch (e) {
            console.error(`[BatchWriter] Error flushing ${path.basename(this.filePath)}:`, e.message);
        } finally {
            this.isFlushing = false;
            clearTimeout(this.timer);
            this.timer = null;
        }
    }
}
