import { ethers } from 'ethers';
import { CONFIG } from '../utils/constants.js';
import { getPrivateKeys, getWallet, loadCreatedTokens } from '../utils/wallet.js';

const ERC20_ABI = [
    "function balanceOf(address owner) view returns (uint256)",
    "function totalSupply() view returns (uint256)",
    "function decimals() view returns (uint8)",
    "function symbol() view returns (string)",
    "function hasRole(bytes32 role, address account) view returns (bool)"
];

const ISSUER_ROLE = ethers.id("ISSUER_ROLE");

async function main() {
    console.log('Checking token balances and supplies...\n');

    const privateKeys = getPrivateKeys();
    if (privateKeys.length === 0) {
        console.log('No private keys found');
        return;
    }

    const createdTokens = loadCreatedTokens();
    console.log(`Found ${createdTokens.length} created tokens total\n`);

    // Check first few tokens
    for (let i = 0; i < Math.min(3, createdTokens.length); i++) {
        const tokenInfo = createdTokens[i];
        console.log(`Token ${i + 1}: ${tokenInfo.symbol} (${tokenInfo.token})`);
        console.log(`  Owner: ${tokenInfo.wallet}`);

        try {
            // Find the wallet index
            const walletAddress = tokenInfo.wallet.toLowerCase();
            let walletIndex = -1;
            for (let j = 0; j < privateKeys.length; j++) {
                const { wallet } = await getWallet(j, privateKeys[j]);
                if (wallet.address.toLowerCase() === walletAddress) {
                    walletIndex = j;
                    break;
                }
            }

            if (walletIndex === -1) {
                console.log('  Wallet not in pv.txt, skipping\n');
                continue;
            }

            const { wallet } = await getWallet(walletIndex, privateKeys[walletIndex]);
            const token = new ethers.Contract(tokenInfo.token, ERC20_ABI, wallet);

            const decimals = await token.decimals();
            const balance = await token.balanceOf(wallet.address);
            const totalSupply = await token.totalSupply();
            const hasRole = await token.hasRole(ISSUER_ROLE, wallet.address);

            console.log(`  Decimals: ${decimals}`);
            console.log(`  Balance: ${ethers.formatUnits(balance, decimals)} (${balance.toString()})`);
            console.log(`  Total Supply: ${ethers.formatUnits(totalSupply, decimals)} (${totalSupply.toString()})`);
            console.log(`  Has ISSUER_ROLE: ${hasRole}`);

            // Calculate what 1000 + current balance would be
            const mintAmount = ethers.parseUnits('1000', decimals);
            const afterMint = balance + mintAmount;
            console.log(`  After minting 1000: ${ethers.formatUnits(afterMint, decimals)}`);

            // Check if error param matches anything
            const errorParam = 2446411860n;
            console.log(`  Error param (2446.41 tokens) matches: ${errorParam === balance ? 'BALANCE' : errorParam === totalSupply ? 'TOTAL_SUPPLY' : errorParam === afterMint ? 'AFTER_MINT' : 'NONE'}`);

        } catch (error) {
            console.log(`  Error: ${error.message}`);
        }

        console.log('');
    }
}

main().catch(console.error);
