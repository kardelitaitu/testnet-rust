import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, getRandomInt } from '../utils/helpers.js';
import { ConcurrentService, loadNonceKey, saveNonceKey } from '../utils/tempoConcurrent.js';
import { TempoInspector } from '../utils/tempoInspector.js';

// Minimal Contract Bytecode (STOP opcode)
const MINIMAL_BYTECODE = "0x60008060093d393df3";

export async function startTimeBomb(wallet, proxy, workerId = 1, walletIndex = 0, silent = false, waitForDetonation = false) {
    const startTime = Date.now();
    const BOMB_DELAY_SEC = getRandomInt(20, 30); // Random delay 20-30s
    const BOMB_SIZE = getRandomInt(2, 5);

    // Calculate Detonation Time
    const now = Math.floor(Date.now() / 1000);
    const detonationTime = now + BOMB_DELAY_SEC;

    if (!silent) {
        console.log(`${COLORS.fg.red}💣 TIME BOMB: Scheduling ${BOMB_SIZE} Deploys for T+${BOMB_DELAY_SEC}s${COLORS.reset}`);
        console.log(`${COLORS.dim}   Target Time: ${new Date(detonationTime * 1000).toLocaleTimeString()}${COLORS.reset}`);
    }

    try {
        const service = new ConcurrentService(wallet.privateKey, proxy);
        // 2. Load Identity & Nonce 
        // Use Timestamp to guarantee uniqueness and avoid "nonce too low" errors
        // because we are starting a fresh batch of parallel keys.
        let currentNonceKeyBase = Date.now();
        if (!silent) console.log(`${COLORS.dim}   Using NonceKey Base: ${currentNonceKeyBase}${COLORS.reset}`);

        const promises = [];
        const hashes = [];

        // 1. Plant Bombs (Broadcast with validAfter)
        for (let i = 0; i < BOMB_SIZE; i++) {
            const nonceKey = currentNonceKeyBase + i;

            const p = service.sendConcurrentTransaction({
                to: null,
                data: MINIMAL_BYTECODE,
                nonce: 0,
                nonceKey: nonceKey,
                value: 0n,
                validAfter: detonationTime // The Fuse
            }).then(hash => {
                hashes.push(hash);
                if (!silent) process.stdout.write(`${COLORS.fg.yellow}*${COLORS.reset}`);
                return { success: true, hash };
            }).catch(err => {
                if (!silent) console.log(`\n${COLORS.fg.red}Arm Failed: ${err.message}${COLORS.reset}`);
                return { success: false, error: err.message };
            });

            promises.push(p);
        }

        if (!silent) console.log(`\n${COLORS.dim}   Arming...${COLORS.reset}`);
        await Promise.all(promises);

        saveNonceKey(wallet.address, currentNonceKeyBase + BOMB_SIZE);

        if (hashes.length === 0) {
            throw new Error("Failed to arm any transactions");
        }

        // Return immediately if not waiting
        if (!waitForDetonation) {
            const duration = (Date.now() - startTime) / 1000;
            if (!silent) console.log(`\n${COLORS.fg.cyan}✓ Armed ${hashes.length}/${BOMB_SIZE} transactions! (Fire & Forget)${COLORS.reset}`);
            logWalletAction(workerId, walletIndex, wallet.address, 'TimeBomb', 'success', `Armed ${hashes.length} Tx`, silent, duration);
            return { success: true, armed: hashes.length };
        }

        // 2. Wait for Detonation
        if (!silent) {
            console.log(`\n${COLORS.fg.cyan}✓ Armed ${hashes.length}/${BOMB_SIZE} transactions!${COLORS.reset}`);
            console.log(`${COLORS.dim}   Waiting for detonation...${COLORS.reset}`);
        }

        await countdown(BOMB_DELAY_SEC + 5, "Detonation in");

        // 3. Verify Damage
        if (!silent) console.log(`${COLORS.dim}   Checking impact...${COLORS.reset}`);

        // We check the last hash to see if it confirmed
        const lastHash = hashes[hashes.length - 1];
        let confirmed = false;
        try {
            // Inspector will fetch receipt using correct provider
            const report = await TempoInspector.logReport(lastHash, { silent: !silent, proxy }); // Log if not silent
            if (report) confirmed = true;
        } catch (e) { }

        const logStatus = confirmed ? 'success' : 'warning';
        const finalDuration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'TimeBomb', logStatus, `${hashes.length} Tx @ ${new Date(detonationTime * 1000).toLocaleTimeString()}`, silent, finalDuration);

        return { success: confirmed, count: hashes.length, time: detonationTime, txHash: lastHash };

    } catch (e) {
        logWalletAction(workerId, walletIndex, wallet.address, 'TimeBomb', 'failed', e.message, silent, 0);
        return { success: false, reason: e.message };
    }
}

export async function runTimeBombMenu() {
    console.log(`\n  ${COLORS.fg.red}💣 TEMPO TIME BOMB (SCHEDULER TEST)${COLORS.reset}\n`);
    const privateKeys = getPrivateKeys();
    // Just run first wallet for demo, or loop? Loop is chaos. Let's do first 3.
    const limit = Math.min(privateKeys.length, 3);

    for (let i = 0; i < limit; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);
        await startTimeBomb(wallet, proxy, 1, i);
    }
}
