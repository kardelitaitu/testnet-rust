import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';

const INFINITY_NAME_ABI = [
    "function register(string domain, address referrer) returns (uint256)",
    "function isAvailable(string domain) view returns (bool)",
    "function price() view returns (uint256)"
];

const ERC20_ABI = [
    "function balanceOf(address owner) view returns (uint256)",
    "function allowance(address owner, address spender) view returns (uint256)",
    "function approve(address spender, uint256 amount) returns (bool)"
];

function generateRandomDomainName(length = 10) {
    const chars = 'abcdefghijklmnopqrstuvwxyz0123456789';
    let name = chars.charAt(Math.floor(Math.random() * 26)); // Start with letter

    for (let i = 1; i < length; i++) {
        name += chars.charAt(Math.floor(Math.random() * chars.length));
    }

    return name;
}

export async function mintRandomDomainForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const domainName = generateRandomDomainName(10);

    return await mintDomainForWallet(wallet, proxy, domainName, workerId, walletIndex, silent);
}

export async function mintDomainForWallet(wallet, proxy, domainName, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const infinityAddress = SYSTEM_CONTRACTS.INFINITY_NAME_CONTRACT;

    if (!infinityAddress) {
        if (!silent) console.log(`${COLORS.fg.red}INFINITY_NAME_CONTRACT address missing${COLORS.reset}`);
        return { success: false, reason: 'infinity_address_missing' };
    }

    if (!silent) console.log(`${COLORS.fg.yellow}Registering domain: ${domainName}.tempo${COLORS.reset}`);

    try {
        const infinityContract = new ethers.Contract(infinityAddress, INFINITY_NAME_ABI, wallet);
        const pathUSDAddress = CONFIG.TOKENS.PathUSD;

        if (!pathUSDAddress) {
            if (!silent) console.log(`${COLORS.fg.red}PathUSD address missing${COLORS.reset}`);
            return { success: false, reason: 'pathusd_missing' };
        }

        const pathUSDContract = new ethers.Contract(pathUSDAddress, ERC20_ABI, wallet);

        // Check PathUSD balance
        const balance = await pathUSDContract.balanceOf(wallet.address);
        const balanceFormatted = ethers.formatUnits(balance, 6);

        if (!silent) console.log(`${COLORS.dim}PathUSD balance: ${balanceFormatted}${COLORS.reset}`);

        // Check availability (may fail)
        try {
            const available = await infinityContract.isAvailable(domainName);
            if (!silent) console.log(`${COLORS.dim}Domain available: ${available}${COLORS.reset}`);

            if (!available) {
                const duration = (Date.now() - startTime) / 1000;
                if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'MintDomain', 'skipped', 'Domain not available', silent, duration);
                if (!silent) console.log(`${COLORS.fg.yellow}⚠ Domain not available${COLORS.reset}`);
                return { success: false, reason: 'domain_not_available' };
            }
        } catch (e) {
            if (!silent) console.log(`${COLORS.dim}Could not check availability${COLORS.reset}`);
        }

        // Approve PathUSD
        const allowance = await pathUSDContract.allowance(wallet.address, infinityAddress);
        const approveAmount = ethers.parseUnits("1000", 6);

        if (allowance < approveAmount) {
            if (!silent) console.log(`${COLORS.dim}Approving PathUSD...${COLORS.reset}`);
            // Use 3x gas multiplier for speed
            await sendTxWithRetry(wallet, async () => {
                const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
                return pathUSDContract.approve(infinityAddress, ethers.MaxUint256, { ...gasOverrides });
            });
            if (!silent) console.log(`${COLORS.dim}✓ Approved${COLORS.reset}`);
        }

        // Register domain
        if (!silent) console.log(`${COLORS.fg.cyan}Registering ${domainName}.tempo...${COLORS.reset}`);

        const result = await sendTxWithRetry(wallet, async () => {
            const gasOverridesReg = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            return infinityContract.register(domainName, ethers.ZeroAddress, {
                gasLimit: 500000,
                ...gasOverridesReg
            });
        });

        const receipt = result.receipt;
        const txHash = result.hash;

        if (!silent) console.log(`${COLORS.dim}Tx: ${CONFIG.EXPLORER_URL}/tx/${txHash}${COLORS.reset}`);

        if (receipt.status === 1) {
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'MintDomain', 'success', `${domainName}.tempo`, silent, duration);
            if (!silent) console.log(`${COLORS.fg.green}✓ Domain registered! Block: ${receipt.blockNumber}${COLORS.reset}`);
            return { success: true, txHash: txHash, block: receipt.blockNumber, domain: `${domainName}.tempo` };
        } else {
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'MintDomain', 'failed', 'Transaction reverted', silent, duration);
            if (!silent) console.log(`${COLORS.fg.red}✗ Transaction reverted${COLORS.reset}`);
            return { success: false, reason: 'transaction_reverted' };
        }

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'MintDomain', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Registration failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function runMintDomainMenu() {
    console.log(`\n  ${COLORS.fg.magenta}🌐  INFINITY NAME MODULE${COLORS.reset}\n`);
    console.log(`${COLORS.fg.yellow}Register domains on Infinity Name service${COLORS.reset}\n`);

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

        await mintRandomDomainForWallet(wallet, proxy, 1, i);

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(5, 10), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ Domain registration completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}
