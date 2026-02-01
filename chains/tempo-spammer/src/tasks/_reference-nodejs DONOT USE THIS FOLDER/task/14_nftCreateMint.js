import solc from 'solc';
import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet, saveCreatedToken } from '../utils/wallet.js'; // Assuming save for NFT tracking if needed
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';
import { getRandomText } from '../utils/randomText.js';

// Minimal ERC721 Source (Flattened for simplicity)
const CONTRACT_SOURCE = `
pragma solidity ^0.8.20;

contract MinimalNFT {
    string public name;
    string public symbol;
    uint256 public nextTokenId;
    mapping(uint256 => address) public owners;
    mapping(address => uint256) public balances;

    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);

    constructor(string memory _name, string memory _symbol) {
        name = _name;
        symbol = _symbol;
    }

    function mint(address to) external {
        uint256 tokenId = nextTokenId++;
        owners[tokenId] = to;
        balances[to]++;
        emit Transfer(address(0), to, tokenId);
    }
    
    function balanceOf(address owner) external view returns (uint256) {
        return balances[owner];
    }

    function ownerOf(uint256 tokenId) external view returns (address) {
        return owners[tokenId];
    }
}
`;

// Compiler (Reusing logic from 1_deployContract)
let compiledCache = null;

function compileNFTContract(silent = false) {
    if (compiledCache) return compiledCache;
    if (!silent) console.log(`${COLORS.fg.cyan}Compiling NFT contract...${COLORS.reset}`);

    const input = {
        language: 'Solidity',
        sources: { 'MinimalNFT.sol': { content: CONTRACT_SOURCE } },
        settings: {
            optimizer: { enabled: true, runs: 200 },
            outputSelection: { '*': { '*': ['abi', 'evm.bytecode'] } }
        }
    };

    try {
        const output = JSON.parse(solc.compile(JSON.stringify(input)));
        if (output.errors) {
            const errors = output.errors.filter(e => e.severity === 'error');
            if (errors.length > 0) throw new Error(errors[0].formattedMessage);
        }
        const contract = output.contracts['MinimalNFT.sol']['MinimalNFT'];
        if (!silent) console.log(`${COLORS.fg.green}✓ NFT Contract compiled!${COLORS.reset}\n`);

        compiledCache = { abi: contract.abi, bytecode: contract.evm.bytecode.object };
        return compiledCache;
    } catch (error) {
        if (!silent) console.error(`${COLORS.fg.red}Compilation failed: ${error.message}${COLORS.reset}`);
        return null;
    }
}

function generateRandomNFTName() {
    const prefixes = ['Space', 'Cyber', 'Pixel', 'Crypto', 'Meta', 'Golden', 'Mystic'];
    const suffixes = ['Punk', 'Ape', 'Cat', 'Dog', 'Robot', 'Dragon', 'Ghost'];
    return `${prefixes[getRandomInt(0, prefixes.length - 1)]}${suffixes[getRandomInt(0, suffixes.length - 1)]}`;
}

export async function createMintRandomNFTForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const nftName = generateRandomNFTName();
    const nftSymbol = nftName.replace(/[a-z]/g, '').substring(0, 4);

    if (!silent) console.log(`${COLORS.fg.yellow}Creating NFT Collection: ${nftName} (${nftSymbol})...${COLORS.reset}`);

    try {
        // 1. Compile
        const artifact = compileNFTContract(silent);
        if (!artifact) throw new Error("Compilation failed");

        // 2. Deploy
        const factory = new ethers.ContractFactory(artifact.abi, artifact.bytecode, wallet);
        const randomGas = getRandomInt(2500000, CONFIG.GAS_LIMIT);

        // Use 3x gas multiplier for speed
        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);

        const contract = await factory.deploy(nftName, nftSymbol, {
            gasLimit: randomGas,
            ...gasOverrides
        });
        if (!silent) console.log(`${COLORS.dim}Deploy Tx: ${CONFIG.EXPLORER_URL}/tx/${contract.deploymentTransaction().hash}${COLORS.reset}`);

        await contract.waitForDeployment();
        const contractAddress = await contract.getAddress();

        if (!silent) console.log(`${COLORS.fg.green}✓ Deployed at: ${CONFIG.EXPLORER_URL}/address/${contractAddress}${COLORS.reset}`);

        // 3. Mint
        if (!silent) console.log(`${COLORS.dim}Minting NFT...${COLORS.reset}`);

        // Retry logic handled by helper
        const txCreator = async () => {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            return contract.mint(wallet.address, {
                gasLimit: 200000,
                ...gasOverrides
            });
        };

        const { hash, receipt } = await sendTxWithRetry(wallet, txCreator);

        if (!silent) console.log(`${COLORS.fg.green}✓ Minted 1 NFT to self${COLORS.reset}`);

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'CreateMintNFT', 'success', `${nftSymbol}: ${contractAddress}`, silent, duration);

        return { success: true, txHash: hash, contractAddress, name: nftName, symbol: nftSymbol, block: receipt.blockNumber };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'CreateMintNFT', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ NFT Task failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function runNFTCreateMintMenu() {
    console.log(`\n  ${COLORS.fg.magenta}🎨  NFT CREATE & MINT MODULE${COLORS.reset}\n`);

    const privateKeys = getPrivateKeys();
    console.log(`${COLORS.fg.cyan}Found ${privateKeys.length} wallet(s)${COLORS.reset}\n`);

    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}/${privateKeys.length}${COLORS.reset}`);
        await createMintRandomNFTForWallet(wallet, proxy, 1, i);
        if (i < privateKeys.length - 1) await countdown(3, 'Next wallet');
    }
    console.log(`\n${COLORS.fg.green}✓ All NFT tasks completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}
