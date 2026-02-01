import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getWalletFiles, getPrivateKeyFromFile, getWallet } from '../utils/wallet.js';
import { getGasWithMultiplier, askPassword } from '../utils/helpers.js';
import { TempoInspector } from '../utils/tempoInspector.js';

const RETRIEVER_NFT_ABI = [
    "function claim(address receiver, uint256 quantity, address currency, uint256 pricePerToken, tuple(bytes32[] proof, uint256 quantityLimitPerWallet, uint256 pricePerToken, address currency) allowlistProof, bytes data)",
    "function balanceOf(address owner) view returns (uint256)",
    "function name() view returns (string)"
];

async function main() {
    console.log(`  ${COLORS.fg.magenta}🐛  DEBUG MODE: Task 16 - Retrieve NFT${COLORS.reset}\n`);

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
    const retrieverAddress = SYSTEM_CONTRACTS.RETRIEVER_NFT_CONTRACT;

    console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
    console.log(`${COLORS.fg.magenta}WALLET: ${wallet.address}${COLORS.reset}`);
    if (proxy) console.log(`${COLORS.dim}Proxy: ${proxy}${COLORS.reset}`);
    console.log(`Contract: ${retrieverAddress}`);

    const provider = wallet.provider;
    const code = await provider.getCode(retrieverAddress);
    console.log(`Code at address: ${code === '0x' ? 'EMPTY (Contract not deployed)' : 'PRESENT'}`);

    if (code === '0x') return;

    const nftContract = new ethers.Contract(retrieverAddress, RETRIEVER_NFT_ABI, wallet);

    // 1. Test Name
    try {
        console.log("Calling name()...");
        const name = await nftContract.name();
        console.log(`Name: ${name}`);
    } catch (e) {
        console.log("Name failed:", e.message);
    }

    // 2. Test BalanceOf
    try {
        console.log("Calling balanceOf()...");
        const bal = await nftContract.balanceOf(wallet.address);
        console.log(`Balance: ${bal.toString()}`);
    } catch (e) {
        console.log("BalanceOf failed:", e.message);
        if (e.data) console.log("Error Data:", e.data);
    }

    // 3. Test Claim
    console.log("Attempting claim...");
    const allowlistProof = {
        proof: [],
        quantityLimitPerWallet: ethers.MaxUint256,
        pricePerToken: 0,
        currency: '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE'
    };

    try {
        // Estimate gas first to see if it reverts
        try {
            console.log("Estimating gas...");
            const gas = await nftContract.claim.estimateGas(
                wallet.address,
                1,
                '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE',
                0,
                allowlistProof,
                '0x'
            );
            console.log(`Estimated Gas: ${gas.toString()}`);
        } catch (e) {
            console.log("Gas Estimation Reverted:", e.shortMessage || e.message);
        }

        const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
        console.log("Sending Transaction...");
        const tx = await nftContract.claim(
            wallet.address,
            1, // quantity
            '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE', // currency (native)
            0, // price
            allowlistProof,
            '0x', // data
            {
                gasLimit: 500000, // Explicit limit
                ...gasOverrides
            }
        );
        console.log(`Tx sent: ${tx.hash}`);
        const receipt = await tx.wait();
        console.log(`Receipt status: ${receipt.status}`);
        if (receipt.status === 1) {
            await TempoInspector.logReport(tx.hash, { proxy });
        }

    } catch (e) {
        console.log("CLAIM FAILED DETAILED:");
        console.log(e);
    }
}

main().catch(console.error);
