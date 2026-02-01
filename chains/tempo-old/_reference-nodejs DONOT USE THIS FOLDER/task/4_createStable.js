
import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet, saveCreatedToken } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier } from '../utils/helpers.js';
import { getRandomText } from '../utils/randomText.js';
import { ConcurrentService } from '../utils/tempoConcurrent.js';

const TIP20_FACTORY_ABI = [
    "function createToken(string name, string symbol, string currency, address quoteToken, address admin, bytes32 salt) returns (address)",
    "event TokenCreated(address indexed token, string name, string symbol, string currency, address quoteToken, address admin, bytes32 salt)"
];

const ERC20_ABI = [
    "function allowance(address owner, address spender) view returns (uint256)",
    "function approve(address spender, uint256 amount) returns (bool)",
    "function grantRole(bytes32 role, address account)"
];

function generateRandomTokenName() {
    const prefixes = ['Alpha', 'Beta', 'Gamma', 'Delta', 'Omega', 'Nova', 'Stellar', 'Crypto', 'Digital', 'Meta'];
    const suffixes = ['Dollar', 'Coin', 'Cash', 'Pay', 'Money', 'Finance', 'Capital', 'Fund'];
    const prefix = prefixes[Math.floor(Math.random() * prefixes.length)];
    const suffix = suffixes[Math.floor(Math.random() * suffixes.length)];
    return `${prefix} ${suffix}`;
}

function generateRandomSymbol() {
    const letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let symbol = "";
    for (let i = 0; i < 3; i++) {
        symbol += letters.charAt(Math.floor(Math.random() * letters.length));
    }
    return symbol + 'USD';
}

export async function createRandomStableForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const tokenName = generateRandomTokenName();
    const tokenSymbol = generateRandomSymbol();
    const currency = 'USD'; // Default currency for random stablecoins
    const quoteToken = CONFIG.TOKENS.PathUSD; // Default quote token for random stablecoins

    return await createStableForWallet(wallet, proxy, tokenName, tokenSymbol, currency, quoteToken, workerId, walletIndex, silent);
}

export async function createStableForWallet(wallet, proxy, tokenName, tokenSymbol, currency, quoteToken, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    if (!silent) console.log(`${COLORS.fg.yellow}Creating stablecoin: ${tokenSymbol} (Atomic Batch)...${COLORS.reset}`);

    let tokenAddress = null;
    let txHash1 = null;

    try {
        const factoryAddress = SYSTEM_CONTRACTS.TIP20_FACTORY;
        if (!factoryAddress) throw new Error("TIP20_FACTORY address not found in SYSTEM_CONTRACTS");
        const factory = new ethers.Contract(factoryAddress, TIP20_FACTORY_ABI, wallet);

        // --- BATCH 1: Create Token ---

        const salt = ethers.hexlify(ethers.randomBytes(32));
        const createData = factory.interface.encodeFunctionData('createToken', [
            tokenName, tokenSymbol, currency, quoteToken, wallet.address, salt
        ]);

        const calls1 = [
            { to: factoryAddress, data: createData, value: 0n }
        ];

        const service = new ConcurrentService(wallet.privateKey, proxy);

        // 5M Gas for Factory Creation
        txHash1 = await service.sendAtomicBatch(calls1, Date.now(), CONFIG.TOKENS.PathUSD, { gas: 5000000n });

        if (!silent) console.log(`${COLORS.dim}Batch 1 (Create) sent: ${txHash1}...${COLORS.reset}`);

        // Wait for Receipt to get Token Address
        const publicClient = service.publicClient;
        const receipt = await publicClient.waitForTransactionReceipt({ hash: txHash1 });

        // Parse Logs manually to find TokenCreated
        // Event signature: TokenCreated(address,string,string,string,address,address,bytes32)
        // Topic 0: keccak256("TokenCreated(address,string,string,string,address,address,bytes32)")

        for (const log of receipt.logs) {
            try {
                // ethers v6 parsing
                const parsed = factory.interface.parseLog({
                    topics: [...log.topics],
                    data: log.data
                });
                if (parsed && parsed.name === 'TokenCreated') {
                    tokenAddress = parsed.args.token;
                    break;
                }
            } catch (e) { }
        }

        if (!tokenAddress) {
            throw new Error("Could not find TokenCreated event in receipt");
        }

        if (!silent) {
            console.log(`${COLORS.fg.green}✓ Token Created: ${tokenAddress}${COLORS.reset}`);
            console.log(`${COLORS.dim}Explorer: ${CONFIG.EXPLORER_URL}/address/${tokenAddress}${COLORS.reset}`);
        }

        // --- BATCH 2: Grant Role + Mint ---
        if (!silent) console.log(`${COLORS.dim}Sending Batch 2 (Setup)...${COLORS.reset}`);

        const MINT_ABI = [
            "function mint(address to, uint256 amount)",
            "function grantRole(bytes32 role, address account)"
        ];
        const tokenContract = new ethers.Contract(tokenAddress, MINT_ABI, wallet);
        const ISSUER_ROLE = ethers.id("ISSUER_ROLE");
        const mintAmount = ethers.parseUnits('100000', 6);

        // We assume tokenAddress is correct. 
        // Note: grantRole might be redundant if creator is admin by default, but keeping to be safe as per original logic.
        const grantData = tokenContract.interface.encodeFunctionData('grantRole', [ISSUER_ROLE, wallet.address]);
        const mintData = tokenContract.interface.encodeFunctionData('mint', [wallet.address, mintAmount]);

        const calls2 = [
            { to: tokenAddress, data: grantData, value: 0n },
            { to: tokenAddress, data: mintData, value: 0n }
        ];

        // Unique nonceKey for second batch (start + 1)
        // 2M Gas for Minting should be plenty
        const txHash2 = await service.sendAtomicBatch(calls2, Date.now() + 1, CONFIG.TOKENS.PathUSD, { gas: 2000000n });

        if (!silent) console.log(`${COLORS.dim}Batch 2 (Setup) sent: ${txHash2}...${COLORS.reset}`);

        // Wait for batch 2 confirmation
        const receipt2 = await publicClient.waitForTransactionReceipt({ hash: txHash2 });

        // Save result
        // Serialize blockNumber to string
        const blockNum = receipt.blockNumber ? receipt.blockNumber.toString() : '0';
        saveCreatedToken(wallet.address, tokenAddress, tokenSymbol, tokenName, blockNum);

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'CreateStable', 'success', `${tokenSymbol}: ${tokenAddress.substring(0, 10)}`, silent, duration);

        return { success: true, tokenAddress, symbol: tokenSymbol, tokenName, txHash: txHash1, block: blockNum };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'CreateStable', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Creation failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function runCreateStableMenu() {
    console.log(`\n  ${COLORS.fg.magenta}🪙  STABLECOIN MODULE${COLORS.reset}\n`);

    const mode = await askQuestion(`${COLORS.fg.cyan}1. Random Name/Symbol (Recommended)\n2. Manual Input\nChoose (1-2): ${COLORS.reset}`);

    let name, symbol;
    if (mode === '2') {
        name = await askQuestion(`${COLORS.fg.cyan}Token Name: ${COLORS.reset}`);
        symbol = await askQuestion(`${COLORS.fg.cyan}Token Symbol: ${COLORS.reset}`);
    } else {
        name = generateRandomTokenName();
        symbol = generateRandomSymbol();
    }

    const privateKeys = getPrivateKeys();
    console.log(`\n${COLORS.fg.cyan}Found ${privateKeys.length} wallet(s)${COLORS.reset}\n`);

    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);

        const proxyMsg = proxy ? `Using Proxy: ${proxy}` : "Using: Direct Connection";
        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}${COLORS.reset}`);
        console.log(`${COLORS.fg.cyan}Address: ${wallet.address}${COLORS.reset}`);

        const effectiveName = (mode === '2') ? name : generateRandomTokenName();
        const effectiveSymbol = (mode === '2') ? symbol : generateRandomSymbol();

        await createStableForWallet(wallet, proxy, effectiveName, effectiveSymbol, 'USD', CONFIG.TOKENS.PathUSD, 1, i);

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(5, 10), 'Next wallet in');
        }
    }
    console.log(`\n${COLORS.fg.green}✓ All tasks completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}
