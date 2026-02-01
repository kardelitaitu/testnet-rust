import { ethers } from 'ethers';
import { getWalletFiles, getPrivateKeyFromFile, getWallet } from '../utils/wallet.js';
import { deployViralNFTForWallet } from './47_deployViralNFT.js';
import { askPassword } from '../utils/helpers.js';
import { COLORS } from '../utils/constants.js';

async function main() {
    console.log(`  ${COLORS.fg.magenta}🐛  DEBUG MODE: Task 47 - Deploy Viral NFT${COLORS.reset}\n`);

    // 1. Get Random Wallet File
    const walletFiles = getWalletFiles();
    if (walletFiles.length === 0) {
        console.error("No wallet files found in wallets/ directory!");
        return;
    }

    const randomIndex = Math.floor(Math.random() * walletFiles.length);
    const selectedFile = walletFiles[randomIndex];
    let password = process.env.WALLET_PASSWORD || "password";
    let privateKey;

    console.log(`${COLORS.fg.cyan}Selected Random Wallet: ${selectedFile} (Index ${randomIndex + 1}/${walletFiles.length})${COLORS.reset}`);

    // Decrypt ONLY this wallet
    try {
        privateKey = getPrivateKeyFromFile(selectedFile, password);
    } catch (e) {
        console.log(`${COLORS.dim}Default password failed. Asking...${COLORS.reset}`);
        password = await askPassword("Enter encryption password: ");
        privateKey = getPrivateKeyFromFile(selectedFile, password);
    }

    if (!privateKey) throw new Error("Failed to decrypt private key");

    const { wallet, proxy } = await getWallet(0, privateKey);

    console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
    console.log(`${COLORS.fg.magenta}WALLET: ${wallet.address}${COLORS.reset}`);
    if (proxy) console.log(`${COLORS.dim}Proxy: ${proxy}${COLORS.reset}`);

    // 2. Execute Deployment
    const result = await deployViralNFTForWallet(wallet, proxy, 'DEBUG', 0, false);

    if (result.success && result.contractAddress) {
        console.log(`${COLORS.fg.green}✓ Deployed ViralNFT at: ${result.contractAddress}${COLORS.reset}`);

        // Verify Contract Holding (Pre-Mint Check)
        try {
            const provider = wallet.provider;
            const nftContract = new ethers.Contract(result.contractAddress, ["function balanceOf(address) view returns (uint256)"], provider);
            const balance = await nftContract.balanceOf(result.contractAddress);
            console.log(`${COLORS.fg.cyan}Inventory Check: Contract holds ${balance.toString()} NFTs.${COLORS.reset}`);
        } catch (e) {
            console.log(`${COLORS.fg.red}⚠ Could not verify inventory: ${e.message}${COLORS.reset}`);
        }
    }

    console.log(`\n${COLORS.fg.green}✓ Debug task completed.${COLORS.reset}`);
}

main().catch(console.error);
