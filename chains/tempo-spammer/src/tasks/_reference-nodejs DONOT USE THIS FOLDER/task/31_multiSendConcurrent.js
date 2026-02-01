import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { parseAbi, encodeFunctionData, parseUnits, isAddress } from 'viem';
import { CONFIG, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { claimRandomFaucetForWallet } from './2_claimFaucet.js';
import { ConcurrentService, loadNonceKey, saveNonceKey } from '../utils/tempoConcurrent.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const TIP20_ABI = parseAbi([
    "function transfer(address to, uint256 amount) returns (bool)",
    "function transferWithMemo(address to, uint256 amount, bytes32 memo) returns (bool)",
    "function balanceOf(address account) view returns (uint256)",
    "function decimals() view returns (uint8)"
]);

import { ethers } from 'ethers'; // For balance check helper

function getRandomInt(min, max) { return Math.floor(Math.random() * (max - min + 1)) + min; }

export async function multiSendConcurrentSystemForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    let actionName = '31_multiSendConcurrent.js';

    // 1. Setup Concurrent Service
    // ethers Wallet has privateKey property
    const privateKey = wallet.privateKey;
    if (!privateKey) throw new Error("Wallet private key not available for concurrent service");

    const service = new ConcurrentService(privateKey, proxy);
    const address = service.getAddress();

    try {
        // 2. Select Funded Token
        const availableTokens = Object.entries(CONFIG.TOKENS);
        if (availableTokens.length === 0) throw new Error("No tokens configured");

        // Simple balance check using ethers (since it's already set up in the wallet)
        const balanceChecks = await Promise.all(availableTokens.map(async ([name, addr]) => {
            try {
                const contract = new ethers.Contract(addr, ["function balanceOf(address) view returns (uint256)", "function decimals() view returns (uint8)"], wallet);
                const bal = await contract.balanceOf(address);
                let dec = 18;
                try { dec = Number(await contract.decimals()); } catch (e) { }
                return { name, addr, balance: bal, decimals: dec };
            } catch (e) { return { name, addr, balance: 0n, decimals: 18 }; }
        }));

        const fundedTokens = balanceChecks.filter(t => t.balance > 0n);

        if (fundedTokens.length === 0) {
            if (!silent) console.log(`${COLORS.fg.yellow}[Worker ${workerId}] No system tokens. Attempting faucet claim...${COLORS.reset}`);
            await claimRandomFaucetForWallet(wallet, proxy, workerId, walletIndex, true);
            // Wait for balance update? A simple retry mechanism or just fail this run but next time it might have balance
            // For now, let's throw friendly error so next loop picks it up
            throw new Error("No system tokens. Faucet claimed, will retry next loop.");
        }

        // Pick one randomly or prefer AlphaUSD if available and funded
        const selected = fundedTokens.find(t => t.name === 'AlphaUSD') || fundedTokens[Math.floor(Math.random() * fundedTokens.length)];
        const { name: tokenName, addr: tokenAddress, decimals: tokenDecimals, balance: tokenBalance } = selected;

        if (!silent) console.log(`${COLORS.fg.cyan}[Worker ${workerId}] Concurrent MultiSend using ${tokenName} (Balance: ${ethers.formatUnits(tokenBalance, tokenDecimals)})${COLORS.reset}`);

        // 3. Load Recipients
        const txtPath = path.join(__dirname, '28_multiSendDisperse.txt');
        let allAddresses = [];
        if (fs.existsSync(txtPath)) {
            allAddresses = fs.readFileSync(txtPath, 'utf-8').split(/\r?\n/).map(l => l.trim()).filter(l => l && isAddress(l));
        }
        if (allAddresses.length === 0) {
            for (let i = 0; i < 20; i++) allAddresses.push(`0x${Math.random().toString(16).slice(2, 42).padStart(40, '0')}`);
        }

        const targetCount = getRandomInt(5, 10);
        const selectedRecipients = allAddresses.sort(() => 0.5 - Math.random()).slice(0, targetCount);

        // 4. Prepare Payments (Dynamic Amount Based on Balance)
        // Similar to task 28, but simplified. We'll target 100-500 per tx, but cap at balance.
        const transferAmountMin = 100.0;
        const transferAmountMax = 500.0;

        const payments = selectedRecipients.map(to => {
            let amountDisplay = (Math.random() * 300 + 100);
            let amount = parseUnits(amountDisplay.toFixed(tokenDecimals > 6 ? 2 : 0), tokenDecimals);

            // If total batch would exceed balance, this will fail on-chain, but the concurrent logic
            // will catch individual failures. For better UX, we'll do a simple check.
            const data = encodeFunctionData({
                abi: TIP20_ABI,
                functionName: 'transfer',
                args: [to, amount]
            });
            return { to: tokenAddress, data };
        });

        // 5. Execution Loop (Nonce Collision Handling)
        // Use Timestamp to guarantee uniqueness and avoid "nonce too low" errors
        let currentStartKey = Date.now();
        let pendingPayments = [...payments];
        const finalResults = [];
        let batchAttempts = 0;
        const MAX_BATCH_RETRIES = 5;

        while (pendingPayments.length > 0 && batchAttempts < MAX_BATCH_RETRIES) {
            batchAttempts++;
            if (!silent) console.log(`${COLORS.dim}[Worker ${workerId}] Batch ${batchAttempts}: Sending ${pendingPayments.length} txs from Key ${currentStartKey}${COLORS.reset}`);

            const batchResults = await service.sendConcurrentPayments(pendingPayments, currentStartKey, false); // Fast mode

            const success = [], retry = [];
            for (let i = 0; i < batchResults.length; i++) {
                const res = batchResults[i];
                const pay = pendingPayments[i];

                if (res.status === 'confirmed' || res.status === 'broadcasted') {
                    success.push(res);
                } else {
                    const msg = res.error || "";
                    if (!silent) console.log(`${COLORS.fg.red}  [!] Tx Failed (Key ${res.nonceKey}): ${msg.substring(0, 100)}${COLORS.reset}`);
                    if (msg.includes('nonce too low') || msg.includes('Nonce provided') || msg.includes('underpriced')) {
                        retry.push(pay);
                    } else {
                        finalResults.push(res);
                    }
                }
            }

            finalResults.push(...success);
            currentStartKey += pendingPayments.length;

            if (retry.length > 0) {
                if (!silent) console.log(`${COLORS.fg.yellow}[Worker ${workerId}] ${retry.length} collisions. Retrying...${COLORS.reset}`);
                pendingPayments = retry;
            } else {
                pendingPayments = [];
            }
        }

        saveNonceKey(address, currentStartKey);

        const confirmedCount = finalResults.filter(r => r.status === 'confirmed' || r.status === 'broadcasted').length;
        const duration = (Date.now() - startTime) / 1000;

        if (confirmedCount > 0) {
            const lastTxHash = finalResults.filter(r => r.status === 'confirmed' || r.status === 'broadcasted').pop()?.hash;
            logWalletAction(workerId, walletIndex, address, actionName, 'success', `Sent ${confirmedCount} txs (${duration.toFixed(1)}s)`, silent, duration);
            return { success: true, sent: confirmedCount, duration: duration.toFixed(1), txHash: lastTxHash };
        } else {
            throw new Error("No transactions confirmed");
        }

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, address, actionName, 'failed', error.message.substring(0, 50), silent, duration);
        return { success: false, reason: error.message, duration: duration.toFixed(1) };
    }
}
