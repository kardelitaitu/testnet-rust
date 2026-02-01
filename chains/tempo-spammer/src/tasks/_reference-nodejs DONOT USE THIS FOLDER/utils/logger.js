import winston from 'winston';
import chalk from 'chalk';
import { incrementTaskCount } from './wallet.js';
import { getProxyForIndex, getProxyIp, getProxies } from './proxies.js';

const { combine, timestamp, printf } = winston.format;

const customFormat = printf(({ level, message, timestamp }) => {
    return `${timestamp} [${level.toUpperCase()}]: ${message}`;
});

export const logger = winston.createLogger({
    level: 'info',
    format: combine(
        timestamp({ format: 'YYYY-MM-DD HH:mm:ss' }),
        customFormat
    ),
    transports: [
        new winston.transports.File({
            filename: 'smart_main.log',
            maxsize: 2 * 1024 * 1024, // 2MB (approx 10k-15k lines)
            maxFiles: 1, // Keep 1 backup file
            tailable: true // Rotate by renaming logic
        }),
        // Console transport handled manually for custom formatting logic
    ],
});

function pad(num, size = 3) {
    let s = "000" + num;
    return s.substr(s.length - size);
}

// Map descriptive names to "filename.js" style tags if needed, or just use as is
// Map descriptive names to "filename.js" style tags if needed, or just use as is
function formatTaskName(actionName) {
    if (actionName && actionName.includes('.js')) return actionName;
    return actionName;
}

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename_logger = fileURLToPath(import.meta.url);
const __dirname_logger = path.dirname(__filename_logger);
const METRICS_FILE = path.join(__dirname_logger, '..', 'data', 'task_metrics.json');

function updateTaskMetrics(taskName, success, duration) {
    try {
        const dataDir = path.dirname(METRICS_FILE);
        if (!fs.existsSync(dataDir)) fs.mkdirSync(dataDir, { recursive: true });

        let metrics = {};
        if (fs.existsSync(METRICS_FILE)) {
            try {
                metrics = JSON.parse(fs.readFileSync(METRICS_FILE, 'utf8'));
            } catch (e) { metrics = {}; }
        }

        if (!metrics[taskName]) {
            metrics[taskName] = { total: 0, success: 0, failed: 0, totalDuration: 0 };
        }

        metrics[taskName].total += 1;
        if (success) {
            metrics[taskName].success += 1;
            if (duration) metrics[taskName].totalDuration += duration;
        } else {
            metrics[taskName].failed += 1;
        }

        fs.writeFileSync(METRICS_FILE, JSON.stringify(metrics, null, 2), 'utf8');
    } catch (e) {
        // Ignore metrics write errors to not crash bot
    }
}

export function logWalletAction(workerId, walletIndex, walletAddress, actionName, result = 'start', info = '', silent = false, durationSeconds = null, proxyIndex = null) {
    const workerTag = `WK:${String(workerId).padStart(3, '0')}`;
    const walletTag = `WL:${String(walletIndex + 1).padStart(3, '0')}`;

    // Proxy Tag Logic
    let proxyTag = 'DIRECT';
    if (proxyIndex !== null && typeof proxyIndex === 'number') {
        proxyTag = `P:${String(proxyIndex + 1).padStart(3, '0')}`;
    } else if (typeof proxyIndex === 'string') {
        const proxies = getProxies();
        const foundIndex = proxies.indexOf(proxyIndex);
        if (foundIndex !== -1) {
            proxyTag = `P:${String(foundIndex + 1).padStart(3, '0')}`;
        } else {
            // Fallback if IP or unknown proxy string is passed
            proxyTag = getProxyIp(proxyIndex);
        }
    }

    const taskTag = formatTaskName(actionName);

    // Color logic
    const prefix = `[${workerTag}][${walletTag}][${chalk.dim(proxyTag)}]`;
    const plainPrefix = `[${workerTag}][${walletTag}][${proxyTag}]`;

    // Add duration to info message if provided
    let finalInfo = info;
    if (durationSeconds !== null && result === 'success') {
        let durStr = `${durationSeconds.toFixed(1)}s`;
        if (durationSeconds > 10) durStr = chalk.red(durStr);
        else if (durationSeconds > 5) durStr = chalk.yellow(durStr);
        else durStr = chalk.cyan(durStr);

        finalInfo = `${info} in ${durStr}`;
    }

    let statusStr = '';
    let messageStr = '';

    if (result === 'start') {
        statusStr = chalk.gray('Starting');
        messageStr = finalInfo;
    } else if (result === 'success') {
        statusStr = chalk.green('Success');
        messageStr = finalInfo || 'Completed';
    } else if (result === 'failed') {
        // Truncate long error messages to keep single line
        let errorMsg = finalInfo || 'Unknown Error';
        if (errorMsg.length > 50) {
            errorMsg = errorMsg.substring(0, 50) + '...';
        }
        statusStr = chalk.red('Failed');
        messageStr = errorMsg;
    } else if (result === 'retry') {
        statusStr = chalk.yellow('Retrying');
        messageStr = finalInfo;
    } else if (result === 'skipped') {
        statusStr = chalk.magenta('Skipped');
        messageStr = finalInfo;
    }

    // Format: [WK][WL][P] Status [Task] Message
    const coloredOutput = `${prefix} ${statusStr} [${chalk.hex('#FFA500')(taskTag)}] ${messageStr}`;
    const plainOutput = `${plainPrefix} ${result.charAt(0).toUpperCase() + result.slice(1)} [${taskTag}] ${messageStr}`;

    // Only log if not silent (handles both Console and File to prevent duplicates)
    // The 'automatic-task.js' runner calls tasks with silent=true (preventing task internal log),
    // and then logs the result itself with silent=false (creating the single authoritative log).
    if (!silent) {
        console.log(coloredOutput);
        logger.info(plainOutput.replace(/\u001b\[\d+m/g, ''));
    }

    // Auto-increment task count on success
    if (result === 'success' && walletAddress) {
        try {
            incrementTaskCount(walletAddress);
        } catch (e) {
            // Silently fail if increment fails
        }
    }

    // Update global task metrics
    if (result === 'success' || result === 'failed') {
        updateTaskMetrics(taskTag, result === 'success', durationSeconds);
    }
}

