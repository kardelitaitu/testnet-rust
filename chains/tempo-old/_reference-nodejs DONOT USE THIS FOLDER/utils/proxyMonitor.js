import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const AUDIT_FILE = path.join(ROOT_DIR, 'data', 'proxy_audit.json');

// Ensure data dir exists
if (!fs.existsSync(path.dirname(AUDIT_FILE))) {
    fs.mkdirSync(path.dirname(AUDIT_FILE), { recursive: true });
}

function loadAuditData() {
    for (let i = 0; i < 3; i++) {
        try {
            if (fs.existsSync(AUDIT_FILE)) {
                const data = fs.readFileSync(AUDIT_FILE, 'utf-8');
                if (!data.trim()) return {};
                return JSON.parse(data);
            }
            return {};
        } catch (e) {
            if (i === 2) {
                console.error('Error loading proxy audit data after retries:', e.message);
                try {
                    if (fs.existsSync(AUDIT_FILE)) {
                        fs.renameSync(AUDIT_FILE, AUDIT_FILE + '.corrupt.' + Date.now());
                        console.log('Corrupt audit file backed up and reset.');
                    }
                } catch (backupErr) { }
            } else {
                // Small sleep and retry
                const syncBuffer = fs.readFileSync(AUDIT_FILE); // try sync read
            }
        }
    }
    return {};
}

function saveAuditData(data) {
    try {
        fs.writeFileSync(AUDIT_FILE, JSON.stringify(data, null, 2), 'utf-8');
    } catch (e) {
        console.error('Error saving proxy audit data:', e.message);
    }
}

/**
 * Log a proxy attempt
 * @param {string} proxyUrl - The proxy connection string
 * @param {string} method - RPC method (e.g. eth_sendRawTransaction)
 * @param {boolean} success - Whether the request succeeded
 * @param {string} [txHash] - Optional transaction hash
 * @param {string} [error] - Error message if failed
 */
export function logProxyAttempt(proxyUrl, method, success, txHash = null, error = null) {
    if (!proxyUrl) return;

    const data = loadAuditData();
    const now = new Date().toISOString();

    // Normalize proxy key (remove user:pass for cleaner logs, or keep full?)
    // Let's keep full for uniqueness, but maybe strip for display later
    const key = proxyUrl;

    if (!data[key]) {
        data[key] = {
            ip: getIpFromProxy(proxyUrl),
            attempts: 0,
            successes: 0,
            failures: 0,
            lastSeen: null,
            lastStatus: null,
            lastTx: null,
            errors: [] // Store last 5 errors
        };
    }

    const record = data[key];

    // Check for Redemption Attempt (Returning after 24h probation)
    // If it was SUSPICIOUS and lastSeen was > 24h ago, this is a redemption try.
    const ONE_DAY_MS = 24 * 60 * 60 * 1000;
    const isRedemption = record.status === 'SUSPICIOUS' && record.lastSeen && (new Date() - new Date(record.lastSeen) > ONE_DAY_MS);

    if (success) {
        if (isRedemption) {
            // Redemption Successful! Wipe bad history.
            record.attempts = 1;
            record.failures = 0;
            record.successes = 1;
            record.status = 'OK';
            record.errors = [];
            // console.log(`[ProxyAudit] Proxy ${key} REDEEMED!`);
        } else {
            // Normal success
            record.successes++;
            record.attempts++;
        }

        record.lastStatus = 'active';
        if (txHash) record.lastTx = txHash;
    } else {
        if (isRedemption) {
            // Redemption Failed. It remains SUSPICIOUS.
            // We just update lastSeen, which effectively resets the 24h timer.
            // console.log(`[ProxyAudit] Proxy ${key} redemption failed.`);
        }

        record.failures++;
        record.attempts++;
        record.lastStatus = 'failed';
        if (error) {
            const errMsg = typeof error === 'object' ? error.message : String(error);
            if (!record.errors.includes(errMsg)) {
                record.errors.unshift(errMsg);
                if (record.errors.length > 5) record.errors.pop();
            }
        }
    }

    record.lastSeen = now;

    // Auto-mark as SUSPICIOUS if failure rate > 50% after 10 attempts
    // Only apply if it's NOT already suspicious (to avoid overwriting redemption logic flow)
    if (record.status !== 'SUSPICIOUS') {
        if (record.attempts > 10 && (record.failures / record.attempts) > 0.5) {
            record.status = 'SUSPICIOUS';
        } else {
            record.status = 'OK';
        }
    }

    saveAuditData(data);
}

function getIpFromProxy(proxyUrl) {
    try {
        if (proxyUrl.includes('@')) {
            return proxyUrl.split('@')[1].split(':')[0];
        }
        return proxyUrl.split(':')[0].replace(/.*\/\//, '');
    } catch (e) {
        return 'unknown';
    }
}

export function getProxyStats() {
    return loadAuditData();
}
