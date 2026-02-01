import { ethers } from 'ethers';
import fs from 'fs';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';
import { ScheduleService } from '../utils/schedule.js';
import process from 'process';
import { fileURLToPath } from 'url';
import { TempoSDKService } from '../utils/tempoService.js';

/* CONFIGURATION */
const AMOUNT_MIN = 1000;
const AMOUNT_MAX = 2000;
const DELAY_MIN = 10; // Seconds
const DELAY_MAX = 3500; // Seconds

const ERC20_ABI = [
    "function balanceOf(address owner) view returns (uint256)",
    "function decimals() view returns (uint8)",
    "function transfer(address to, uint256 amount) returns (bool)",
    "function symbol() view returns (string)"
];

import { Transaction } from 'viem/tempo';


export async function transferLaterForWallet(wallet, proxy, tokenAddress, amount, recipientAddress, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const shortAddr = `${wallet.address.substring(0, 6)}...${wallet.address.substring(38)}`;
    const shortDst = `${recipientAddress.substring(0, 6)}...${recipientAddress.substring(38)}`;

    let symbol = "TOKEN";
    let decimals = 18;

    try {
        const token = new ethers.Contract(tokenAddress, ERC20_ABI, wallet);
        try { symbol = await token.symbol(); } catch (e) { }
        try { decimals = Number(await token.decimals()); } catch (e) {
            if (!silent) console.log(`${COLORS.dim}Using default decimals: 18${COLORS.reset}`);
        }

        // Calculate Future Execution Time
        const delaySeconds = getRandomInt(DELAY_MIN, DELAY_MAX);
        const executeAt = new Date(Date.now() + delaySeconds * 1000);

        if (!silent) console.log(`${COLORS.fg.yellow}Scheduling ${amount} ${symbol} to ${shortDst} for ${executeAt.toLocaleString()}...${COLORS.reset}`);

        const amountWei = ethers.parseUnits(amount.toString(), decimals);

        // Check balance
        const balance = await token.balanceOf(wallet.address);
        const balanceFormatted = ethers.formatUnits(balance, decimals);
        if (!silent) console.log(`${COLORS.dim}Balance: ${balanceFormatted} ${symbol}${COLORS.reset}`);

        if (balance < amountWei) {
            if (!silent) console.log(`${COLORS.fg.red}✗ Insufficient balance: ${balanceFormatted} < ${amount}${COLORS.reset}`);
            logWalletAction(workerId, walletIndex, wallet.address, 'ScheduleTransfer', 'failed', `Insufficient ${symbol} balance`, silent, null, proxy);
            return { success: false, reason: 'insufficient_balance' };
        }

        // Detect Fee Token (System token for gas)
        const SYSTEM_TOKENS = ['PathUSD', 'AlphaUSD', 'BetaUSD', 'ThetaUSD'];
        let selectedFeeToken = 'PathUSD';
        for (const sym of SYSTEM_TOKENS) {
            try {
                const tokenAddr = CONFIG.TOKENS[sym];
                if (!tokenAddr) continue;
                const feeTokenContract = new ethers.Contract(tokenAddr, ["function balanceOf(address) view returns (uint256)"], wallet);
                const feeBal = await feeTokenContract.balanceOf(wallet.address);
                if (feeBal > 0n) {
                    selectedFeeToken = sym;
                    break;
                }
            } catch (e) { }
        }

        if (!silent) console.log(`${COLORS.dim}Gas Payment: Paying in ${selectedFeeToken}${COLORS.reset}`);

        // Schedule using the real Tempo SDK logic (Type 0x76)
        const tempoService = new TempoSDKService(wallet);

        const result = await tempoService.createScheduledTransfer(
            tokenAddress,
            amountWei,
            recipientAddress,
            executeAt,
            selectedFeeToken
        );

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'ScheduleTransfer', 'success', `${amount} ${symbol} -> ${shortDst} in ${delaySeconds}s`, silent, duration, proxy);

        if (!silent) {
            console.log(`${COLORS.fg.green}✓ Scheduled successfully!${COLORS.reset}`);
            console.log(`${COLORS.dim}Schedule ID: ${result.scheduleId}${COLORS.reset}`);
            console.log(`${COLORS.dim}Tx Hash: ${CONFIG.EXPLORER_URL}/tx/${result.transactionHash}${COLORS.reset}`);
        }

        return {
            success: true,
            txHash: result.transactionHash,
            scheduleId: result.scheduleId,
            executeAt,
            amount,
            tokenAddress,
            symbol
        };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'ScheduleTransfer', 'failed', error.message.substring(0, 50), silent, duration, proxy);
        if (!silent) console.log(`${COLORS.fg.red}✗ Schedule failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function transferLaterRandomTokenForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    // 1. Pick Random Token
    const tokenEntries = Object.entries(CONFIG.TOKENS);
    if (tokenEntries.length === 0) {
        if (!silent) console.log(`${COLORS.fg.red}No tokens configured in CONFIG.TOKENS${COLORS.reset}`);
        return { success: false, reason: 'no_tokens_configured' };
    }
    const [symbol, tokenAddress] = tokenEntries[Math.floor(Math.random() * tokenEntries.length)];

    // 2. Random Amount
    const amount = (Math.random() * (AMOUNT_MAX - AMOUNT_MIN) + AMOUNT_MIN).toFixed(2);

    // 3. Random Destination
    const toAddress = ethers.Wallet.createRandom().address;

    return await transferLaterForWallet(wallet, proxy, tokenAddress, amount, toAddress, workerId, walletIndex, silent);
}

export async function runSendTokenMenu() {
    console.log(`\n  ${COLORS.fg.magenta}📅  TOKEN SCHEDULE MODULE (AUTO)${COLORS.reset}\n`);

    const privateKeys = getPrivateKeys();
    console.log(`\n${COLORS.fg.cyan}Found ${privateKeys.length} wallet(s)${COLORS.reset}\n`);

    if (privateKeys.length === 0) {
        console.log(`${COLORS.fg.red}No private keys found.${COLORS.reset}`);
        return;
    }

    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);

        const proxyMsg = proxy ? `Using Proxy: ${proxy}` : "Using: Direct Connection";
        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}${COLORS.reset}`);
        console.log(`${COLORS.fg.cyan}Address: ${wallet.address}${COLORS.reset}`);
        console.log(`${COLORS.fg.cyan}${proxyMsg}${COLORS.reset}\n`);

        await transferLaterRandomTokenForWallet(wallet, proxy, 1, i);

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(5, 10), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ All schedules completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}

// Allow direct execution
if (process.argv[1] === fileURLToPath(import.meta.url)) {
    runSendTokenMenu();
}
