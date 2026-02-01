import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet, loadCreatedTokens, loadCreatedMemes } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { countdown } from '../utils/helpers.js';
import fs from 'fs';
import path from 'path';

const ERC20_ABI = [
    "function balanceOf(address owner) view returns (uint256)",
    "function symbol() view returns (string)",
    "function decimals() view returns (uint8)"
];

async function getWalletAnalytics(wallet, silent = false) {
    const analytics = {
        address: wallet.address,
        nativeBalance: '0',
        tokenBalances: [],
        createdTokens: 0,
        createdMemes: 0,
        totalTransactions: 0,
        successRate: 0,
        lastActivity: null
    };

    try {
        // 1. Get native TEMPO balance
        const balance = await wallet.provider.getBalance(wallet.address);
        analytics.nativeBalance = ethers.formatEther(balance);

        // 2. Check all default stablecoin balances
        const stablecoins = [
            { name: 'PathUSD', address: CONFIG.TOKENS.PathUSD },
            { name: 'AlphaUSD', address: CONFIG.TOKENS.AlphaUSD },
            { name: 'BetaUSD', address: CONFIG.TOKENS.BetaUSD },
            { name: 'ThetaUSD', address: CONFIG.TOKENS.ThetaUSD }
        ];

        let totalStableBalance = 0;

        for (const stable of stablecoins) {
            try {
                const stableContract = new ethers.Contract(stable.address, ERC20_ABI, wallet);
                const balance = await stableContract.balanceOf(wallet.address);
                const decimals = await stableContract.decimals();
                const formattedBalance = ethers.formatUnits(balance, decimals);

                if (balance > 0) {
                    analytics.tokenBalances.push({
                        symbol: stable.name,
                        balance: formattedBalance,
                        address: stable.address
                    });
                    totalStableBalance += parseFloat(formattedBalance);
                }
            } catch (e) { /* Skip if fails */ }
        }

        analytics.totalStableUSD = totalStableBalance.toFixed(2);

        // 3. Load created tokens and memes
        const createdTokens = loadCreatedTokens();
        const createdMemes = loadCreatedMemes();

        const walletAddress = ethers.getAddress(wallet.address);
        const myTokens = createdTokens[walletAddress] || createdTokens[walletAddress.toLowerCase()] || [];
        const myMemes = createdMemes[walletAddress] || createdMemes[walletAddress.toLowerCase()] || [];

        analytics.createdTokens = myTokens.length;
        analytics.createdMemes = myMemes.length;

        // 4. Check balances of created tokens (first 5 only to avoid too many requests)
        const tokensToCheck = [...myTokens.slice(0, 3), ...myMemes.slice(0, 2)];

        for (const tokenInfo of tokensToCheck) {
            try {
                const tokenContract = new ethers.Contract(tokenInfo.token, ERC20_ABI, wallet);
                const tokenBalance = await tokenContract.balanceOf(wallet.address);

                if (tokenBalance > 0) {
                    const decimals = await tokenContract.decimals();
                    analytics.tokenBalances.push({
                        symbol: tokenInfo.symbol,
                        balance: ethers.formatUnits(tokenBalance, decimals),
                        address: tokenInfo.token
                    });
                }
            } catch (e) { /* Skip if fails */ }
        }

        // 5. Read transaction stats from logs
        try {
            const logFile = path.join(process.cwd(), 'logs', 'wallet_actions.log');
            if (fs.existsSync(logFile)) {
                const logContent = fs.readFileSync(logFile, 'utf8');
                const lines = logContent.split('\\n');

                let walletTransactions = 0;
                let successCount = 0;
                let lastTimestamp = null;

                for (const line of lines) {
                    if (line.includes(wallet.address.toLowerCase()) || line.includes(wallet.address)) {
                        walletTransactions++;
                        if (line.includes('success')) successCount++;

                        // Try to extract timestamp
                        const timestampMatch = line.match(/\[(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\]/);
                        if (timestampMatch) {
                            lastTimestamp = timestampMatch[1];
                        }
                    }
                }

                analytics.totalTransactions = walletTransactions;
                analytics.successRate = walletTransactions > 0
                    ? ((successCount / walletTransactions) * 100).toFixed(1)
                    : 0;
                analytics.lastActivity = lastTimestamp;
            }
        } catch (e) { /* Skip if can't read logs */ }

    } catch (error) {
        if (!silent) console.error(`Error getting analytics: ${error.message}`);
    }

    return analytics;
}

function displayAnalytics(analytics, silent = false) {
    if (silent) return;

    console.log(`\\n${COLORS.fg.cyan}═══════════════════════════════════════════${COLORS.reset}`);
    console.log(`${COLORS.fg.magenta}📊  Wallet Analytics${COLORS.reset}`);
    console.log(`${COLORS.fg.cyan}═══════════════════════════════════════════${COLORS.reset}`);

    console.log(`\\n${COLORS.fg.yellow}Address:${COLORS.reset} ${analytics.address}`);

    // Native balance
    console.log(`\\n${COLORS.fg.cyan}💰 Native Balance:${COLORS.reset}`);
    console.log(`   ${analytics.nativeBalance} TEMPO`);

    // Token balances
    if (analytics.tokenBalances.length > 0) {
        console.log(`\n${COLORS.fg.cyan}🪙  Token Balances:${COLORS.reset}`);
        for (const token of analytics.tokenBalances) {
            console.log(`   ${token.balance} ${token.symbol} ${COLORS.dim}(${token.address.substring(0, 10)}...)${COLORS.reset}`);
        }

        // Show total stable balance
        if (analytics.totalStableUSD && parseFloat(analytics.totalStableUSD) > 0) {
            console.log(`\n   ${COLORS.fg.green}💵 Total Stable Balance: $${analytics.totalStableUSD} USD${COLORS.reset}`);
        }
    }

    // Created assets
    console.log(`\\n${COLORS.fg.cyan}🎨 Created Assets:${COLORS.reset}`);
    console.log(`   Stablecoins: ${analytics.createdTokens}`);
    console.log(`   Meme Tokens: ${analytics.createdMemes}`);

    // Transaction stats
    if (analytics.totalTransactions > 0) {
        console.log(`\\n${COLORS.fg.cyan}📈 Transaction Stats:${COLORS.reset}`);
        console.log(`   Total: ${analytics.totalTransactions}`);
        console.log(`   Success Rate: ${analytics.successRate}%`);
        if (analytics.lastActivity) {
            console.log(`   Last Activity: ${analytics.lastActivity}`);
        }
    } else {
        console.log(`\n${COLORS.dim}No transaction history found${COLORS.reset}`);
    }

    console.log(`\n${COLORS.fg.cyan}═══════════════════════════════════════════${COLORS.reset}\n`);
}

export async function walletAnalyticsForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const analytics = await getWalletAnalytics(wallet, silent);

    if (!silent) {
        displayAnalytics(analytics);
    }

    // Log to automatic runner with concise format
    const logMessage = `Stable: [${analytics.createdTokens}] Meme: [${analytics.createdMemes}] Balance: ${analytics.totalStableUSD} USD`;
    const duration = (Date.now() - startTime) / 1000;
    logWalletAction(workerId, walletIndex, wallet.address, 'Analytics', 'success', logMessage, silent, duration);

    // Return useful data even in silent mode
    return {
        success: true,
        nativeBalance: analytics.nativeBalance,
        tokenCount: analytics.tokenBalances.length,
        createdAssets: analytics.createdTokens + analytics.createdMemes,
        stableCount: analytics.createdTokens,
        memeCount: analytics.createdMemes,
        transactions: analytics.totalTransactions,
        totalStableUSD: analytics.totalStableUSD
    };
}

export async function runWalletAnalyticsMenu() {
    console.log(`\n  ${COLORS.fg.magenta}📊  WALLET ANALYTICS MODULE${COLORS.reset}\n`);
    console.log(`${COLORS.fg.yellow}View detailed wallet statistics and analytics${COLORS.reset}\n`);

    const privateKeys = getPrivateKeys();
    if (privateKeys.length === 0) {
        console.log(`${COLORS.fg.red}No private keys found${COLORS.reset}`);
        return;
    }

    console.log(`${COLORS.fg.cyan}Found ${privateKeys.length} wallet(s)${COLORS.reset}\\n`);
    console.log(`${COLORS.dim}Press Ctrl+C to stop at any time${COLORS.reset}\\n`);

    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);

        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}${COLORS.reset}`);

        await walletAnalyticsForWallet(wallet, proxy, 1, i, false);

        if (i < privateKeys.length - 1) {
            await countdown(3, 'Next wallet in');
        }
    }

    console.log(`\\n${COLORS.fg.green}✓ Analytics completed for all wallets.${COLORS.reset}\\n`);
    await countdown(5, 'Returning to menu');
}
