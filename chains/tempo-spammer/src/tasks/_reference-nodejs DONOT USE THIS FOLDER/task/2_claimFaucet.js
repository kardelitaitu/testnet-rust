import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt } from '../utils/helpers.js';

// Faucet Logic
export async function claimFaucetForWallet(wallet, proxy, claimCount, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const address = wallet.address;
    let finalResult = { success: false, reason: 'No claims attempted' };

    for (let c = 1; c <= claimCount; c++) {
        const shortAddr = `${address.substring(0, 6)}...${address.substring(38)}`;
        if (!silent) console.log(`${COLORS.fg.cyan}⟳ Sending faucet request for ${shortAddr}... (${c}/${claimCount})${COLORS.reset}`);

        try {
            const result = await wallet.provider.send('tempo_fundAddress', [address]);

            let txHashes = [];
            if (Array.isArray(result)) {
                txHashes = result;
            } else if (result) {
                txHashes = [result];
            }

            const duration = (Date.now() - startTime) / 1000;
            logWalletAction(workerId, walletIndex, wallet.address, 'Faucet', 'success', `Claims: ${txHashes.length}`, silent, duration, proxy);
            if (!silent) console.log(`${COLORS.fg.green}✓ Faucet claimed successfully!${COLORS.reset}`);

            // Log tokens
            CONFIG.FAUCET_TOKENS.forEach((token, idx) => {
                if (idx < txHashes.length) {
                    const tx = txHashes[idx];
                    if (!silent) console.log(`${COLORS.fg.green}✓${COLORS.reset} ${token.amount} ${token.symbol} : ${COLORS.fg.cyan}${CONFIG.EXPLORER_URL}/tx/${tx}${COLORS.reset}`);
                }
            });

            // Get block number from the first transaction
            let blockNumber = 'unknown';
            if (txHashes.length > 0) {
                try {
                    const provider = wallet.provider;
                    const receipt = await provider.waitForTransaction(txHashes[0], 1, 10000); // Wait up to 10s
                    if (receipt) {
                        blockNumber = receipt.blockNumber;
                    }
                } catch (e) {
                    // Ignore receipt fetch error, not critical
                }
            }

            finalResult = { success: true, txHashes, txHash: txHashes[0], claimCount: txHashes.length, block: blockNumber };

        } catch (error) {
            const duration = (Date.now() - startTime) / 1000;
            logWalletAction(workerId, walletIndex, wallet.address, 'Faucet', 'failed', error.message, silent, duration);
            if (!silent) console.log(`${COLORS.fg.red}✗ Claim failed: ${error.message}${COLORS.reset}`);
            finalResult = { success: false, reason: error.message };
        }

        if (c < claimCount) {
            await countdown(CONFIG.FAUCET_CLAIM_DELAY_SEC, 'Next claim in');
        }
    }
    return finalResult;
}

export async function claimRandomFaucetForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const claimCount = 1; // Default to 1 claim
    return await claimFaucetForWallet(wallet, proxy, claimCount, workerId, walletIndex, silent);
}

// Interactive Menu Handler
export async function runFaucetMenu() {
    console.log(`\n  ${COLORS.fg.magenta}💧  FAUCET MODULE${COLORS.reset}\n`);

    const privateKeys = getPrivateKeys();
    console.log(`${COLORS.fg.cyan}Found ${privateKeys.length} wallet(s)${COLORS.reset}`);

    if (privateKeys.length === 0) {
        console.log(`${COLORS.fg.red}Private keys not found in pv.txt${COLORS.reset}`);
        return;
    }

    let claimCount = 1;
    while (true) {
        const input = await askQuestion(`${COLORS.fg.cyan}Number of faucet claims per wallet (1-100): ${COLORS.reset}`);
        const v = parseInt(input);
        if (!isNaN(v) && v >= 1 && v <= 100) {
            claimCount = v;
            break;
        }
        console.log(`${COLORS.fg.red}Enter a number between 1 and 100${COLORS.reset}`);
    }

    console.log(`\n${COLORS.fg.green}Claims per wallet set to: ${claimCount}${COLORS.reset}\n`);

    for (let i = 0; i < privateKeys.length; i++) {
        const pk = privateKeys[i];
        const { wallet, proxy } = await getWallet(i, pk);

        const proxyMsg = proxy ? `Using Proxy: ${proxy}` : "Using: Direct Connection";

        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}${COLORS.reset}`);
        console.log(`${COLORS.fg.cyan}Address: ${wallet.address}${COLORS.reset}`);
        console.log(`${COLORS.fg.cyan}${proxyMsg}${COLORS.reset}\n`);

        await claimFaucetForWallet(wallet, proxy, claimCount, 1, i);

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(1, 3), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ All faucet claims completed.${COLORS.reset}\n`);
    await countdown(CONFIG.FAUCET_FINISH_DELAY_SEC, 'Returning to main menu in');
}
