import { Config } from '../config.js';

// Colors remain hardcoded constants
export const COLORS = {
    reset: "\x1b[0m",
    bright: "\x1b[1m",
    dim: "\x1b[2m",
    underscore: "\x1b[4m",
    blink: "\x1b[5m",
    reverse: "\x1b[7m",
    hidden: "\x1b[8m",

    fg: {
        black: "\x1b[30m",
        red: "\x1b[31m",
        green: "\x1b[32m",
        yellow: "\x1b[33m",
        blue: "\x1b[34m",
        magenta: "\x1b[35m",
        cyan: "\x1b[36m",
        white: "\x1b[37m",
    },
    bg: {
        black: "\x1b[40m",
        red: "\x1b[41m",
        green: "\x1b[42m",
        yellow: "\x1b[43m",
        blue: "\x1b[44m",
        magenta: "\x1b[45m",
        cyan: "\x1b[46m",
        white: "\x1b[47m",
    }
};

export const VERSION_INFO = {
    VERSION: '2.0.1',
    BUILD_DATE: '2025-01-09',
    AUTHOR: 'Shadow & Antigravity'
};

// Export CONFIG object wrapping the imported Config
export const CONFIG = {
    RPC_URL: Config.RPC_URL || 'https://rpc.testnet.tempo.xyz',
    RPC_LIST: Config.RPC_LIST || [],
    CHAIN_ID: Config.CHAIN_ID || 42429,
    EXPLORER_URL: Config.EXPLORER_URL || 'https://explore.tempo.xyz',
    GAS_LIMIT: Config.GAS_LIMIT || 3000000,
    GAS_PRICE_MULTIPLIER: Config.GAS_PRICE_MULTIPLIER || 5.0,

    MIN_DELAY_BETWEEN_WALLETS: Config.MIN_DELAY_BETWEEN_WALLETS || 5,
    MAX_DELAY_BETWEEN_WALLETS: Config.MAX_DELAY_BETWEEN_WALLETS || 30,
    MIN_DELAY_BETWEEN_DEPLOYS: Config.MIN_DELAY_BETWEEN_DEPLOYS || 3,
    MAX_DELAY_BETWEEN_DEPLOYS: Config.MAX_DELAY_BETWEEN_DEPLOYS || 10,
    FAUCET_CLAIM_DELAY_SEC: Config.FAUCET_CLAIM_DELAY_SEC || 15,
    FAUCET_FINISH_DELAY_SEC: Config.FAUCET_FINISH_DELAY_SEC || 30,
    FAUCET_PRE_CLAIM_MS: Config.FAUCET_PRE_CLAIM_MS || 4000,

    TOKENS: Config.TOKENS || {},
    FAUCET_TOKENS: Config.FAUCET_TOKENS || [],
    RATE_LIMIT: Config.RATE_LIMIT || {}
};

// Export System Contracts
export const SYSTEM_CONTRACTS = Config.SYSTEM_CONTRACTS || {};
