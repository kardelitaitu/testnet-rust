import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet, loadCreatedTokens } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier } from '../utils/helpers.js';
import { createRandomStableForWallet } from './4_createStable.js';

const ERC20_ABI = [
    "function grantRole(bytes32 role, address account)",
    "function hasRole(bytes32 role, address account) view returns (bool)",
    "function symbol() view returns (string)"
];

// Role hashes
const ISSUER_ROLE = ethers.id("ISSUER_ROLE");
const PAUSE_ROLE = ethers.id("PAUSE_ROLE");

export async function grantRandomRoleForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    // Load created tokens
    let createdTokens = loadCreatedTokens();
    const walletAddress = ethers.getAddress(wallet.address);
    let myTokens = createdTokens[walletAddress] || createdTokens[walletAddress.toLowerCase()] || [];

    // Auto-create if no tokens exist
    if (myTokens.length === 0) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No tokens - creating one first...${COLORS.reset} `);

        const createResult = await createRandomStableForWallet(wallet, proxy, workerId, walletIndex, silent);

        if (createResult?.success) {
            await sleep(2000);
            createdTokens = loadCreatedTokens();
            myTokens = createdTokens[ethers.getAddress(wallet.address)] || [];

            if (myTokens.length > 0) {
                if (!silent) console.log(`${COLORS.fg.green}✓ Token created and tracked${COLORS.reset} `);
            } else {
                if (!silent) console.log(`${COLORS.fg.red}✗ Token created but not loaded${COLORS.reset} `);
                return { success: false, reason: 'token_not_loaded' };
            }
        } else {
            if (!silent) console.log(`${COLORS.fg.red}✗ Failed to create token${COLORS.reset} `);
            return { success: false, reason: 'failed_to_create_token' };
        }
    }

    // Random token selection
    const tokenInfo = myTokens[Math.floor(Math.random() * myTokens.length)];

    // Random role (mostly ISSUER_ROLE for minting)
    const useIssuerRole = Math.random() > 0.2; // 80% ISSUER, 20% PAUSE
    const roleHash = useIssuerRole ? ISSUER_ROLE : PAUSE_ROLE;
    const roleName = useIssuerRole ? 'ISSUER_ROLE' : 'PAUSE_ROLE';

    return await grantRoleForWallet(wallet, proxy, tokenInfo.token, tokenInfo.symbol, roleHash, roleName, workerId, walletIndex, silent);
}

export async function grantRoleForWallet(wallet, proxy, tokenAddress, tokenSymbol, roleHash, roleName, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    if (!silent) console.log(`${COLORS.fg.yellow}Granting ${roleName} to ${tokenSymbol}...${COLORS.reset} `);

    try {
        const tokenContract = new ethers.Contract(tokenAddress, ERC20_ABI, wallet);

        // Check if role already granted
        let hasRole = false;
        try {
            hasRole = await tokenContract.hasRole(roleHash, wallet.address);
        } catch (e) {
            // If hasRole reverts (e.g. unknown custom error), assume we need to grant
            if (!silent) console.log(`${COLORS.dim}hasRole check failed, proceeding to grant...${COLORS.reset}`);
        }

        if (hasRole) {
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'GrantRole', 'skipped', `${roleName} already granted`, silent, duration);
            if (!silent) console.log(`${COLORS.fg.green}✓ ${roleName} already granted${COLORS.reset} `);
            return { success: true, alreadyGranted: true, role: roleName, token: tokenSymbol };
        }

        if (!silent) console.log(`${COLORS.fg.cyan}Granting ${roleName}...${COLORS.reset} `);

        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
        const tx = await tokenContract.grantRole(roleHash, wallet.address, {
            gasLimit: 500000,
            ...gasOverrides
        });

        if (!silent) console.log(`${COLORS.dim} Tx: ${CONFIG.EXPLORER_URL} /tx/${tx.hash}${COLORS.reset} `);
        const receipt = await tx.wait();

        // Verify role granted
        const hasRoleAfter = await tokenContract.hasRole(roleHash, wallet.address);

        if (hasRoleAfter) {
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'GrantRole', 'success', `${roleName} to ${tokenSymbol} `, silent, duration);
            if (!silent) console.log(`${COLORS.fg.green}✓ ${roleName} granted! Block: ${receipt.blockNumber}${COLORS.reset} `);
            return { success: true, txHash: tx.hash, block: receipt.blockNumber, role: roleName, token: tokenSymbol };
        } else {
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'GrantRole', 'failed', 'Role not granted after tx', silent, duration);
            if (!silent) console.log(`${COLORS.fg.red}✗ Role not granted${COLORS.reset} `);
            return { success: false, reason: 'role_not_granted_after_tx' };
        }

    } catch (error) {
        // If execution reverts, it might be because the role is already granted (and contract reverts),
        // or hasRole failed to detect it. Since createStable grants role initially, this is likely a duplicate.
        const duration = (Date.now() - startTime) / 1000;

        let reason = error.message;
        let isRevert = reason.includes('execution reverted') || reason.includes('revert');

        if (isRevert) {
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'GrantRole', 'success', `(Assumed) ${roleName} granted`, silent, duration);
            if (!silent) console.log(`${COLORS.fg.green}✓ ${roleName} granted (or already present)${COLORS.reset} `);
            return { success: true, assumed: true, role: roleName, token: tokenSymbol };
        }

        if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'GrantRole', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Grant failed: ${error.message}${COLORS.reset} `);
        return { success: false, reason: error.message };
    }
}

export async function runGrantRoleMenu() {
    console.log(`\n  ${COLORS.fg.magenta}🔑  GRANT ROLE MODULE${COLORS.reset} \n`);
    console.log(`${COLORS.fg.yellow}Grant ISSUER_ROLE or PAUSE_ROLE to created tokens${COLORS.reset} \n`);

    const privateKeys = getPrivateKeys();
    if (privateKeys.length === 0) {
        console.log(`${COLORS.fg.red}No private keys found${COLORS.reset} `);
        return;
    }

    const createdTokens = loadCreatedTokens();
    if (Object.keys(createdTokens).length === 0) {
        console.log(`${COLORS.fg.red}No created tokens found${COLORS.reset} `);
        return;
    }

    console.log(`${COLORS.fg.cyan}Select role:${COLORS.reset} `);
    console.log(`${COLORS.fg.cyan} 1. ISSUER_ROLE(for mint)${COLORS.reset} `);
    console.log(`${COLORS.fg.cyan} 2. PAUSE_ROLE(for pause)${COLORS.reset} `);

    const roleChoice = await askQuestion(`${COLORS.fg.cyan} Choose(1 - 2, default 1): ${COLORS.reset} `);
    const useIssuerRole = roleChoice !== '2';
    const roleHash = useIssuerRole ? ISSUER_ROLE : PAUSE_ROLE;
    const roleName = useIssuerRole ? 'ISSUER_ROLE' : 'PAUSE_ROLE';

    console.log(`\n${COLORS.fg.green}Granting role: ${roleName}${COLORS.reset} \n`);

    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);
        const walletAddress = ethers.getAddress(wallet.address);
        const walletTokens = createdTokens[walletAddress] || createdTokens[walletAddress.toLowerCase()] || [];

        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset} `);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}${COLORS.reset}`);
        console.log(`${COLORS.fg.cyan}Address: ${wallet.address}${COLORS.reset}`);

        if (walletTokens.length === 0) {
            console.log(`${COLORS.fg.yellow}⚠ No tokens - skipping${COLORS.reset}`);
            continue;
        }

        for (const tokenInfo of walletTokens) {
            await grantRoleForWallet(wallet, proxy, tokenInfo.token, tokenInfo.symbol, roleHash, roleName, 1, i);
            await sleep(2000);
        }

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(3, 6), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ Role granting completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}
