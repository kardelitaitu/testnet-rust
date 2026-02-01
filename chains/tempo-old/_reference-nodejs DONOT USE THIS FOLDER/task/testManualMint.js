import { ethers } from 'ethers';
import { CONFIG } from '../utils/constants.js';
import { getPrivateKeys, getWallet, loadCreatedTokens } from '../utils/wallet.js';

// Use web3.py-style manual transaction building
async function manualMint() {
    const privateKeys = getPrivateKeys();
    const createdTokens = loadCreatedTokens();

    if (createdTokens.length === 0) {
        console.log('No tokens found!');
        return;
    }

    // Get last token
    const tokenInfo = createdTokens[createdTokens.length - 1];
    console.log(`\nTesting: ${tokenInfo.symbol} (${tokenInfo.token})`);
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

    const MINT_ABI = [
        "function mint(address to, uint256 amount)",
        "function hasRole(bytes32 role, address account) view returns (bool)",
        "function decimals() view returns (uint8)",
        "function balanceOf(address owner) view returns (uint256)"
    ];

    const ISSUER_ROLE = ethers.id("ISSUER_ROLE");

    try {
        const token = new ethers.Contract(tokenInfo.token, MINT_ABI, wallet);

        console.log('Step 1: Check ISSUER_ROLE...');
        const hasRole = await token.hasRole(ISSUER_ROLE, wallet.address);
        console.log(`  Has Role: ${hasRole}\n`);

        console.log('Step 2: Get decimals...');
        const decimals = await token.decimals();
        console.log(`  Decimals: ${decimals}\n`);

        console.log('Step 3: Check balance...');
        const balBefore = await token.balanceOf(wallet.address);
        console.log(`  Balance: ${ethers.formatUnits(balBefore, decimals)}\n`);

        console.log('Step 4: Prepare mint transaction (Python-style)...');
        const amount = 1;
        const amountWei = ethers.parseUnits(amount.toString(), decimals);
        console.log(`  Amount Wei: ${amountWei.toString()}\n`);

        console.log('Step 5: Build transaction manually...');
        const nonce = await wallet.provider.getTransactionCount(wallet.address);
        const feeData = await wallet.provider.getFeeData();

        // Build transaction like Python does
        const mintTx = await token.mint.populateTransaction(wallet.address, amountWei);
        mintTx.from = wallet.address;
        mintTx.nonce = nonce;
        mintTx.gasLimit = 200000; // Match Python
        mintTx.gasPrice = feeData.gasPrice;
        mintTx.chainId = CONFIG.CHAIN_ID;

        console.log('  Transaction built:');
        console.log(`    - Nonce: ${nonce}`);
        console.log(`    - Gas: ${mintTx.gasLimit}`);
        console.log(`    - Gas Price: ${mintTx.gasPrice}`);
        console.log('');

        console.log('Step 6: Sign and send transaction...');
        const signedTx = await wallet.signTransaction(mintTx);
        const txResponse = await wallet.provider.broadcastTransaction(signedTx);
        console.log(`  TX: ${txResponse.hash}\n`);

        console.log('Step 7: Wait for confirmation...');
        const receipt = await txResponse.wait();
        console.log(`  ✓ Confirmed in block ${receipt.blockNumber}\n`);

        console.log('Step 8: Check balance after...');
        const balAfter = await token.balanceOf(wallet.address);
        console.log(`  Balance: ${ethers.formatUnits(balAfter, decimals)}`);
        console.log(`  Change: +${ethers.formatUnits(balAfter - balBefore, decimals)}\n`);

        console.log('🎉 SUCCESS!');

    } catch (error) {
        console.log(`\n❌ ERROR at: ${error.message}`);
        if (error.data) {
            console.log(`   Error Data: ${error.data}`);
        }
        console.log(error);
    }
}

manualMint().catch(console.error);
