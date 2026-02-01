import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { parseAbi, encodeFunctionData, parseUnits, isAddress } from 'viem';
import { CONFIG, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { ConcurrentService, loadNonceKey, saveNonceKey } from '../utils/tempoConcurrent.js';
import { ethers } from 'ethers';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const TIP20_ABI = parseAbi([
    "function transfer(address to, uint256 amount) returns (bool)",
    "function transferWithMemo(address to, uint256 amount, bytes32 memo) returns (bool)",
    "function balanceOf(address account) view returns (uint256)",
    "function decimals() view returns (uint8)"
]);

function getRandomInt(min, max) { return Math.floor(Math.random() * (max - min + 1)) + min; }

export async function multiSendConcurrentStableForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    let actionName = '32_multiSendConcurrentStable.js';

    const privateKey = wallet.privateKey;
    if (!privateKey) throw new Error("Wallet private key not available");

    const service = new ConcurrentService(privateKey, proxy);
    const address = service.getAddress();

    try {
        // 1. Load Created Tokens
        const dataPath = path.resolve(process.cwd(), 'data', 'created_tokens.json');
        if (!fs.existsSync(dataPath)) throw new Error("created_tokens.json not found");

        const data = JSON.parse(fs.readFileSync(dataPath, 'utf-8'));
        const myTokens = data[address] || [];
        if (myTokens.length === 0) throw new Error("No stable tokens found for this wallet");

        // Pick a funded token
        const balanceChecks = await Promise.all(myTokens.map(async (t) => {
            try {
                const contract = new ethers.Contract(t.token, TIP20_ABI, wallet);
                const bal = await contract.balanceOf(address);
                let dec = 6;
                try { dec = Number(await contract.decimals()); } catch (e) { }
                return { ...t, balance: bal, decimals: dec };
            } catch (e) { return { ...t, balance: 0n, decimals: 6 }; }
        }));

        const fundedTokens = balanceChecks.filter(t => t.balance > 0n);
        if (fundedTokens.length === 0) throw new Error("No funded stable tokens found.");

        const t = fundedTokens[Math.floor(Math.random() * fundedTokens.length)];
        if (!silent) console.log(`${COLORS.fg.cyan}[Worker ${workerId}] Concurrent MultiSend using ${t.symbol} (Balance: ${ethers.formatUnits(t.balance, t.decimals)})${COLORS.reset}`);

        // 2. Load Recipients
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

        // 3. Prepare Payments
        const payments = selectedRecipients.map(to => {
            const amountDisplay = (Math.random() * 300 + 100);
            const amount = parseUnits(amountDisplay.toFixed(t.decimals > 6 ? 2 : 0), t.decimals);
            const data = encodeFunctionData({
                abi: TIP20_ABI,
                functionName: 'transfer',
                args: [to, amount]
            });
            return { to: t.token, data };
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
            const batchResults = await service.sendConcurrentPayments(pendingPayments, currentStartKey, false); // Fast mode

            const success = [], retry = [];
            for (let i = 0; i < batchResults.length; i++) {
                const res = batchResults[i];
                if (res.status === 'confirmed' || res.status === 'broadcasted') {
                    success.push(res);
                } else {
                    const msg = res.error || "";
                    if (!silent) console.log(`${COLORS.fg.red}  [!] Tx Failed (Key ${res.nonceKey}): ${msg.substring(0, 100)}${COLORS.reset}`);
                    if (msg.includes('nonce too low') || msg.includes('Nonce provided') || msg.includes('underpriced')) {
                        retry.push(pendingPayments[i]);
                    } else {
                        finalResults.push(res);
                    }
                }
            }

            finalResults.push(...success);
            currentStartKey += pendingPayments.length;
            pendingPayments = retry;
        }

        saveNonceKey(address, currentStartKey);

        const confirmedCount = finalResults.filter(r => r.status === 'confirmed' || r.status === 'broadcasted').length;
        const duration = (Date.now() - startTime) / 1000;

        if (confirmedCount > 0) {
            const lastTxHash = finalResults.filter(r => r.status === 'confirmed' || r.status === 'broadcasted').pop()?.hash;
            logWalletAction(workerId, walletIndex, address, actionName, 'success', `Sent ${confirmedCount} txs of ${t.symbol} (${duration.toFixed(1)}s)`, silent, duration);
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
