import { ethers } from 'ethers';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';
import { mintRandomMemeForWallet } from './22_mintMeme.js';

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

export async function multiSendDisperseMemeForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    let actionName = '30_multiSendDisperseMeme.js';

    try {
        // 1. Load Meme Tokens for this wallet
        const dataPath = path.join(process.cwd(), 'data', 'created_memes.json');
        let availableTokens = [];

        if (fs.existsSync(dataPath)) {
            const data = JSON.parse(fs.readFileSync(dataPath, 'utf-8'));
            const myTokens = data[wallet.address] || [];
            availableTokens = myTokens.map(t => [t.symbol, t.token]);
        }

        if (availableTokens.length === 0) {
            if (!silent) console.log(`${COLORS.fg.yellow}⚠ No memes found. Auto-creating/minting first...${COLORS.reset}`);
            // Fallback: Mint a new meme token
            await mintRandomMemeForWallet(wallet, proxy, workerId, walletIndex, true);
            return { success: false, reason: 'no_memes_minting_now' }; // Will succeed next loop
        }

        // 2. Determine Target Count and Recipients
        const recipientNumberMin = 5;
        const recipientNumberMax = 10;
        const targetCount = getRandomInt(recipientNumberMin, recipientNumberMax);

        const txtPath = path.join(__dirname, '28_multiSendDisperse.txt');
        let allAddresses = [];
        if (fs.existsSync(txtPath)) {
            const content = fs.readFileSync(txtPath, 'utf-8');
            allAddresses = content.split(/\r?\n/).map(l => l.trim()).filter(l => l && ethers.isAddress(l));
        }

        if (allAddresses.length === 0) {
            for (let i = 0; i < 15; i++) allAddresses.push(ethers.Wallet.createRandom().address);
        }

        const recipients = allAddresses.sort(() => 0.5 - Math.random()).slice(0, targetCount);

        // 3. Select Funded Token (Parallel Check)
        const balanceChecks = await Promise.all(availableTokens.slice(0, 10).map(async ([name, addr]) => {
            try {
                const contract = new ethers.Contract(addr, TIP20_ABI, wallet);
                const bal = await contract.balanceOf(wallet.address);
                return { name, addr, balance: bal };
            } catch (e) { return { name, addr, balance: 0n }; }
        }));

        const fundedTokens = balanceChecks.filter(t => t.balance > 0n);

        if (fundedTokens.length === 0) {
            if (!silent) console.log(`${COLORS.fg.yellow}⚠ No funded memes. Auto-minting...${COLORS.reset}`);
            // Fallback: Mint using existing token if possible, or create new
            await mintRandomMemeForWallet(wallet, proxy, workerId, walletIndex, true);
            throw new Error("No funded meme tokens. Minting initiated, retry next loop.");
        }

        const selected = fundedTokens[Math.floor(Math.random() * fundedTokens.length)];
        const [tokenName, tokenAddress] = [selected.name, selected.addr];

        const tokenContract = new ethers.Contract(tokenAddress, TIP20_ABI, wallet);
        let decimals = 18;
        try { decimals = await tokenContract.decimals(); } catch (e) { }

        if (!silent) console.log(`${COLORS.fg.yellow}🎲 Selected Meme: ${tokenName} (Balance: ${ethers.formatUnits(selected.balance, decimals)})${COLORS.reset}`);

        // 4. Prepare Amounts
        const values = [];
        let totalAmount = 0n;
        for (const addr of recipients) {
            const randomAmount = (Math.random() * 300 + 100).toFixed(2);
            const amt = ethers.parseUnits(randomAmount, decimals);
            values.push(amt);
            totalAmount += amt;
        }

        if (selected.balance < totalAmount) {
            totalAmount = selected.balance;
            const share = totalAmount / BigInt(recipients.length);
            for (let i = 0; i < values.length; i++) values[i] = share;
        }

        // 5. Multicall Check & Approval
        const CANONICAL_MULTICALL = "0xcA11bde05977b3631167028862bE2a173976CA11";
        const allowance = await tokenContract.allowance(wallet.address, CANONICAL_MULTICALL);
        if (allowance < totalAmount) {
            // Wrap approval in retry logic
            await sendTxWithRetry(wallet, async () => {
                const gasOverrides = await getGasWithMultiplier(wallet.provider, 3.0, wallet);
                return tokenContract.approve(CANONICAL_MULTICALL, ethers.MaxUint256, { ...gasOverrides, gasLimit: 1000000 });
            });
        }

        // 6. Execute Batch
        const multicallContract = new ethers.Contract(CANONICAL_MULTICALL, MULTICALL_ABI, wallet);
        const calls = recipients.map((to, i) => {
            return [tokenAddress, tokenContract.interface.encodeFunctionData('transferFrom', [wallet.address, to, values[i]])];
        });

        if (!silent) console.log(`${COLORS.fg.magenta}🚀 Batch Transferring Meme to ${recipients.length} addrs...${COLORS.reset}`);
        // Wrap aggregate call in retry logic
        const { receipt, hash } = await sendTxWithRetry(wallet, async () => {
            const txGasOverrides = await getGasWithMultiplier(wallet.provider, 3.0, wallet);
            return multicallContract.aggregate(calls, { ...txGasOverrides, gasLimit: 3000000 });
        });


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
