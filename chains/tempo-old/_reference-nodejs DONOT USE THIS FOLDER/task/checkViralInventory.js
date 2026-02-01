import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const TRACKER_FILE = path.join(ROOT_DIR, 'data', 'viral_nfts.json');

const NFT_ABI = [
    "function name() view returns (string)",
    "function symbol() view returns (string)",
    "function balanceOf(address owner) view returns (uint256)",
    "function totalSupply() view returns (uint256)"
];

async function main() {
    console.log(`\n${COLORS.fg.magenta}🔍 Viral NFT Inventory Checker${COLORS.reset}\n`);

    if (!fs.existsSync(TRACKER_FILE)) {
        console.log(`${COLORS.fg.red}No tracker file found at ${TRACKER_FILE}${COLORS.reset}`);
        return;
    }

    const nftList = JSON.parse(fs.readFileSync(TRACKER_FILE, 'utf8'));
    if (nftList.length === 0) {
        console.log(`${COLORS.fg.yellow}Tracker file is empty.${COLORS.reset}`);
        return;
    }

    // Use static provider for reading (No Wallet Needed)
    const provider = new ethers.JsonRpcProvider(CONFIG.RPC_URL, {
        chainId: CONFIG.CHAIN_ID,
        name: 'tempo-testnet'
    });

    console.log(`${COLORS.fg.cyan}Scanning ${nftList.length} contracts...${COLORS.reset}\n`);
    console.log(`| Address                                    | Name           | Symbol | Inventory | Claimed |`);
    console.log(`|--------------------------------------------|----------------|--------|-----------|---------|`);

    for (const nft of nftList) {
        try {
            const contract = new ethers.Contract(nft.address, NFT_ABI, provider);

            // Parallel fetch for speed
            const [name, symbol, balance, total] = await Promise.all([
                contract.name().catch(() => nft.name || 'Unknown'),
                contract.symbol().catch(() => nft.symbol || '???'),
                contract.balanceOf(nft.address).catch(() => 0), // Inventory held by contract
                contract.totalSupply().catch(() => 100) // Assuming 100 if fail
            ]);

            const inventory = parseInt(balance);
            const claimed = parseInt(total) - inventory;

            // Simple padding for table alignment
            const addrStr = nft.address.padEnd(42);
            const nameStr = name.substring(0, 14).padEnd(14);
            const symStr = symbol.substring(0, 6).padEnd(6);
            const invStr = inventory.toString().padEnd(9);
            const claimStr = claimed.toString().padEnd(7);

            const color = inventory > 0 ? COLORS.fg.green : COLORS.fg.red;

            console.log(`${color}| ${addrStr} | ${nameStr} | ${symStr} | ${invStr} | ${claimStr} |${COLORS.reset}`);

        } catch (e) {
            console.log(`${COLORS.fg.red}| ${nft.address.padEnd(42)} | ERROR          | ???    | ???       | ???     |${COLORS.reset}`);
        }
    }
    console.log('\nDone.');
}

main().catch(console.error);
