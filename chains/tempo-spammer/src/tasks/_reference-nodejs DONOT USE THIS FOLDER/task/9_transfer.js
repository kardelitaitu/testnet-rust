import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';
import { claimRandomFaucetForWallet } from './2_claimFaucet.js';

const ERC20_ABI = [
    "function transfer(address to, uint256 amount) returns (bool)",
    "function balanceOf(address owner) view returns (uint256)",
    "function decimals() view returns (uint8)",
    "function symbol() view returns (string)"
];

import fs from 'fs';
import path from 'path';

export async function transferRandomTokenForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    let availableTokens = [];

    // 1. Add System Tokens
    const systemTokens = Object.keys(CONFIG.TOKENS).map(key => ({
        name: key,
        address: CONFIG.TOKENS[key],
        type: 'System'
    }));
    availableTokens = [...availableTokens, ...systemTokens];

    // 2. Add Stable Tokens (from data/created_tokens.json)
    try {
        const stablePath = path.join(process.cwd(), 'data', 'created_tokens.json');
        if (fs.existsSync(stablePath)) {
            const data = JSON.parse(fs.readFileSync(stablePath, 'utf-8'));
            const myTokens = data[wallet.address] || [];
            availableTokens = [...availableTokens, ...myTokens.map(t => ({
                name: t.symbol,
                address: t.token,
                type: 'Stable'
            }))];
        }
    } catch (e) { /* ignore */ }

    // 3. Add Meme Tokens (from data/created_memes.json)
    try {
        const memePath = path.join(process.cwd(), 'data', 'created_memes.json');
        if (fs.existsSync(memePath)) {
            const data = JSON.parse(fs.readFileSync(memePath, 'utf-8'));
            const myTokens = data[wallet.address] || [];
            availableTokens = [...availableTokens, ...myTokens.map(t => ({
                name: t.symbol,
                address: t.token,
                type: 'Meme'
            }))];
        }
    } catch (e) { /* ignore */ }

    if (availableTokens.length === 0) {
        if (!silent) console.log(`${COLORS.fg.red}✗ No tokens found for any category${COLORS.reset}`);
        return;
    }

    // Shuffle and check balance (optimization: check random subset to avoid too many RPC calls)
    const shuffled = availableTokens.sort(() => 0.5 - Math.random());

    // Try up to 5 tokens to find one with balance
    for (const token of shuffled.slice(0, 5)) {
        try {
            const contract = new ethers.Contract(token.address, ERC20_ABI, wallet);
            const balance = await contract.balanceOf(wallet.address);

            if (balance > 0n) {
                // Determine amount (ensure it's not more than balance)
                // Random 10-50% of balance, or fixed small amount? 
                // Original was 10-50 units. Let's stick to small random amount but capped at balance.

                let decimals = 18;
                try { decimals = await contract.decimals(); } catch (e) { }

                // Original logic: random 10-50.
                let amountVal = Math.random() * 40 + 10; // 10-50
                let amountWei = ethers.parseUnits(amountVal.toFixed(2), decimals);

                // If balance is less than this random amount, send 50% of balance
                if (balance < amountWei) {
                    amountWei = balance / 2n;
                    if (amountWei === 0n) continue; // Skip if too small
                } else {
                    // Use the calculated amountWei
                }

                const amountFormatted = ethers.formatUnits(amountWei, decimals);

                // Recipient
                const recipientAddress = ethers.Wallet.createRandom().address;

                if (!silent) console.log(`${COLORS.fg.magenta}[Worker ${workerId}] Selected ${token.type} Token: ${token.name} (${token.address.substring(0, 8)}...)${COLORS.reset}`);

                return await transferTokenForWallet(wallet, proxy, token.address, token.name, recipientAddress, parseFloat(amountFormatted), workerId, walletIndex, silent);
            }
        } catch (e) {
            // Ignore RPC errors during check, try next
        }
    }

    if (!silent) console.log(`${COLORS.fg.yellow}⚠ Checked 5 random tokens but found no positive balance.${COLORS.reset}`);
}

export async function transferTokenForWallet(wallet, proxy, tokenAddress, tokenSymbol, toAddress, amount, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now(); // Start timing

    if (!silent) console.log(`${COLORS.fg.yellow}Transferring ${amount} ${tokenSymbol} to ${toAddress.substring(0, 10)}...${COLORS.reset}`);

    try {
        const token = new ethers.Contract(tokenAddress, ERC20_ABI, wallet);

        // Get decimals
        let decimals = 6; // Default
        try {
            decimals = await token.decimals();
        } catch (e) {
            if (!silent) console.log(`${COLORS.dim}Using default decimals: 6${COLORS.reset}`);
        }

        const amountWei = ethers.parseUnits(amount.toString(), decimals);

        // Check balance
        const balance = await token.balanceOf(wallet.address);
        const balanceFormatted = ethers.formatUnits(balance, decimals);

        if (!silent) console.log(`${COLORS.dim}Balance: ${balanceFormatted} ${tokenSymbol}${COLORS.reset}`);

        if (balance < amountWei) {
            if (!silent) console.log(`${COLORS.fg.red}✗ Insufficient balance. Have: ${balanceFormatted}, Need: ${amount}${COLORS.reset}`);
            logWalletAction(workerId, walletIndex, wallet.address, 'Transfer', 'failed', 'Insufficient balance', silent, null, proxy);
            return { success: false, reason: 'insufficient_balance' };
        }

        // Transfer
        // Check Gas Token Balance (if strictly needed)
        // Usually PathUSD (or native) is needed for gas unless using Fee Abstraction
        // If we are sending a Meme/Stable, check if we have minimal PathUSD
        // Note: address check is heuristic.
        // Determine feeToken
        let feeToken;

        // 1. If sending a System Token, prefer using it for gas
        const isSystemToken = Object.values(CONFIG.TOKENS).some(addr => addr.toLowerCase() === tokenAddress.toLowerCase());
        if (isSystemToken) {
            feeToken = tokenAddress;
        } else {
            // 2. If sending Non-System Token (Meme/Stable), randomly choose ANY System Token for gas
            // User requested to skip scanning and just pick one randomly.
            const systemTokenKeys = Object.keys(CONFIG.TOKENS);
            const randomKey = systemTokenKeys[Math.floor(Math.random() * systemTokenKeys.length)];
            feeToken = CONFIG.TOKENS[randomKey];

            if (!silent) console.log(`${COLORS.dim}Using random gas token: ${randomKey}${COLORS.reset}`);
        }

        const txCreator = async () => {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);

            return token.transfer(toAddress, amountWei, {
                gasLimit: 250000,
                ...gasOverrides,
                feeCurrency: feeToken
            });
        };

        const { hash, receipt } = await sendTxWithRetry(wallet, txCreator);
        const tx = { hash }; // Backward compatibility for logging if needed

        if (receipt.status === 0) {
            throw new Error('Transaction reverted');
        }

        const duration = (Date.now() - startTime) / 1000; // Calculate duration in seconds
        logWalletAction(workerId, walletIndex, wallet.address, 'Transfer', 'success', `Transfer completed (Block: ${receipt.blockNumber})`, silent, duration, proxy);
        if (!silent) console.log(`${COLORS.fg.green}✓ Sent successfully! Block: ${receipt.blockNumber}${COLORS.reset}`);
        return { success: true, txHash: tx.hash, block: receipt.blockNumber };

    } catch (error) {
        if (error.message.includes("Insufficient balance for Gas")) {
            if (!silent) console.log(`${COLORS.fg.yellow}⚠ Insufficient balance for Gas. Attempting Faucet Refill...${COLORS.reset}`);
            try {
                await claimRandomFaucetForWallet(wallet, proxy, workerId, walletIndex, true);
            } catch (faucetError) { }
            return { success: false, reason: 'low_gas_refueling' };
        }

        // Handle generic RPC errors (-32000)
        if (error?.code === -32000 || error?.message?.includes("-32000")) {
            const duration = (Date.now() - startTime) / 1000;
            logWalletAction(workerId, walletIndex, wallet.address, 'Transfer', 'failed', 'RPC Server Error (-32000)', silent, duration, proxy);
            return { success: false, reason: 'rpc_server_error' };
        }

        const duration = (Date.now() - startTime) / 1000;
        // Clean up error message
        const shortError = error.reason || error.shortMessage || error.message.substring(0, 50);
        logWalletAction(workerId, walletIndex, wallet.address, 'Transfer', 'failed', shortError, silent, duration, proxy);
        if (!silent) console.log(`${COLORS.fg.red}✗ Transfer failed: ${shortError}${COLORS.reset}`);
        return { success: false, reason: shortError };
    }
}

export async function runTransferMenu() {
    console.log(`\n  ${COLORS.fg.magenta}💸  TRANSFER MODULE${COLORS.reset}\n`);

    const mode = await askQuestion(`${COLORS.fg.cyan}1. Random recipient\\n2. Specific address\\nChoose (1-2): ${COLORS.reset}`);

    let recipientAddress;
    if (mode === '2') {
        recipientAddress = await askQuestion(`${COLORS.fg.cyan}Recipient address: ${COLORS.reset}`);
        if (!ethers.isAddress(recipientAddress)) {
            console.log(`${COLORS.fg.red}✗ Invalid address${COLORS.reset}`);
            return;
        }
    }

    // Token selection
    const tokenKeys = Object.keys(CONFIG.TOKENS);
    console.log(`\n${COLORS.fg.cyan}Available tokens:${COLORS.reset}`);
    tokenKeys.forEach((key, index) => {
        console.log(`  ${index + 1}. ${key}`);
    });

    const tokenChoice = await askQuestion(`${COLORS.fg.cyan}Select token (1-${tokenKeys.length}): ${COLORS.reset}`);
    const tokenIndex = parseInt(tokenChoice) - 1;

    if (tokenIndex < 0 || tokenIndex >= tokenKeys.length) {
        console.log(`${COLORS.fg.red}✗ Invalid selection${COLORS.reset}`);
        return;
    }

    const tokenSymbol = tokenKeys[tokenIndex];
    const tokenAddress = CONFIG.TOKENS[tokenSymbol];

    const amountInput = await askQuestion(`${COLORS.fg.cyan}Amount to send: ${COLORS.reset}`);
    const amount = parseFloat(amountInput);

    if (isNaN(amount) || amount <= 0) {
        console.log(`${COLORS.fg.red}✗ Invalid amount${COLORS.reset}`);
        return;
    }

    const privateKeys = getPrivateKeys();
    console.log(`\n${COLORS.fg.cyan}Found ${privateKeys.length} wallet(s)${COLORS.reset}\n`);

    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);

        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}${COLORS.reset}`);
        console.log(`${COLORS.fg.cyan}Address: ${wallet.address}${COLORS.reset}`);

        // Determine recipient for this iteration
        const finalRecipient = mode === '2'
            ? recipientAddress
            : (Math.random() < 0.5 && privateKeys.length > 1
                ? (await getWallet(Math.floor(Math.random() * privateKeys.length), privateKeys[Math.floor(Math.random() * privateKeys.length)])).wallet.address
                : ethers.Wallet.createRandom().address);

        await transferTokenForWallet(wallet, proxy, tokenAddress, tokenSymbol, finalRecipient, amount, 1, i);

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(5, 10), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ All transfers completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}
