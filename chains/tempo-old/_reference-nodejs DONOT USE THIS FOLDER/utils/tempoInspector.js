import { createPublicClient, http, decodeFunctionData, decodeEventLog, formatEther, formatUnits } from 'viem';
import { tempoModerato } from 'viem/chains';
import { tempoActions } from 'viem/tempo';
import { CONFIG, COLORS, VERSION_INFO } from './constants.js';
import { getProxyAgent, getProxyIp } from './proxies.js';
import axios from 'axios';

/**
 * Common ABIs for Decoding
 */
const ABIS = {
    FUNCTIONS: [
        { type: 'function', name: 'transfer', inputs: [{ name: 'to', type: 'address' }, { name: 'value', type: 'uint256' }] },
        { type: 'function', name: 'approve', inputs: [{ name: 'spender', type: 'address' }, { name: 'value', type: 'uint256' }] },
        { type: 'function', name: 'mint', inputs: [{ name: 'to', type: 'address' }, { name: 'amount', type: 'uint256' }] },
        { type: 'function', name: 'burn', inputs: [{ name: 'amount', type: 'uint256' }] },
        { type: 'function', name: 'swapExactTokensForTokens', inputs: [{ name: 'amountIn', type: 'uint256' }, { name: 'amountOutMin', type: 'uint256' }, { name: 'path', type: 'address[]' }, { name: 'to', type: 'address' }, { name: 'deadline', type: 'uint256' }] }
    ],
    EVENTS: [
        { type: 'event', name: 'Transfer', inputs: [{ name: 'from', type: 'address', indexed: true }, { name: 'to', type: 'address', indexed: true }, { name: 'value', type: 'uint256', indexed: false }] },
        { type: 'event', name: 'Approval', inputs: [{ name: 'owner', type: 'address', indexed: true }, { name: 'spender', type: 'address', indexed: true }, { name: 'value', type: 'uint256', indexed: false }] }
    ]
};

/**
 * TempoInspector: A functional, easy-to-read transaction auditor for Tempo.
 */
export class TempoInspector {
    /**
     * Prints a clean, high-hierarchy audit report to the console.
     * @param {string} txHash - The full transaction hash.
     * @param {object} options - Options: { silent: boolean, verbose: boolean, proxy: string }
     */
    static async logReport(txHash, options = {}) {
        const { silent = false, verbose = false, proxy = null } = options;
        if (!txHash?.startsWith('0x')) return null;

        const agent = proxy ? getProxyAgent(proxy) : null;
        const proxyIp = getProxyIp(proxy);

        const customFetch = async (url, opts) => {
            const cfg = { method: opts.method || 'POST', url: url.toString(), data: opts.body, headers: JSON.parse(opts.headers || '{}'), timeout: 30000 };
            if (agent) { cfg.httpsAgent = agent; cfg.httpAgent = agent; }
            const res = await axios(cfg);
            return { ok: res.status < 300, status: res.status, json: async () => res.data, text: async () => JSON.stringify(res.data) };
        };

        const client = createPublicClient({ chain: tempoModerato, transport: http(CONFIG.RPC_URL, { fetch: customFetch }) }).extend(tempoActions());

        try {
            const tx = await client.getTransaction({ hash: txHash });
            const receipt = await client.getTransactionReceipt({ hash: txHash }).catch(() => null);
            if (silent) return tx;

            const divider = `${COLORS.dim}────────────────────────────────────────────────────────────────────────────────${COLORS.reset}`;
            const label = (txt) => `${COLORS.fg.yellow}${txt.padEnd(16)}:${COLORS.reset}`;

            console.log(`\n${COLORS.bright}${COLORS.fg.cyan}📊 TEMPO TRANSACTION AUDIT${COLORS.reset}`);
            console.log(divider);

            // 1. PRIMARY STATUS & ACTION (Highest Priority)
            const statusColor = receipt ? (receipt.status === 'success' ? COLORS.fg.green : COLORS.fg.red) : COLORS.fg.cyan;
            const statusText = receipt ? receipt.status.toUpperCase() : 'PENDING';

            console.log(`${label('Status')} ${statusColor}${COLORS.bright}${statusText}${COLORS.reset}`);
            console.log(`${label('Hash')} ${tx.hash}`);

            // Decoded Top-Level Action
            const action = this._getActionName(tx.input);
            if (action) console.log(`${label('Action')} ${COLORS.bright}${COLORS.fg.green}${action}${COLORS.reset}`);

            console.log(divider);

            // 2. ENTITIES & VALUE
            console.log(`${label('From')} ${tx.from}`);
            console.log(`${label('To')} ${tx.to || (tx.type === 'tempo' ? 'Batch/Native Call' : 'Contract Creation')}`);
            if (tx.value > 0n) console.log(`${label('Value')} ${formatEther(tx.value)} Alpha`);

            // 3. EXECUTION DETAILS (GAS & FEES)
            console.log(divider);
            const gasLim = tx.gas || tx.gasLimit;
            if (receipt) {
                const gasPct = Math.round(Number(receipt.gasUsed) / Number(gasLim) * 100);
                console.log(`${label('Gas Usage')} ${Number(receipt.gasUsed).toLocaleString()} / ${Number(gasLim).toLocaleString()} units (${gasPct}%)`);
                console.log(`${label('Effective Price')} ${(Number(receipt.effectiveGasPrice) / 1e9).toFixed(4)} gWei`);
            } else {
                console.log(`${label('Gas Limit')} ${Number(gasLim).toLocaleString()} units`);
            }

            // 4. TEMPO SCHEDULING
            if (tx.type === 'tempo' && (tx.validAfter || tx.validBefore)) {
                console.log(divider);
                const now = Math.floor(Date.now() / 1000);
                if (tx.validAfter) console.log(`${label('Valid After')} ${new Date(Number(tx.validAfter) * 1000).toLocaleString()}`);
                if (tx.validBefore) console.log(`${label('Valid Before')} ${new Date(Number(tx.validBefore) * 1000).toLocaleString()}`);

                let windowStatus = 'ACTIVE';
                if (tx.validAfter && now < Number(tx.validAfter)) windowStatus = `PENDING (In ${Number(tx.validAfter) - now}s)`;
                if (tx.validBefore && now > Number(tx.validBefore)) windowStatus = 'EXPIRED';
                console.log(`${label('Time Window')} ${windowStatus}`);
            }

            // 5. DATA PAYLOAD & LOGS
            if ((tx.input && tx.input !== '0x') || (receipt && receipt.logs.length > 0)) {
                console.log(divider);
                if (tx.type === 'tempo' && tx.calls?.length > 0) {
                    tx.calls.forEach((c, i) => {
                        console.log(`  ${COLORS.fg.magenta}[Call ${i}]${COLORS.reset} -> ${c.to}`);
                        this._decodeDataSub(c.data);
                    });
                } else if (tx.input !== '0x') {
                    this._decodeDataSub(tx.input);
                }

                if (receipt?.logs?.length > 0) {
                    console.log(`${COLORS.dim}Events Emitted:${COLORS.reset}`);
                    receipt.logs.forEach((l, i) => this._decodeLogSub(l, i));
                }
            }

            // 6. TECHNICAL TRACE (COMPACT)
            console.log(divider);
            console.log(`${COLORS.dim}Network ID: ${tx.chainId || '42431'} | Nonce: ${tx.nonce} | Type: ${tx.type} | Node: ${proxyIp}${COLORS.reset}`);

            if (verbose) {
                console.log(divider);
                console.log(`${COLORS.dim}Signature Trace:${COLORS.reset}`);
                console.log(`  r: ${tx.r}\n  s: ${tx.s}\n  yParity: ${tx.yParity}`);
                console.log(`${COLORS.dim}Raw JSON Check:${COLORS.reset}`);
                console.log(JSON.stringify(tx, (k, v) => typeof v === 'bigint' ? v.toString() : v, 2));
            }
            console.log(divider + '\n');

            return tx;
        } catch (e) {
            if (!silent) console.error(`\n${COLORS.fg.red}❌ Audit Error: ${e.message}${COLORS.reset}\n`);
            return null;
        }
    }

    /** @private */
    static _getActionName(data) {
        if (!data || data === '0x') return null;
        for (const abi of ABIS.FUNCTIONS) {
            try { const d = decodeFunctionData({ abi: [abi], data }); if (d) return d.functionName.toUpperCase(); } catch (e) { }
        }
        return null;
    }

    /** @private */
    static _decodeDataSub(data) {
        if (!data || data === '0x') return;
        let d = null;
        for (const abi of ABIS.FUNCTIONS) {
            try { d = decodeFunctionData({ abi: [abi], data }); if (d) break; } catch (e) { }
        }
        if (d) {
            d.args.forEach((a, i) => {
                let v = (typeof a === 'bigint' && a > 1000000n) ? formatUnits(a, 18) : a.toString();
                console.log(`    ${COLORS.dim}arg[${i}]${COLORS.reset} ${v}`);
            });
        }
    }

    /** @private */
    static _decodeLogSub(log, idx) {
        let d = null;
        for (const abi of ABIS.EVENTS) {
            try { d = decodeEventLog({ abi: [abi], data: log.data, topics: log.topics }); if (d) break; } catch (e) { }
        }
        if (d) {
            console.log(`  ${COLORS.fg.cyan}[${d.eventName}]${COLORS.reset} ${Object.values(d.args).join(' | ')}`);
        } else {
            console.log(`  ${COLORS.dim}[Log ${idx}] ${log.address.substring(0, 12)}...${COLORS.reset}`);
        }
    }
}
