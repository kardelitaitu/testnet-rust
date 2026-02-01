import { ethers } from 'ethers';
import { CONFIG } from '../utils/constants.js';
import { getPrivateKeys, getWallet, loadCreatedTokens } from '../utils/wallet.js';

const MINT_ABI = [
    "function mint(address to, uint256 amount)",
    "function hasRole(bytes32 role, address account) view returns (bool)",
    "function decimals() view returns (uint8)",
    "function symbol() view returns (string)",
    "function balanceOf(address owner) view returns (uint256)",
    "function totalSupply() view returns (uint256)"
];

const ISSUER_ROLE = ethers.id("ISSUER_ROLE");

async function main() {
    console.log('Testing LAST created token...\n');

    const privateKeys = getPrivateKeys();
    const createdTokens = loadCreatedTokens();

    if (createdTokens.length === 0) {
        console.log('No tokens found!');
        return;
    }

    // Get the LAST created token (most recent)
    const tokenInfo = createdTokens[createdTokens.length - 1];
    console.log(`Token: ${tokenInfo.symbol} (${tokenInfo.token})`);
    console.log(`Owner: ${tokenInfo.wallet}\n`);

    // Find wallet
    let walletIndex = -1;
    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet } = await getWallet(i, privateKeys[i]);
        if (wallet.address.toLowerCase() === tokenInfo.wallet.toLowerCase()) {
            walletIndex = i;
            break;
        }
    }

    if (walletIndex === -1) {
        console.log('Wallet not found!');
        return;
    }

    const { wallet } = await getWallet(walletIndex, privateKeys[walletIndex]);
    console.log(`Using wallet: ${wallet.address}\n`);

    try {
        const token = new ethers.Contract(tokenInfo.token, MINT_ABI, wallet);

        console.log('1. Checking decimals...');
        const decimals = await token.decimals();
        console.log(`   ✓ Decimals: ${decimals}\n`);

        console.log('2. Checking hasRole...');
        const hasRole = await token.hasRole(ISSUER_ROLE, wallet.address);
        console.log(`   ✓ Has ISSUER_ROLE: ${hasRole}\n`);

        console.log('3. Checking balanceOf...');
        const balance = await token.balanceOf(wallet.address);
        console.log(`   ✓ Balance: ${ethers.formatUnits(balance, decimals)}\n`);

        console.log('4. Checking totalSupply...');
        const totalSupply = await token.totalSupply();
        console.log(`   ✓ Total Supply: ${ethers.formatUnits(totalSupply, decimals)}\n`);

        console.log('5. Attempting to mint 1000 tokens...');
        const amount = 1000;
        const amountWei = ethers.parseUnits(amount.toString(), decimals);
        console.log(`   Amount Wei: ${amountWei.toString()}`);

        const tx = await token.mint(wallet.address, amountWei, { gasLimit: 1000000 });
        console.log(`   Tx sent: ${tx.hash}`);
        await tx.wait();
        console.log(`   ✓ MINT SUCCESSFUL!\n`);

        const balanceAfter = await token.balanceOf(wallet.address);
        console.log(`   Balance after: ${ethers.formatUnits(balanceAfter, decimals)}`);

    } catch (error) {
        console.log(`\n❌ ERROR: ${error.message}`);
        if (error.data) {
            console.log(`   Error Data: ${error.data}`);
        }
    }
}

main().catch(console.error);
