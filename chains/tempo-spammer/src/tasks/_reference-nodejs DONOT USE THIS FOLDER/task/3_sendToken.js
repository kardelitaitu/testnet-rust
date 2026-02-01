import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';

const ERC20_ABI = [
    "function balanceOf(address owner) view returns (uint256)",
    "function decimals() view returns (uint8)",
    "function transfer(address to, uint256 amount) returns (bool)",
    "function symbol() view returns (string)"
];

export async function sendTokenForWallet(wallet, proxy, tokenAddress, amount, recipientAddress, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const shortAddr = `${wallet.address.substring(0, 6)}...${wallet.address.substring(38)}`;
    const shortDst = `${recipientAddress.substring(0, 6)}...${recipientAddress.substring(38)}`;

    if (!silent) console.log(`${COLORS.fg.yellow}Sending ${amount} tokens to ${shortDst}...${COLORS.reset}`);

    try {
        const token = new ethers.Contract(tokenAddress, ERC20_ABI, wallet);

        // Check decimals
        let decimals = 18;
        try {
            decimals = await token.decimals();
        } catch (e) {
            if (!silent) console.log(`${COLORS.dim}Using default decimals: 18${COLORS.reset}`);
        }
        const amountWei = ethers.parseUnits(amount.toString(), decimals);

        // Check balance
        const balance = await token.balanceOf(wallet.address);
        const balanceFormatted = ethers.formatUnits(balance, decimals);
        if (!silent) console.log(`${COLORS.dim}Balance: ${balanceFormatted}${COLORS.reset}`);

        if (balance < amountWei) {
            if (!silent) console.log(`${COLORS.fg.red}✗ Insufficient balance: ${balanceFormatted} < ${amount}${COLORS.reset}`);
            logWalletAction(workerId, walletIndex, wallet.address, 'SendToken', 'failed', 'Insufficient balance', silent);
            return { success: false, reason: 'insufficient_balance' };
        }

        // Send
        // Send
        const txCreator = async () => {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            return token.transfer(recipientAddress, amountWei, {
                gasLimit: 100000,
                ...gasOverrides
            });
        };

        const { hash, receipt } = await sendTxWithRetry(wallet, txCreator);
        const tx = { hash }; // For compatibility

        if (!silent) console.log(`${COLORS.dim}Tx Sent: ${CONFIG.EXPLORER_URL}/tx/${hash}${COLORS.reset}`);

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'SendToken', 'success', `${amount} -> ${shortDst}`, silent, duration);
        if (!silent) console.log(`${COLORS.fg.green}✓ Sent successfully! Block: ${receipt.blockNumber}${COLORS.reset}`);
        return { success: true, txHash: tx.hash, block: receipt.blockNumber, amount, tokenAddress };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'SendToken', 'failed', error.message.substring(0, 50), silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Send failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}



export async function sendRandomTokenForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    // 1. Pick Random Token
    const tokenEntries = Object.entries(CONFIG.TOKENS);
    if (tokenEntries.length === 0) {
        if (!silent) console.log(`${COLORS.fg.red}No tokens configured in CONFIG.TOKENS${COLORS.reset}`);
        return { success: false, reason: 'no_tokens_configured' };
    }
    const [symbol, address] = tokenEntries[Math.floor(Math.random() * tokenEntries.length)];

    // 2. Random Amount (10 - 50)
    const min = 10;
    const max = 50;
    const amount = (Math.random() * (max - min) + min).toFixed(2);

    // 3. Random Destination
    const toAddress = ethers.Wallet.createRandom().address;

    return await sendTokenForWallet(wallet, proxy, address, amount, toAddress, workerId, walletIndex, silent);
}

export async function runSendTokenMenu() {
    console.log(`\n  ${COLORS.fg.magenta}💸  TOKEN SEND MODULE${COLORS.reset}\n`);

    const tokenList = Object.entries(CONFIG.TOKENS);
    console.log(`${COLORS.fg.yellow}Available tokens:${COLORS.reset}`);
    tokenList.forEach(([name, _], idx) => {
        console.log(`  ${COLORS.fg.cyan}${idx + 1}. ${name}${COLORS.reset}`);
    });
    console.log(`  ${COLORS.fg.cyan}${tokenList.length + 1}. All tokens${COLORS.reset}\n`);

    const tokenChoice = await askQuestion(`${COLORS.fg.cyan}Select token (number): ${COLORS.reset}`);
    const tokenIndex = parseInt(tokenChoice) - 1;

    if (isNaN(tokenIndex) || tokenIndex < 0 || tokenIndex > tokenList.length) {
        console.log(`${COLORS.fg.red}Invalid choice${COLORS.reset}`);
        return;
    }

    const tokensToSend = (tokenIndex === tokenList.length) ? tokenList : [tokenList[tokenIndex]];

    console.log(`\n${COLORS.fg.yellow}Send destination:${COLORS.reset}`);
    console.log(`  ${COLORS.fg.cyan}1. Random address${COLORS.reset}`);
    console.log(`  ${COLORS.fg.cyan}2. Enter address manually${COLORS.reset}\n`);

    const destChoice = await askQuestion(`${COLORS.fg.cyan}Choose (1-2): ${COLORS.reset}`);
    let toAddress = null;
    const useRandomAddress = (destChoice === '1');

    if (!useRandomAddress) {
        toAddress = await askQuestion(`${COLORS.fg.cyan}Enter recipient address: ${COLORS.reset}`);
        if (!ethers.isAddress(toAddress)) {
            console.log(`${COLORS.fg.red}Invalid address${COLORS.reset}`);
            return;
        }
    }

    const amountInput = await askQuestion(`${COLORS.fg.cyan}Amount to send (default 1): ${COLORS.reset}`);
    const amount = parseFloat(amountInput) || 1.0;

    const privateKeys = getPrivateKeys();
    console.log(`\n${COLORS.fg.cyan}Found ${privateKeys.length} wallet(s)${COLORS.reset}\n`);

    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);

        const proxyMsg = proxy ? `Using Proxy: ${proxy}` : "Using: Direct Connection";
        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}${COLORS.reset}`);
        console.log(`${COLORS.fg.cyan}Address: ${wallet.address}${COLORS.reset}`);
        console.log(`${COLORS.fg.cyan}${proxyMsg}${COLORS.reset}\n`);

        for (const [symbol, address] of tokensToSend) {
            const dest = useRandomAddress ? ethers.Wallet.createRandom().address : toAddress;
            await sendTokenForWallet(wallet, proxy, address, symbol, dest, amount, 1, i);

            if (tokensToSend.length > 1) await sleep(2000);
        }

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(5, 10), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ All transfers completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}
