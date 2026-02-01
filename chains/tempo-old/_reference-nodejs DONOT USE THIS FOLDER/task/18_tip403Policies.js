import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier } from '../utils/helpers.js';

// Correct ABI for TIP-403 Registry based on Python reference
const TIP403_REGISTRY_ABI = [
    "function createPolicy(address admin, uint8 policyType) returns (uint64)",
    "function createPolicyWithAccounts(address admin, uint8 policyType, address[] accounts) returns (uint64)",
    "function isAuthorized(uint64 policyId, address user) view returns (bool)"
];

export async function manageTIP403PolicyForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const tip403Address = SYSTEM_CONTRACTS.TIP403_REGISTRY;

    if (!tip403Address) {
        if (!silent) console.log(`${COLORS.fg.red}TIP403_REGISTRY address missing${COLORS.reset}`);
        return { success: false, reason: 'tip403_address_missing' };
    }

    if (!silent) console.log(`${COLORS.fg.yellow}Creating TIP-403 Policy...${COLORS.reset}`);

    try {
        const registryContract = new ethers.Contract(tip403Address, TIP403_REGISTRY_ABI, wallet);

        // 0 = Whitelist, 1 = Blacklist
        const policyType = 0;

        if (!silent) console.log(`${COLORS.fg.cyan}Creating new Whitelist policy...${COLORS.reset}`);

        // Estimate gas first to ensure it doesn't revert
        const gasEstimate = await registryContract.createPolicy.estimateGas(wallet.address, policyType);

        let success = false;
        let retryCount = 0;
        let receipt;
        let tx;

        while (!success && retryCount < 3) {
            try {
                const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
                const nonce = await wallet.getNonce("pending");

                tx = await registryContract.createPolicy(wallet.address, policyType, {
                    gasLimit: Math.floor(Number(gasEstimate) * 1.2), // Add 20% buffer
                    ...gasOverrides,
                    nonce: nonce
                });

                if (!silent) console.log(`${COLORS.dim}Tx: ${CONFIG.EXPLORER_URL}/tx/${tx.hash}${COLORS.reset}`);
                receipt = await tx.wait();
                success = true;

            } catch (err) {
                const errMsg = err.message.toLowerCase();
                if (errMsg.includes('nonce') || errMsg.includes('replacement')) {
                    if (!silent) console.log(`${COLORS.fg.yellow}⚠ Nonce error (attempt ${retryCount + 1}): ${err.shortMessage || err.message}. Retrying...${COLORS.reset}`);
                    retryCount++;
                    await sleep(2000);
                } else {
                    throw err;
                }
            }
        }

        if (!success) throw new Error("TIP-403 Policy creation failed after nonce retries");

        if (receipt.status === 1) {
            // Try to parse logs to find policy ID if possible, or just succeed
            if (!silent) {
                const duration = (Date.now() - startTime) / 1000;
                logWalletAction(workerId, walletIndex, wallet.address, 'TIP403', 'success', 'Policy Created', silent, duration);
            }
            if (!silent) console.log(`${COLORS.fg.green}✓ Policy created! Block: ${receipt.blockNumber}${COLORS.reset}`);

            return { success: true, txHash: tx.hash, block: receipt.blockNumber };
        } else {
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'TIP403', 'failed', 'Transaction reverted', silent, duration);
            if (!silent) console.log(`${COLORS.fg.red}✗ Transaction reverted${COLORS.reset}`);
            return { success: false, reason: 'transaction_reverted' };
        }

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'TIP403', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Creation failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function runTIP403PoliciesMenu() {
    console.log(`\n  ${COLORS.fg.magenta}📋  TIP-403 POLICIES MODULE${COLORS.reset}\n`);
    console.log(`${COLORS.fg.yellow}Register TIP-403 policies${COLORS.reset}\n`);

    const privateKeys = getPrivateKeys();
    if (privateKeys.length === 0) {
        console.log(`${COLORS.fg.red}No private keys found${COLORS.reset}`);
        return;
    }

    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);

        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}${COLORS.reset}`);
        console.log(`${COLORS.fg.cyan}Address: ${wallet.address}${COLORS.reset}`);

        await manageTIP403PolicyForWallet(wallet, proxy, 1, i);

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(3, 6), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ Policy registration completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}
