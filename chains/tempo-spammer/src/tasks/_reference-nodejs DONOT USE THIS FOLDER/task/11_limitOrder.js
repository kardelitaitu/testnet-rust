import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';

const STABLECOIN_DEX_ABI = [
    "function place(address token, uint128 amount, bool isBid, int16 tick) returns (uint128 orderId)",
    "event OrderPlaced(uint128 indexed orderId, address indexed user, address indexed token, uint128 amount, bool isBid, int16 tick)"
];

const ERC20_ABI = [
    "function balanceOf(address owner) view returns (uint256)",
    "function allowance(address owner, address spender) view returns (uint256)",
    "function approve(address spender, uint256 amount) returns (bool)",
    "function symbol() view returns (string)",
    "function decimals() view returns (uint8)"
];

export async function placeRandomLimitOrderForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const tokenEntries = Object.entries(CONFIG.TOKENS);
    if (tokenEntries.length === 0) {
        if (!silent) console.log(`${COLORS.fg.red}No tokens configured.${COLORS.reset}`);
        return { success: false, reason: 'no_tokens_configured' };
    }

    // Filter out PathUSD - we only want to trade OTHER tokens against PathUSD
    const nonPathUSDTokens = tokenEntries.filter(([symbol, _]) => symbol !== 'PathUSD');

    if (nonPathUSDTokens.length === 0) {
        if (!silent) console.log(`${COLORS.fg.red}No non-PathUSD tokens configured for limit orders.${COLORS.reset}`);
        return { success: false, reason: 'no_tradeable_tokens' };
    }

    // 1. Random token selection (excluding PathUSD)
    const [tokenSymbol, tokenAddress] = nonPathUSDTokens[Math.floor(Math.random() * nonPathUSDTokens.length)];

    // 2. Random order type (BID = buy, ASK = sell)
    // BID: Buy token with PathUSD
    // ASK: Sell token for PathUSD
    // 2. Force BID (Buy) for reliability as ASK (Sell) seems to revert on DEX currently
    const isBid = true;

    // 3. Random amount (500-1000) for higher volume
    const amount = (Math.random() * 500 + 500).toFixed(2);

    // 4. Tick 0 (Peg $1.00) is safest/most common for stats
    const tick = 0;

    if (!silent) console.log(`${COLORS.dim}[PathUSD pair] ${isBid ? 'Buying' : 'Selling'} ${tokenSymbol}${COLORS.reset}`);

    return await placeLimitOrderForWallet(wallet, proxy, tokenSymbol, tokenAddress, isBid, amount, tick, workerId, walletIndex, silent);
}

export async function placeLimitOrderForWallet(wallet, proxy, tokenSymbol, tokenAddress, isBid, amount, tick, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    let dexAddress = SYSTEM_CONTRACTS.STABLECOIN_DEX;
    if (!dexAddress) {
        if (!silent) console.log(`${COLORS.fg.red}STABLECOIN_DEX address missing config.${COLORS.reset}`);
        return { success: false, reason: 'dex_address_missing' };
    }

    try {
        dexAddress = ethers.getAddress(dexAddress);
    } catch (e) {
        if (!silent) console.log(`${COLORS.fg.red}Invalid DEX Address.${COLORS.reset}`);
        return { success: false, reason: 'invalid_dex_address' };
    }

    const orderType = isBid ? 'BID' : 'ASK';
    if (!silent) console.log(`${COLORS.fg.yellow}Placing ${orderType} order: ${amount} ${tokenSymbol} @ tick ${tick}${COLORS.reset}`);

    try {
        const dex = new ethers.Contract(dexAddress, STABLECOIN_DEX_ABI, wallet);

        // Determine which token to approve
        // BID: Approve PathUSD (buying token with PathUSD)
        // ASK: Approve the token itself (selling token for PathUSD)
        const tokenToApprove = isBid ? CONFIG.TOKENS.PathUSD : tokenAddress;
        const tokenToApproveSymbol = isBid ? 'PathUSD' : tokenSymbol;

        if (!tokenToApprove) {
            if (!silent) console.log(`${COLORS.fg.red}Token to approve not found.${COLORS.reset}`);
            return { success: false, reason: 'token_not_found' };
        }

        const tokenContract = new ethers.Contract(tokenToApprove, ERC20_ABI, wallet);

        let decimals = 6; // Default to 6 for stablecoins
        try {
            decimals = await tokenContract.decimals();
        } catch (e) {
            decimals = 6;
        }

        const amountWei = ethers.parseUnits(amount.toString(), decimals);

        // Check balance
        const bal = await tokenContract.balanceOf(wallet.address);
        const balFormatted = ethers.formatUnits(bal, decimals);

        if (!silent) console.log(`${COLORS.dim}Balance ${tokenToApproveSymbol}: ${balFormatted}${COLORS.reset}`);

        if (bal < amountWei) {
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'LimitOrder', 'skipped', `Insufficient ${tokenToApproveSymbol}`, silent, duration);
            if (!silent) console.log(`${COLORS.fg.red}✗ Insufficient ${tokenToApproveSymbol}: need ${amount}, have ${balFormatted}${COLORS.reset}`);
            return { success: false, reason: 'insufficient_balance' };
        }

        // Approve
        const allowance = await tokenContract.allowance(wallet.address, dexAddress);
        // Use 3x gas multiplier for speed
        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);

        if (allowance < amountWei) {
            if (!silent) console.log(`${COLORS.dim}Approving ${tokenToApproveSymbol}...${COLORS.reset}`);

            const approveTxCreator = async () => {
                const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
                return tokenContract.approve(dexAddress, ethers.MaxUint256, { ...gasOverrides });
            };
            await sendTxWithRetry(wallet, approveTxCreator);
            if (!silent) console.log(`${COLORS.dim}✓ Approved${COLORS.reset}`);
        }

        // Place order
        if (!silent) {
            console.log(`${COLORS.fg.cyan}Placing ${orderType} order...${COLORS.reset}`);
            console.log(`${COLORS.dim}  Token: ${tokenSymbol} (${tokenAddress})${COLORS.reset}`);
            console.log(`${COLORS.dim}  Amount: ${amount} (${amountWei.toString()} wei)${COLORS.reset}`);
            console.log(`${COLORS.dim}  isBid: ${isBid}${COLORS.reset}`);
            console.log(`${COLORS.dim}  Tick: ${tick}${COLORS.reset}`);
        }
        const placeTxCreator = async () => {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            return dex.place(tokenAddress, amountWei, isBid, tick, {
                gasLimit: 3000000,
                ...gasOverrides
            });
        };

        const { hash, receipt } = await sendTxWithRetry(wallet, placeTxCreator);
        const tx = { hash }; // For compatibility

        // Try to extract orderId from events
        let orderId = null;
        for (const log of receipt.logs) {
            try {
                const parsedLog = dex.interface.parseLog(log);
                if (parsedLog && parsedLog.name === 'OrderPlaced') {
                    orderId = parsedLog.args.orderId.toString();
                    break;
                }
            } catch (e) { /* ignore */ }
        }

        if (!silent) {
            const duration = (Date.now() - startTime) / 1000;
            logWalletAction(workerId, walletIndex, wallet.address, 'LimitOrder', 'success', `${orderType} ${amount} ${tokenSymbol}${orderId ? ` #${orderId}` : ''}`, silent, duration);
        }
        if (!silent) console.log(`${COLORS.fg.green}✓ Order placed! Block: ${receipt.blockNumber}${orderId ? ` | Order ID: ${orderId}` : ''}${COLORS.reset}`);

        return { success: true, txHash: tx.hash, block: receipt.blockNumber, orderType, token: tokenSymbol, amount, tick, orderId };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;

        // Handle reverts gracefully to avoid stopping the worker
        if (error.message.includes('revert') || error.message.includes('execution reverted')) {
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'LimitOrder', 'success', `(Skipped) Revert detected`, silent, duration);
            if (!silent) console.log(`${COLORS.fg.yellow}⚠ Order placement reverted (likely DEX issue). Treating as skipped.${COLORS.reset}`);
            return { success: true, skipped: true, reason: 'revert_handled' };
        }

        if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'LimitOrder', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Order placement failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function runLimitOrderMenu() {
    console.log(`\n  ${COLORS.fg.magenta}📊  LIMIT ORDER MODULE${COLORS.reset}\n`);
    console.log(`${COLORS.fg.yellow}Place limit orders on the DEX${COLORS.reset}\n`);

    const privateKeys = getPrivateKeys();
    if (privateKeys.length === 0) {
        console.log(`${COLORS.fg.red}No private keys found in pv.txt${COLORS.reset}`);
        return;
    }

    const tokenList = Object.entries(CONFIG.TOKENS);
    if (tokenList.length === 0) {
        console.log(`${COLORS.fg.red}No tokens configured${COLORS.reset}`);
        return;
    }

    console.log(`${COLORS.fg.cyan}Select token:${COLORS.reset}`);
    for (let i = 0; i < tokenList.length; i++) {
        console.log(`${COLORS.fg.cyan}  ${i + 1}. ${tokenList[i][0]}${COLORS.reset}`);
    }

    const tokenChoice = await askQuestion(`${COLORS.fg.cyan}Choose (1-${tokenList.length}): ${COLORS.reset}`);
    const tokenIndex = parseInt(tokenChoice) - 1;
    if (tokenIndex < 0 || tokenIndex >= tokenList.length) {
        console.log(`${COLORS.fg.red}Invalid choice${COLORS.reset}`);
        return;
    }

    const [tokenSymbol, tokenAddress] = tokenList[tokenIndex];

    console.log(`\n${COLORS.fg.cyan}Order type:${COLORS.reset}`);
    console.log(`${COLORS.fg.cyan}  1. BID (buy token for PathUSD)${COLORS.reset}`);
    console.log(`${COLORS.fg.cyan}  2. ASK (sell token for PathUSD)${COLORS.reset}`);

    const typeChoice = await askQuestion(`${COLORS.fg.cyan}Choose (1-2): ${COLORS.reset}`);
    const isBid = typeChoice === '1';

    const amountInput = await askQuestion(`${COLORS.fg.cyan}Amount (default 10): ${COLORS.reset}`);
    const amount = amountInput || '10';

    const tickInput = await askQuestion(`${COLORS.fg.cyan}Tick (default 0 = $1.00): ${COLORS.reset}`);
    const tick = tickInput ? parseInt(tickInput) : 0;

    console.log(`\n${COLORS.fg.green}Parameters:${COLORS.reset}`);
    console.log(`  Token: ${tokenSymbol}`);
    console.log(`  Type: ${isBid ? 'BID (buy)' : 'ASK (sell)'}`);
    console.log(`  Amount: ${amount}`);
    console.log(`  Tick: ${tick}\n`);

    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);

        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}${COLORS.reset}`);
        console.log(`${COLORS.fg.cyan}Address: ${wallet.address}${COLORS.reset}`);

        await placeLimitOrderForWallet(wallet, proxy, tokenSymbol, tokenAddress, isBid, amount, tick, 1, i);

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(3, 6), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ All orders placed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}
