import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';

const SYSTEM_TOKEN_ABI = [
    "function transferLater(address to, uint256 amount, uint256 validAfter) returns (bool)",
    "function decimals() view returns (uint8)"
];

async function main() {
    process.env.WALLET_PASSWORD = "password";
    console.log(`Debug TransferLater Params...`);

    const privateKeys = getPrivateKeys();
    const { wallet } = await getWallet(0, privateKeys[0]);

    const tokenAddress = CONFIG.TOKENS.PathUSD;
    const token = new ethers.Contract(tokenAddress, SYSTEM_TOKEN_ABI, wallet);
    const to = ethers.Wallet.createRandom().address;
    const amount = ethers.parseUnits("1.0", 6); // PathUSD usually 18? check decimals

    let decimals = 18;
    try { decimals = await token.decimals(); } catch (e) { }
    const amountWei = ethers.parseUnits("1.0", decimals);

    console.log(`Decimal: ${decimals}, Amount: ${amountWei}`);

    // TEST 1: TIMESTAMP
    const timestamp = Math.floor(Date.now() / 1000) + 300; // 5 mins future
    console.log(`Test 1: Using Timestamp ${timestamp}`);
    try {
        await token.transferLater.estimateGas(to, amountWei, timestamp);
        console.log("SUCCESS with TIMESTAMP!");
    } catch (e) {
        console.log("Failed with Timestamp:", e.shortMessage || e.message);
    }

    // TEST 2: DELAY
    const delay = 300;
    console.log(`Test 2: Using Delay ${delay}`);
    try {
        await token.transferLater.estimateGas(to, amountWei, delay);
        console.log("SUCCESS with DELAY!");
    } catch (e) {
        console.log("Failed with Delay:", e.shortMessage || e.message);
    }
}

main().catch(console.error);
