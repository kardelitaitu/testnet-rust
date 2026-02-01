import { ethers } from 'ethers';
import { CONFIG, COLORS } from '../utils/constants.js';
import { getPrivateKeys, getWallet } from '../utils/wallet.js';
import { logWalletAction } from '../utils/logger.js';
import { sleep, countdown, getRandomInt } from '../utils/helpers.js';
import { ConcurrentService, loadNonceKey, saveNonceKey } from '../utils/tempoConcurrent.js';
import { TempoInspector } from '../utils/tempoInspector.js';

// Minimal Contract Bytecode (STOP opcode = 0x00)
// This is the smallest valid contract possible. It does nothing but exist.
const MINIMAL_BYTECODE = "0x60008060093d393df3";

export async function startDeployStorm(wallet, proxy, workerId = 1, walletIndex = 0, silent = false) {
    const startTime = Date.now();
    const STORM_SIZE = getRandomInt(10, 20); // 10-20 parallel deployments per run

    if (!silent) {
        console.log(`${COLORS.fg.magenta}🌩  DEPLOY STORM: Spawning ${STORM_SIZE} Contracts...${COLORS.reset}`);
    }

    try {
        // 1. Initialize Concurrent Service
        const service = new ConcurrentService(wallet.privateKey, proxy);

        // 2. Load Identity & Nonce (FROM MEMORY / INITIAL DISK)
        // We read from disk ONCE at the start, then increment in memory
        let currentNonceKeyBase = loadNonceKey(wallet.address);

        // To avoid collision with other runs, we jump the key significantly 
        // or usage a timestamp-based offset for the session if needed.
        // For now, we trust the tracker + increment.

        let successCount = 0;
        const promises = [];

        // 3. Prepare Batch (Burst Mode)
        for (let i = 0; i < STORM_SIZE; i++) {
            const nonceKey = currentNonceKeyBase + i;

            // We use the service to construct the raw Transaction
            const p = service.sendConcurrentTransaction({
                to: null, // Deployment
                data: MINIMAL_BYTECODE,
                nonce: 0, // Type 0x76 usually ignores this if nonceKey is unique/fresh
                nonceKey: nonceKey,
                value: 0n
            }).then(hash => {
                if (!silent) process.stdout.write(`${COLORS.fg.green}.${COLORS.reset}`);
                return { success: true, hash };
            }).catch(err => {
                if (!silent) process.stdout.write(`${COLORS.fg.red}x${COLORS.reset}`);
                return { success: false, error: err.message };
            });

            promises.push(p);
        }

        // 4. Await All (Fire and Forget completion)
        if (!silent) console.log(`${COLORS.dim}   Broadcasting...${COLORS.reset}`);
        const results = await Promise.all(promises);

        successCount = results.filter(r => r.success).length;

        // 5. Update Tracker (Commit to Disk)
        saveNonceKey(wallet.address, currentNonceKeyBase + STORM_SIZE);

        // 6. Inspect Sample High-Speed Transaction (The last one)
        const lastSuccess = results.reverse().find(r => r.success);
        if (lastSuccess && !silent) {
            console.log(`\n${COLORS.dim}   Inspecting Sample Tx: ${lastSuccess.hash}${COLORS.reset}`);
            // Wait brief moment for indexing?
            // Actually inspector handles pending, but receipt might be missing if too fast.
            // We just show what we have.
            try { await TempoInspector.logReport(lastSuccess.hash, { silent: false, proxy }); } catch (e) { }
        }

        const duration = (Date.now() - startTime) / 1000;
        const tps = (successCount / duration).toFixed(2);

        logWalletAction(workerId, walletIndex, wallet.address, 'DeployStorm', 'success', `${successCount}/${STORM_SIZE} Contracts (${tps} TPS)`, silent, duration);

        if (!silent) {
            console.log(`\n${COLORS.fg.green}✓ Storm Result: ${successCount}/${STORM_SIZE} deployed in ${duration.toFixed(2)}s (${tps} TPS)${COLORS.reset}`);
        }

        return { success: true, deployed: successCount, tps };

    } catch (e) {
        const duration = (Date.now() - startTime) / 1000;
        logWalletAction(workerId, walletIndex, wallet.address, 'DeployStorm', 'failed', e.message, silent, duration);
        return { success: false, reason: e.message };
    }
}

export async function runDeployStormMenu() {
    console.log(`\n  ${COLORS.fg.magenta}🌩  TEMPO DEPLOY STORM (STRESS TEST)${COLORS.reset}\n`);

    const privateKeys = getPrivateKeys();
    for (let i = 0; i < privateKeys.length; i++) {
        const { wallet, proxy } = await getWallet(i, privateKeys[i]);
        console.log(`${COLORS.dim}────────────────────────────────────────────${COLORS.reset}`);
        console.log(`${COLORS.fg.magenta}WALLET #${i + 1}: ${wallet.address}${COLORS.reset}`);

        await startDeployStorm(wallet, proxy, 1, i);

        if (i < privateKeys.length - 1) await sleep(1000);
    }
    console.log(`\n${COLORS.fg.green}✓ Storm Cycle Complete.${COLORS.reset}\n`);
}
