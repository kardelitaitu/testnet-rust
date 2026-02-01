import { ethers } from 'ethers';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { CONFIG, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const MULTICALL_ABI = [
    "function aggregate(tuple(address target, bytes callData)[] calls) payable returns (uint256 blockNumber, bytes[] returnData)"
];

const TIP20_ABI = [
    "function balanceOf(address account) view returns (uint256)",
    "function approve(address spender, uint256 amount) returns (bool)",
    "function allowance(address owner, address spender) view returns (uint256)",
    "function transferFrom(address from, address to, uint256 amount) returns (bool)",
    "function symbol() view returns (string)",
    "function decimals() view returns (uint8)"
];

function getRandomInt(min, max) { return Math.floor(Math.random() * (max - min + 1)) + min; }

export async function multiSendDisperseSystemForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    let actionName = '28_multiSendDisperse.js';

    try {
        // 1. Determine Target Count and Recipients
        const recipientNumberMin = 6;
        const recipientNumberMax = 15;
        const targetCount = getRandomInt(recipientNumberMin, recipientNumberMax);

        const txtPath = path.join(__dirname, '28_multiSendDisperse.txt');
        let allAddresses = [];
        if (fs.existsSync(txtPath)) {
            const content = fs.readFileSync(txtPath, 'utf-8');
            allAddresses = content.split(/\r?\n/).map(l => l.trim()).filter(l => l && ethers.isAddress(l));
        }

        if (allAddresses.length === 0) {
            if (!silent) console.log(`${COLORS.dim}No recipients found. Generating random ones.${COLORS.reset}`);
            for (let i = 0; i < 15; i++) allAddresses.push(ethers.Wallet.createRandom().address);
        }

        const recipients = allAddresses.sort(() => 0.5 - Math.random()).slice(0, targetCount);

        // 2. Select Funded Token (Parallel Check)
        const availableTokens = Object.entries(CONFIG.TOKENS);
        if (availableTokens.length === 0) throw new Error("No tokens configured");

        const balanceChecks = await Promise.all(availableTokens.map(async ([name, addr]) => {
            try {
                const contract = new ethers.Contract(addr, TIP20_ABI, wallet);
                const bal = await contract.balanceOf(wallet.address);
                return { name, addr, balance: bal };
            } catch (e) { return { name, addr, balance: 0n }; }
        }));

        const fundedTokens = balanceChecks.filter(t => t.balance > 0n);
        if (fundedTokens.length === 0) throw new Error("No tokens with balance found for this wallet.");

        // Pick one randomly from funded tokens
        const selected = fundedTokens[Math.floor(Math.random() * fundedTokens.length)];
        const [tokenName, tokenAddress] = [selected.name, selected.addr];

        const tokenContract = new ethers.Contract(tokenAddress, TIP20_ABI, wallet);
        let decimals = 18;
        try { decimals = await tokenContract.decimals(); } catch (e) { }

        if (!silent) console.log(`${COLORS.fg.yellow}🎲 Selected Token: ${tokenName} (Balance: ${ethers.formatUnits(selected.balance, decimals)})${COLORS.reset}`);

        // 3. Prepare Amounts
        const values = [];
        let totalAmount = 0n;
        const transferAmountMin = 100.0;
        const transferAmountMax = 500.0;

        for (const addr of recipients) {
            const randomAmount = (Math.random() * (transferAmountMax - transferAmountMin) + transferAmountMin).toFixed(2);
            const amt = ethers.parseUnits(randomAmount, decimals);
            values.push(amt);
            totalAmount += amt;
        }

        if (selected.balance < totalAmount) {
            if (!silent) console.log(`${COLORS.fg.yellow}⚠️ Balance too low for full batch. Reducing amounts...${COLORS.reset}`);
            // Logic to cap if needed, or error out. Let's simple cap for now:
            totalAmount = selected.balance;
            // Redetermine values (not perfect but keeps it moving)
            const share = totalAmount / BigInt(recipients.length);
            for (let i = 0; i < values.length; i++) values[i] = share;
        }

        // 4. Multicall Contract Check
        const CANONICAL_MULTICALL = "0xcA11bde05977b3631167028862bE2a173976CA11";
        const code = await wallet.provider.getCode(CANONICAL_MULTICALL);
        if (!code || code === '0x') throw new Error(`Multicall not found on this chain.`);

        // 5. Approval (Skip if sufficient)
        const allowance = await tokenContract.allowance(wallet.address, CANONICAL_MULTICALL);
        if (allowance < totalAmount) {
            if (!silent) console.log(`${COLORS.dim}Approving ${tokenName}...${COLORS.reset}`);
            // Wrap approval in retry logic
            await sendTxWithRetry(wallet, async () => {
                const gasOverrides = await getGasWithMultiplier(wallet.provider, 3.0, wallet);
                return tokenContract.approve(CANONICAL_MULTICALL, ethers.MaxUint256, { ...gasOverrides, gasLimit: 1000000 });
            });
            if (!silent) console.log(`${COLORS.dim}Approved MaxUint256.${COLORS.reset}`);
        }

        // 6. Execute Batch
        const multicallContract = new ethers.Contract(CANONICAL_MULTICALL, MULTICALL_ABI, wallet);
        const calls = recipients.map((to, i) => {
            return [tokenAddress, tokenContract.interface.encodeFunctionData('transferFrom', [wallet.address, to, values[i]])];
        });

        if (!silent) console.log(`${COLORS.fg.magenta}🚀 Batch Transferring to ${recipients.length} addrs...${COLORS.reset}`);

        // Wrap aggregate call in retry logic
        const { receipt, hash } = await sendTxWithRetry(wallet, async () => {
            const txGasOverrides = await getGasWithMultiplier(wallet.provider, 3.0, wallet);
            return multicallContract.aggregate(calls, { ...txGasOverrides, gasLimit: 3000000 });
        });

        if (!silent) console.log(`${COLORS.dim}Tx: ${hash}${COLORS.reset}`);

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, actionName, 'success', `Sent to ${recipients.length} addrs`, silent, duration);

        if (!silent) console.log(`${COLORS.fg.green}✓ Success! Block: ${receipt.blockNumber}${COLORS.reset}`);
        return { success: true, txHash: hash, block: receipt.blockNumber };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, actionName, 'failed', error.message.substring(0, 50), silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}
