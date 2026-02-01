import { ethers } from 'ethers';
import inquirer from 'inquirer';
import { CONFIG, COLORS } from './constants.js';

// ... (existing code) ...

// Gas Multiplier Helper
export async function getGasWithMultiplier(provider, multiplier = CONFIG.GAS_PRICE_MULTIPLIER, wallet = null) {
    const feeData = await provider.getFeeData();

    let selectedFeeToken = undefined;

    if (wallet) {
        // Smart selection: Check balances first
        const feeTokens = [
            { address: CONFIG.TOKENS?.PathUSD, name: 'PathUSD' },
            { address: CONFIG.TOKENS?.AlphaUSD, name: 'AlphaUSD' },
            { address: CONFIG.TOKENS?.BetaUSD, name: 'BetaUSD' },
            { address: CONFIG.TOKENS?.ThetaUSD, name: 'ThetaUSD' }
        ].filter(t => t.address);

        const ABI = ["function balanceOf(address) view returns (uint256)"];
        const validTokens = [];

        // Check in parallel
        await Promise.all(feeTokens.map(async (t) => {
            try {
                const c = new ethers.Contract(t.address, ABI, wallet);
                const bal = await c.balanceOf(wallet.address);
                if (bal > 0n) validTokens.push(t.address);
            } catch (e) { }
        }));

        if (validTokens.length > 0) {
            selectedFeeToken = validTokens[Math.floor(Math.random() * validTokens.length)];
        }
    } else {
        // Fallback: Pick a random fee token from the list if checking isn't possible
        // But this is risky if balance is 0. But for backward compatibility or simple calls:
        const feeTokens = [
            CONFIG.TOKENS?.PathUSD,
            CONFIG.TOKENS?.AlphaUSD,
            CONFIG.TOKENS?.BetaUSD,
            CONFIG.TOKENS?.ThetaUSD
        ].filter(t => t);
        if (feeTokens.length > 0) {
            selectedFeeToken = feeTokens[Math.floor(Math.random() * feeTokens.length)];
        }
    }

    // Use legacy gasPrice for feeCurrency transactions
    if (selectedFeeToken) {
        const gasPrice = feeData.gasPrice
            ? (feeData.gasPrice * BigInt(Math.floor(multiplier * 100))) / 100n
            : undefined;
        return { gasPrice, feeCurrency: selectedFeeToken };
    }

    // Default EIP-1559 logic for native token
    const maxFeePerGas = feeData.maxFeePerGas
        ? (feeData.maxFeePerGas * BigInt(Math.floor(multiplier * 100))) / 100n
        : undefined;
    const maxPriorityFeePerGas = feeData.maxPriorityFeePerGas
        ? (feeData.maxPriorityFeePerGas * BigInt(Math.floor(multiplier * 100))) / 100n
        : undefined;

    return { maxFeePerGas, maxPriorityFeePerGas };
}

// Random integer between min and max (inclusive)
export function getRandomInt(min, max) {
    return Math.floor(Math.random() * (max - min + 1)) + min;
}

// Sleep function
export function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

// Countdown timer
export async function countdown(seconds, message = 'Waiting') {
    for (let i = seconds; i > 0; i--) {
        process.stdout.write(`\r${message} ${i}s...   `);
        await sleep(1000);
    }
    process.stdout.write(`\r${message} 0s...   \n`);
}

// Ask a question
export async function askQuestion(query) {
    const answers = await inquirer.prompt([
        {
            type: 'input',
            name: 'response',
            message: query,
        }
    ]);
    return answers.response;
}

// Ask password
export async function askPassword(message = 'Enter wallet password: ') {
    const answers = await inquirer.prompt([
        {
            type: 'password',
            name: 'password',
            message: message,
            mask: '*'
        }
    ]);
    return answers.password;
}

// Ask selection
export async function askSelection(message, choices) {
    const answers = await inquirer.prompt([
        {
            type: 'list',
            name: 'selection',
            message: message,
            choices: choices
        }
    ]);
    return answers.selection;
}

// Format Token Amount
export function formatAmount(amount, decimals = 18) {
    return (Number(amount) / 10 ** decimals).toFixed(4);
}

// Banner
export function showBanner() {
    console.clear();
    console.log(`${COLORS.fg.cyan}╔═══════════════════════════════════════════════════════════════╗${COLORS.reset}`);
    console.log(`${COLORS.fg.cyan}║                                                               ║${COLORS.reset}`);
    console.log(`${COLORS.fg.cyan}║           ${COLORS.fg.magenta}🚀  TEMPO TESTNET NODEJS  v2.0.1  🚀${COLORS.fg.cyan}                ║${COLORS.reset}`);
    console.log(`${COLORS.fg.cyan}║                                                               ║${COLORS.reset}`);
    console.log(`${COLORS.fg.cyan}║  ${COLORS.fg.white}Automation of activities on Tempo Testnet${COLORS.fg.cyan}                   ║${COLORS.reset}`);
    console.log(`${COLORS.fg.cyan}║  ${COLORS.fg.white}Ported from Python to Node.js${COLORS.fg.cyan}                               ║${COLORS.reset}`);
    console.log(`${COLORS.fg.cyan}║                                                               ║${COLORS.reset}`);
    console.log(`${COLORS.fg.cyan}╚═══════════════════════════════════════════════════════════════╝${COLORS.reset}`);
    console.log();
}


// Random Message
export function getRandomMessage() {
    const messages = [
        "Hello Tempo!",
        "Testing Tempo Network",
        "Deploying on Tempo",
        "Just checking in",
        "Node.js Bot Active",
        "Automation is key",
        "Enjoying the testnet",
        "Blockchain dev life",
        "Shadow & Antigravity",
        "Random message"
    ];
    return messages[Math.floor(Math.random() * messages.length)];
}

// ════════════════════════════════════════════════════════════════════════════════
// ROBUST TRANSACTION SENDER WITH RETRY
// ════════════════════════════════════════════════════════════════════════════════

/**
 * Sends a transaction with robust retry logic for nonce issues.
 * @param {ethers.Wallet} wallet - The wallet to send from.
 * @param {Function} txCreator - A function that returns a Promise resolving to the tx response (e.g. () => contract.method(...)).
 *                               Or, returns a populated transaction object to be sent via wallet.sendTransaction.
 * @param {number} maxRetries - Maximum number of retries.
 * @returns {Promise<ethers.TransactionReceipt>} - The transaction receipt.
 */
export async function sendTxWithRetry(wallet, txCreator, maxRetries = 3) {
    let attempt = 0;
    while (attempt < maxRetries) {
        try {
            // await sleep(500 * attempt); // Jitter
            const txResponse = await txCreator();
            // Wait for receipt
            const receipt = await txResponse.wait();
            return { receipt, hash: txResponse.hash };
        } catch (error) {
            attempt++;
            const msg = error.message.toLowerCase();

            // Check for rate limit (429) or "too many requests"
            if (msg.includes('429') || msg.includes('too many requests')) {
                // Aggressive Smart Backoff for 429s: 30s, 60s, 90s...
                // Server bans usually last at least 1 minute.
                if (attempt <= maxRetries + 2) { // Allow 2 extra retries for rate limits
                    const delay = 30000 * attempt;
                    // if (!silent) console.log(`${COLORS.dim}Rate limit (429). Retrying in ${delay/1000}s...${COLORS.reset}`);
                    await sleep(delay);
                    continue;
                }
            }

            // Log warning but don't clutter main log unless it's the last attempt
            if (attempt < maxRetries && (
                msg.includes('nonce') ||
                msg.includes('replacement transaction underpriced') ||
                msg.includes('already known') ||
                msg.includes('transaction execution reverted') // Sometimes reverting happens due to state sync, worth 1 retry? No, usually logic error.
            )) {
                // Check specific retryable errors
                if (msg.includes('nonce') || msg.includes('underpriced') || msg.includes('already known') || msg.includes('rpc server error') || msg.includes('-32000')) {
                    // Force nonce refresh by just letting the next attempt query it
                    // ethers.js usually auto-refreshes nonce on new tx creation if not explicitly set
                    // We add a small delay to let propagation happen
                    await sleep(2000 * attempt);
                    continue;
                }
            }

            throw error; // Rethrow if not retryable or max retries reached
        }
    }
}

/**
 * Runs a read-only operation with retry logic for 429/network errors.
 * @param {Function} taskFn - Async function to execute.
 * @param {number} maxRetries - Max retries.
 * @returns {Promise<any>} - Result of taskFn.
 */
export async function runWithRetry(taskFn, maxRetries = 3) {
    let attempt = 0;
    while (attempt < maxRetries) {
        try {
            return await taskFn();
        } catch (error) {
            attempt++;
            const msg = error.message.toLowerCase();

            // Retry on network/rate limit errors
            if (msg.includes('429') || msg.includes('too many requests') || msg.includes('server response') || msg.includes('502') || msg.includes('503')) {
                // Aggressive Backoff for read ops too
                if (attempt <= maxRetries + 2) {
                    const delay = 20000 * attempt; // 20s, 40s...
                    await sleep(delay);
                    continue;
                }
            }
            throw error;
        }
    }
}
