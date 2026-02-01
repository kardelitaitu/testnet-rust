import { ethers } from 'ethers';
import { CONFIG, SYSTEM_CONTRACTS, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, askQuestion, getRandomInt, getGasWithMultiplier, sendTxWithRetry } from '../utils/helpers.js';

const RETRIEVER_NFT_ABI = [
    "function claim(address receiver, uint256 quantity, address currency, uint256 pricePerToken, tuple(bytes32[] proof, uint256 quantityLimitPerWallet, uint256 pricePerToken, address currency) allowlistProof, bytes data)",
    "function balanceOf(address owner) view returns (uint256)",
    "function name() view returns (string)"
];

export async function retrieveRandomNFTForWallet(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const retrieverAddress = SYSTEM_CONTRACTS.RETRIEVER_NFT_CONTRACT;

    if (!retrieverAddress) {
        if (!silent) console.log(`${COLORS.fg.red}RETRIEVER_NFT_CONTRACT address missing${COLORS.reset}`);
        return { success: false, reason: 'retriever_address_missing' };
    }

    if (!silent) console.log(`${COLORS.fg.yellow}Minting Retriever NFT...${COLORS.reset}`);

    try {
        const nftContract = new ethers.Contract(retrieverAddress, RETRIEVER_NFT_ABI, wallet);

        // Check balance before
        let balanceBefore = BigInt(0);
        try {
            balanceBefore = await nftContract.balanceOf(wallet.address);
            if (!silent) console.log(`${COLORS.dim}NFT balance: ${balanceBefore.toString()}${COLORS.reset}`);
        } catch (e) {
            if (!silent) console.log(`${COLORS.dim}Could not check balance${COLORS.reset}`);
        }

        // Get collection name
        try {
            const name = await nftContract.name();
            if (!silent) console.log(`${COLORS.dim}Collection: ${name}${COLORS.reset}`);
        } catch (e) { }

        // Allowlist proof (empty for public mint)
        const allowlistProof = {
            proof: [],
            quantityLimitPerWallet: ethers.MaxUint256,
            pricePerToken: 0,
            currency: '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE'
        };

        if (!silent) console.log(`${COLORS.fg.cyan}Claiming Retriever NFT...${COLORS.reset}`);

        const txCreator = async () => {
            const gasOverrides = await getGasWithMultiplier(wallet.provider, undefined, wallet);
            return nftContract.claim(
                wallet.address,
                1, // quantity
                '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE', // currency (native)
                0, // price
                allowlistProof,
                '0x', // data
                {
                    gasLimit: 300000,
                    ...gasOverrides
                }
            );
        };

        const { hash, receipt } = await sendTxWithRetry(wallet, txCreator);

        const tx = { hash }; // Backward compatibility

        if (receipt && receipt.status === 1) {
            // Check balance after
            const balanceAfter = await nftContract.balanceOf(wallet.address);
            const received = balanceAfter - balanceBefore;

            const duration = (Date.now() - startTime) / 1000;
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'RetrieveNFT', 'success', `+${received} NFT`, silent, duration);
            if (!silent) console.log(`${COLORS.fg.green}✓ NFT claimed! Balance: ${balanceAfter.toString()} (+${received}) Block: ${receipt.blockNumber}${COLORS.reset}`);

            return { success: true, txHash: tx.hash, block: receipt.blockNumber, received: received.toString(), newBalance: balanceAfter.toString() };
        } else {
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'RetrieveNFT', 'failed', 'Transaction reverted', silent, duration);
            if (!silent) console.log(`${COLORS.fg.red}✗ Transaction reverted${COLORS.reset}`);
            return { success: false, reason: 'transaction_reverted' };
        }

    } catch (error) {
        const duration = (Date.now() - startTime) / 1000;
        if (!silent) logWalletAction(workerId, walletIndex, wallet.address, 'RetrieveNFT', 'failed', error.message, silent, duration);
        if (!silent) console.log(`${COLORS.fg.red}✗ Claim failed: ${error.message}${COLORS.reset}`);
        return { success: false, reason: error.message };
    }
}

export async function runRetrieveNFTMenu() {
    console.log(`\n  ${COLORS.fg.magenta}🐕  RETRIEVER NFT MODULE${COLORS.reset}\n`);
    console.log(`${COLORS.fg.yellow}Claim Retriever NFT collection${COLORS.reset}\n`);

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

        await retrieveRandomNFTForWallet(wallet, proxy, 1, i);

        if (i < privateKeys.length - 1) {
            await countdown(getRandomInt(5, 10), 'Next wallet in');
        }
    }

    console.log(`\n${COLORS.fg.green}✓ NFT retrieval completed.${COLORS.reset}\n`);
    await countdown(5, 'Returning to menu');
}
