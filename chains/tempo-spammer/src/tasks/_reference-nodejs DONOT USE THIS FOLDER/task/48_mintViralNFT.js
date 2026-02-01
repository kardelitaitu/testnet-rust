import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { logWalletAction } from '../utils/logger.js';
import { getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const TRACKER_FILE = path.join(ROOT_DIR, 'data', 'viral_nfts.json');

const NFT_ABI = [
    "function claim() public",
    "function balanceOf(address owner) view returns (uint256)",
    "function ownerOf(uint256 tokenId) view returns (address)"
];

export async function mintViralNFTForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    // 1. Load NFT List
    if (!fs.existsSync(TRACKER_FILE)) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ No viral NFTs found. Run Task 47 first.${COLORS.reset}`);
        return { success: false, reason: 'no_viral_nfts_found' };
    }

    let nftList = [];
    try {
        nftList = JSON.parse(fs.readFileSync(TRACKER_FILE, 'utf8'));
    } catch (e) { }

    if (nftList.length === 0) {
        if (!silent) console.log(`${COLORS.fg.yellow}⚠ Viral NFT list is empty.${COLORS.reset}`);
        return { success: false, reason: 'empty_nft_list' };
    }

    // 2. Select Random NFT Contract
    const target = nftList[Math.floor(Math.random() * nftList.length)];
    const contractAddress = target.address;
    const symbol = target.symbol || 'VNFT'; // Fallback for old records

    const startTime = Date.now();
    if (!silent) console.log(`${COLORS.fg.cyan}Claiming Viral NFT (${symbol}) from ${contractAddress}...${COLORS.reset}`);

    const contract = new ethers.Contract(contractAddress, NFT_ABI, wallet);

    try {
        // 3. Check if already claimed
        // Contract enforces 1 per wallet, so check balance first to save gas
        try {
            const bal = await contract.balanceOf(wallet.address);
            // Debug Log
            if (!silent) console.log(`Debug: Balance of ${wallet.address} is ${bal.toString()}`);

            if (bal > 0) {
                const duration = (Date.now() - startTime) / 1000;
                logWalletAction(workerId, walletIndex, wallet.address, 'ClaimViralNFT', 'success', `Already Claimed ${symbol} (Bal: ${bal})`, silent, duration, proxy);
                if (!silent) console.log(`${COLORS.fg.green}✓ Already claimed NFT from ${contractAddress}. Skipping (Bal: ${bal}).${COLORS.reset}`);
                return { success: true, reason: 'already_claimed', contract: contractAddress, amount: 0, balance: bal.toString() };
            }
        } catch (e) {
            if (!silent) console.log(`Warning: Could not check balance: ${e.message}`);
        }

        // 4. Claim (Single Item)
        const txCreator = async () => {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            return contract.claim({
                gasLimit: 300000,
                ...gasOverrides
            });
        };
        const amount = 1;

        const { hash, receipt } = await sendTxWithRetry(wallet, txCreator);

        // Verify Balance
        let newBalance = 'unknown';
        try {
            const bal = await contract.balanceOf(wallet.address);
            newBalance = bal.toString();
        } catch (e) { }

        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'ClaimViralNFT', 'success', `${amount} ${symbol} (Bal: ${newBalance})`, silent, duration, proxy);
        if (!silent) console.log(`${COLORS.fg.green}✓ Claimed ${amount} ${symbol}! Block: ${receipt.blockNumber} | New Balance: ${newBalance}${COLORS.reset}`);

        return { success: true, txHash: hash, contract: contractAddress, amount, balance: newBalance };

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'ClaimViralNFT', 'failed', error.message.substring(0, 50), silent, duration, proxy);
        if (!silent) console.log(`${COLORS.fg.red}Claim failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}
