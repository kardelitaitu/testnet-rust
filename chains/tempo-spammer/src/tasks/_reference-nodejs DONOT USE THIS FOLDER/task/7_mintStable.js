import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { loadCreatedTokens } from '../utils/wallet.js';
import { createRandomStableForWallet } from './4_createStable.js';
import { getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';

const MINT_ABI = [
    "function mint(address to, uint256 amount)",
    "function hasRole(bytes32 role, address account) view returns (bool)",
    "function grantRole(bytes32 role, address account)",
    "function decimals() view returns (uint8)",
    "function symbol() view returns (string)",
    "function balanceOf(address owner) view returns (uint256)"
];

const ISSUER_ROLE = ethers.id("ISSUER_ROLE");

export async function mintRandomTokenForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    // 1. Load Created Tokens (new format: object with wallet addresses as keys)
    let createdTokens = loadCreatedTokens();
    const walletAddress = ethers.getAddress(wallet.address);
    let myTokens = createdTokens[walletAddress] || createdTokens[walletAddress.toLowerCase()] || [];

    if (myTokens.length === 0) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No created tokens found. Creating a new one first...${COLORS.reset}`);
        // Auto-create logic
        const newStable = await createRandomStableForWallet(wallet, proxy, workerId, walletIndex, silent);

        if (newStable && newStable.success && newStable.tokenAddress) {
            myTokens = [{
                token: newStable.tokenAddress,
                symbol: newStable.symbol
            }];
        } else {
            // If creation failed, we can't do anything
            if (!silent) console.log(`${COLORS.fg.red}✗ Failed to create reserve token.${COLORS.reset}`);
            return { success: false, reason: 'auto_create_failed' };
        }
    }

    if (myTokens.length === 0) {
        if (!silent) console.log(`${COLORS.fg.red}✗ Still no tokens. Skipping.${COLORS.reset}`);
        return { success: false, reason: 'no_tokens_to_mint' };
    }

    if (!silent) console.log(`${COLORS.fg.cyan}Found ${myTokens.length} token(s) to mint for this wallet.${COLORS.reset}`);

    let overallSuccess = true;
    let lastReason = '';

    // 2. Select random tokens to try (Try up to 3 different ones)
    // Shuffle arrays
    const shuffledTokens = myTokens.sort(() => 0.5 - Math.random());
    const attempts = Math.min(10, shuffledTokens.length);

    for (let i = 0; i < attempts; i++) {
        const tokenInfo = shuffledTokens[i];
        const tokenAddress = tokenInfo.token;
        const tokenSymbol = tokenInfo.symbol || '???';

        // Random amount (10M - 20M)
        const amount = getRandomInt(10000000, 20000000);

        const result = await mintTokenForWallet(wallet, proxy, tokenAddress, tokenSymbol, amount, workerId, walletIndex, silent);

        if (result.success) return result;

        // If failed, log and try next
        lastReason = result.reason;
        if (!silent && i < attempts - 1) console.log(`${COLORS.dim}Mint failed for ${tokenSymbol}, trying another...${COLORS.reset}`);
    }

    // If all attempts failed
    return { success: false, reason: lastReason || 'all_mint_attempts_failed' };
}

export async function mintTokenForWallet(wallet, proxy, tokenAddress, tokenSymbol, amount, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    if (!silent) console.log(`${COLORS.fg.yellow}Minting ${amount} ${tokenSymbol}...${COLORS.reset}`);

    try {
        const token = new ethers.Contract(tokenAddress, MINT_ABI, wallet);

        // Check if contract exists
        const code = await wallet.provider.getCode(tokenAddress);
        if (code === '0x') {
            if (!silent) console.log(`${COLORS.dim}Token ${tokenSymbol} has no code (deleted/failed).${COLORS.reset}`);
            return { success: false, reason: 'contract_no_code' };
        }

        // Check Role (Best effort)
        let hasRole = false;
        try {
            hasRole = await token.hasRole(ISSUER_ROLE, wallet.address);
            if (!silent) console.log(`${COLORS.dim}[DEBUG] hasRole=${hasRole}${COLORS.reset}`);

            if (!hasRole) {
                console.log(`${COLORS.dim}Granting ISSUER_ROLE...${COLORS.reset}`);
                try {
                    const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
                    const txGrant = await token.grantRole(ISSUER_ROLE, wallet.address, {
                        gasLimit: 500000,
                        ...gasOverrides,
                        feeCurrency: CONFIG.TOKENS.PathUSD
                    });
                    await txGrant.wait();
                    if (!silent) console.log(`${COLORS.fg.green}✓ Role Granted${COLORS.reset}`);

                    // Wait for role propagation
                    if (!silent) console.log(`${COLORS.dim}Waiting 3s for role propagation...${COLORS.reset}`);
                    await new Promise(r => setTimeout(r, 3000));
                    hasRole = true;
                } catch (grantErr) {
                    if (!silent) console.log(`${COLORS.fg.yellow}⚠ Role grant skipped/failed: ${grantErr.message.substring(0, 50)}${COLORS.reset}`);
                    return { success: false, reason: `role_grant_failed: ${grantErr.message}` };
                }
            }

            // Final check
            hasRole = await token.hasRole(ISSUER_ROLE, wallet.address);
            if (!hasRole) {
                if (!silent) console.log(`${COLORS.fg.red}✗ Missing ISSUER_ROLE after attempt. Aborting mint.${COLORS.reset}`);
                return { success: false, reason: 'missing_issuer_role' };
            }

        } catch (roleErr) {
            // If hasRole reverts, it might be an Ownable contract (no hasRole), or just weird.
            // We should NOT skip, but try to mint anyway.
            if (!silent) console.log(`${COLORS.dim}Role check reverted (likely not AccessControl). Attempting mint directly...${COLORS.reset}`);
            // Fall through to minting logic below
        }

        // Proceed to mint - simplified for robustness
        let decimals = 6;
        try { decimals = await token.decimals(); } catch (e) { }

        const amountWei = ethers.parseUnits(amount.toString(), decimals);

        if (!silent) console.log(`${COLORS.dim}Calling mint(${wallet.address}, ${amountWei.toString()})...${COLORS.reset}`);



        const txCreator = async () => {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            return token.mint(wallet.address, amountWei, {
                gasLimit: 1000000,
                ...gasOverrides,
                feeCurrency: CONFIG.TOKENS.PathUSD
            });
        };

        const { hash, receipt } = await sendTxWithRetry(wallet, txCreator);

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'MintToken', 'success', `${amount} ${tokenSymbol}`, silent, duration);
        if (!silent) console.log(`${COLORS.fg.green}✓ Minted successfully! Block: ${receipt.blockNumber}${COLORS.reset}`);
        return { success: true, txHash: hash, block: receipt.blockNumber, token: tokenSymbol, tokenAddress: tokenAddress, amount };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'MintToken', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Mint failed: ${error.message}${COLORS.reset}`);
        if (error.data) if (!silent) console.log(`${COLORS.fg.red}Error Data: ${error.data}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function runMintTokenMenu() {
    console.log(`\n  ${COLORS.fg.magenta}🏭  MINT TOKEN MODULE${COLORS.reset}\n`);
    // Logic to iterate all wallets...
}
