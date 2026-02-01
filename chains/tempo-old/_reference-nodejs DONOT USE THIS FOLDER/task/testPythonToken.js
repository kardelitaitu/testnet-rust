import { ethers } from 'ethers';
import { CONFIG } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';

// Test minting to a PYTHON-CREATED token
// Replace these with actual values from Python's working tokens
async function testPythonToken() {
    console.log('\n🧪 Testing Node.js mint on Python-created token...\n');

    const privateKeys = getPrivateKeys();

    // You need to provide:
    // 1. A token address created by Python (from the successful mint)
    // 2. The wallet index that owns it

    const PYTHON_TOKEN_ADDRESS = '0x20C0000000000000000000000000000000000000'; // REPLACE THIS
    const WALLET_INDEX = 0; // REPLACE with the wallet index from Python

    console.log(`Token Address: ${PYTHON_TOKEN_ADDRESS}`);
    console.log(`Wallet Index: ${WALLET_INDEX}\n`);

    const { wallet } = await getWallet(WALLET_INDEX, privateKeys[WALLET_INDEX]);
    console.log(`Wallet: ${wallet.address}\n`);

    const MINT_ABI = [
        "function mint(address to, uint256 amount)",
        "function hasRole(bytes32 role, address account) view returns (bool)",
        "function grantRole(bytes32 role, address account)",
        "function decimals() view returns (uint8)",
        "function symbol() view returns (string)",
        "function balanceOf(address owner) view returns (uint256)"
    ];

    const ISSUER_ROLE = ethers.id("ISSUER_ROLE");

    try {
        const token = new ethers.Contract(PYTHON_TOKEN_ADDRESS, MINT_ABI, wallet);

        console.log('Step 1: Get symbol...');
        const symbol = await token.symbol();
        console.log(`  Symbol: ${symbol}\n`);

        console.log('Step 2: Get decimals...');
        const decimals = await token.decimals();
        console.log(`  Decimals: ${decimals}\n`);

        console.log('Step 3: Check ISSUER_ROLE...');
        const hasRole = await token.hasRole(ISSUER_ROLE, wallet.address);
        console.log(`  Has Role: ${hasRole}\n`);

        if (!hasRole) {
            console.log('Step 4: Grant ISSUER_ROLE...');
            const txGrant = await token.grantRole(ISSUER_ROLE, wallet.address, { gasLimit: 300000 });
            await txGrant.wait();
            console.log(`  ✓ Role granted\n`);

            // Wait for propagation
            await new Promise(r => setTimeout(r, 3000));
        }

        console.log('Step 5: Check balance before...');
        const balBefore = await token.balanceOf(wallet.address);
        console.log(`  Balance: ${ethers.formatUnits(balBefore, decimals)}\n`);

        console.log('Step 6: Mint 1000 tokens...');
        const amount = 1000;
        const amountWei = ethers.parseUnits(amount.toString(), decimals);
        const tx = await token.mint(wallet.address, amountWei, { gasLimit: 1000000 });
        console.log(`  TX: ${tx.hash}`);
        await tx.wait();
        console.log(`  ✓ Confirmed\n`);

        console.log('Step 7: Check balance after...');
        const balAfter = await token.balanceOf(wallet.address);
        console.log(`  Balance: ${ethers.formatUnits(balAfter, decimals)}`);
        console.log(`  Change: +${ethers.formatUnits(balAfter - balBefore, decimals)}\n`);

        console.log('🎉 SUCCESS! Node.js CAN mint to Python-created tokens!');
        console.log('    → The bug is ONLY in our token creation process\n');

    } catch (error) {
        console.log(`\n❌ ERROR: ${error.message}`);
        if (error.data) {
            console.log(`   Error Data: ${error.data}`);
        }
    }
}

testPythonToken().catch(console.error);

console.log(`
╔════════════════════════════════════════════════════════════════╗
║  INSTRUCTIONS:                                                 ║
║  1. Run Python: python python-reference/main.py                ║
║  2. Choose option 8 (Mint Token)                               ║
║  3. Look for a token address in the output (e.g., 0x20c0...)   ║
║  4. Find the wallet index for that token                       ║
║  5. Edit this script with those values                         ║
║  6. Run: node task/testPythonToken.js                          ║
╚════════════════════════════════════════════════════════════════╝
`);
