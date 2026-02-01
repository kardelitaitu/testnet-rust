import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';
import { getRandomText } from '../utils/randomText.js';

const ERC20_ABI = [
    "function transferWithMemo(address to, uint256 amount, bytes32 memo) returns (bool)",
    "function balanceOf(address owner) view returns (uint256)",
    "function decimals() view returns (uint8)",
    "function symbol() view returns (string)"
];

export async function transferMemoRandomForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    // Select random token from CONFIG.TOKENS (PathUSD, AlphaUSD, BetaUSD, ThetaUSD)
    const tokenKeys = Object.keys(CONFIG.TOKENS);
    if (tokenKeys.length === 0) {
        console.log(`${COLORS.fg.red}✗ No tokens configured${COLORS.reset}`);
        return { success: false, reason: 'no_tokens_configured' };
    }

    // New Logic: Check balances first to avoid "insufficient_balance" on empty tokens
    const validTokens = [];

    // We want to send between 10-50, so let's look for tokens with at least 50
    // We'll check all of them in parallel for speed
    if (!silent) console.log(`${COLORS.dim}Checking balances for ${tokenKeys.length} tokens...${COLORS.reset}`);

    const balanceChecks = tokenKeys.map(async (key) => {
        try {
            const tokenAddr = CONFIG.TOKENS[key];
            const tokenContract = new ethers.Contract(tokenAddr, ERC20_ABI, wallet);
            const balance = await tokenContract.balanceOf(wallet.address);
            // Assuming 18 decimals for simplicity/speed in check, or just check > 0 initially
            // Better to check properly. Most are 18, but let's just check if > 50 * 10^6 (USDC-like) or 10^18
            // To be safe and fast, let's just use a low threshold of 50 units essentially
            // We'll assume standard 18 decimals for these testnet tokens usually, or 6.
            // Let's use logic: if balance > 50 * 10^18 OR balance > 50 * 10^6
            if (balance > 50000000n) { // Covers both 6 and 18 decimals for amount 50
                return { key, balance };
            }
        } catch (e) { }
        return null;
    });

    const results = await Promise.all(balanceChecks);
    const available = results.filter(r => r !== null);

    if (available.length === 0) {
        const duration = 0; // estimation
        logWalletAction(workerId, walletIndex, wallet.address, 'TransferMemo', 'failed', 'No tokens with sufficient balance', silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ No tokens with sufficient balance (>50)${COLORS.reset}`);
        return { success: false, reason: 'no_tokens_with_balance' };
    }

    // Pick random from AVAILABLE tokens
    const choice = available[Math.floor(Math.random() * available.length)];
    const randomTokenKey = choice.key;

    const tokenAddress = CONFIG.TOKENS[randomTokenKey];
    const tokenSymbol = randomTokenKey;

    if (!silent) console.log(`${COLORS.fg.cyan}Selected ${tokenSymbol} (Balance > 50)${COLORS.reset}`);

    // Random amount between 10-50 with 2 decimal places (e.g., 23.45)
    // Ensure we don't send MORE than we have if it's tight, but we filtered for >50
    const amount = (Math.random() * 40 + 10).toFixed(2); // 10.00 - 50.00

    // Always send to random address
    const recipientAddress = ethers.Wallet.createRandom().address;

    // Generate memo: 2-3 random single words + 3-5 digit random number
    // e.g., "happy birthday 742" or "ocean sunrise mountain 45678"
    const wordCount = getRandomInt(2, 3); // 2 or 3 words
    const words = [];
    for (let i = 0; i < wordCount; i++) {
        words.push(getRandomText().split(' ')[0]);
    }

    // 3-5 digit number: 100-99999
    const digitCount = getRandomInt(3, 5);
    const minNum = Math.pow(10, digitCount - 1); // 100, 1000, or 10000
    const maxNum = Math.pow(10, digitCount) - 1; // 999, 9999, or 99999
    const randomNumber = getRandomInt(minNum, maxNum);

    const memo = `${words.join(' ')} ${randomNumber}`;

    return await transferMemoForWallet(wallet, proxy, tokenAddress, tokenSymbol, recipientAddress, parseFloat(amount), memo, workerId, walletIndex, silent);
}

export async function transferMemoForWallet(wallet, proxy, tokenAddress, tokenSymbol, toAddress, amount, memo, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    if (!silent) {
        console.log(`${COLORS.fg.yellow}Transferring ${amount} ${tokenSymbol} to ${toAddress.substring(0, 10)}...${COLORS.reset}`);
        console.log(`${COLORS.dim}Memo: "${memo}"${COLORS.reset}`);
    }

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
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) console.log(`${COLORS.fg.red}✗ Insufficient balance. Have: ${balanceFormatted}, Need: ${amount}${COLORS.reset}`);
            logWalletAction(workerId, walletIndex, wallet.address, 'TransferMemo', 'failed', 'Insufficient balance', silent, duration);
            return { success: false, reason: 'insufficient_balance' };
        }

        // Encode memo as bytes32 (max 32 bytes)
        // UTF-8 encode, take first 32 bytes, pad with zeros if needed
        const memoBytes = ethers.toUtf8Bytes(memo.substring(0, 32));
        const memoBytes32 = ethers.zeroPadBytes(memoBytes, 32);

        // Transfer with memo
        // Transfer with memo
        // Use 3x gas multiplier for speed (using helper)
        // Transfer with memo
        // Use 3x gas multiplier for speed (using helper)
        // Transfer with memo
        // Use 3x gas multiplier for speed (using helper)
        const txCreator = async () => {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            return token.transferWithMemo(toAddress, amountWei, memoBytes32, {
                gasLimit: 150000,
                ...gasOverrides
            });
        };

        const { hash, receipt } = await sendTxWithRetry(wallet, txCreator);
        const tx = { hash };
        if (!silent) console.log(`${COLORS.dim}Tx: ${CONFIG.EXPLORER_URL}/tx/${hash}${COLORS.reset}`);

        if (receipt.status === 0) {
            throw new Error('Transaction reverted');
        }

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'TransferMemo', 'success', `${amount} ${tokenSymbol} → ${toAddress.substring(0, 10)} | "${memo}"`, silent, duration);
        if (!silent) console.log(`${COLORS.fg.green}✓ Sent successfully! Block: ${receipt.blockNumber}${COLORS.reset}`);
        return { success: true, txHash: tx.hash, block: receipt.blockNumber, memo };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'TransferMemo', 'failed', error.message.substring(0, 50), silent, duration);
        console.log(`${COLORS.fg.red}✗ Transfer failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function runTransferMemoMenu() {
    console.log(`\n  ${COLORS.fg.magenta}💬  TRANSFER WITH MEMO MODULE${COLORS.reset}\n`);

    const mode = await askQuestion(`${COLORS.fg.cyan}1. Random recipient\n2. Specific address\nChoose (1-2): ${COLORS.reset}`);

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

    const memoInput = await askQuestion(`${COLORS.fg.cyan}Memo message: ${COLORS.reset}`);
    const memo = memoInput || 'No memo';

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
            : ethers.Wallet.createRandom().address;

        await transferMemoForWallet(wallet, proxy, tokenAddress, tokenSymbol, finalRecipient, amount, memo, 1, i);

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(5, 10), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ All transfers completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}
