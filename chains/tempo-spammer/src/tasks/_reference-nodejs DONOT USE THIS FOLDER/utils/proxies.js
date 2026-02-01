import fs from 'fs';
import path from 'path';
import { HttpsProxyAgent } from 'https-proxy-agent';
import { SocksProxyAgent } from 'socks-proxy-agent';

import { getProxyStats } from './proxyMonitor.js';

const PROXIES_FILE = path.join(process.cwd(), 'proxies.txt');


let cachedProxies = null;

export function getProxies() {
    // Return cached if available
    if (cachedProxies !== null) return cachedProxies;

    try {
        if (!fs.existsSync(PROXIES_FILE)) {
            cachedProxies = [];
            return [];
        }
        const content = fs.readFileSync(PROXIES_FILE, 'utf-8');
        cachedProxies = content
            .split('\n')
            .map(line => line.trim())
            .filter(line => line && !line.startsWith('#'));

        return cachedProxies;
    } catch (error) {
        console.warn(`Warning: Could not read proxies.txt: ${error.message}`);
        return [];
    }
}

// Optional: Force reload if needed
export function reloadProxies() {
    cachedProxies = null;
    return getProxies();
}

export function getValidProxies() {
    const proxies = getProxies();
    const stats = getProxyStats();
    return proxies.filter(p => {
        if (!stats[p]) return true;
        // Allow if OK OR if it's a redemption attempt (SUSPICIOUS but > 24h old)
        if (stats[p].status !== 'SUSPICIOUS') return true;

        const lastSeen = stats[p].lastSeen ? new Date(stats[p].lastSeen).getTime() : 0;
        const ONE_DAY_MS = 24 * 60 * 60 * 1000;
        if (Date.now() - lastSeen > ONE_DAY_MS) return true; // Redemption chance

        return false;
    });
}

export function getProxyForIndex(index) {
    const proxies = getProxies();
    if (!proxies.length) return null;
    return proxies[index % proxies.length];
}

export function getRandomProxy() {
    const proxies = getProxies();
    if (!proxies.length) return null;
    return proxies[Math.floor(Math.random() * proxies.length)];
}

export function formatProxy(proxyStr) {
    if (!proxyStr) return null;

    // Already formatted
    if (proxyStr.includes('://')) return proxyStr;

    const parts = proxyStr.split(':');

    // ip:port:user:pass
    if (parts.length === 4) {
        const [ip, port, user, pass] = parts;
        return `http://${user}:${pass}@${ip}:${port}`;
    }

    // ip:port
    if (parts.length === 2) {
        const [ip, port] = parts;
        return `http://${ip}:${port}`;
    }

    return proxyStr;
}

/**
 * Extracts only the IP address from a proxy URL
 * @param {string} proxyUrl - The formatted proxy URL
 * @returns {string} - The IP address or 'DIRECT'
 */
export function getProxyIp(proxyUrl) {
    if (!proxyUrl) return 'DIRECT';
    try {
        const url = new URL(proxyUrl);
        return url.hostname;
    } catch (e) {
        // Fallback for non-URL formats
        const match = proxyUrl.match(/(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})/);
        return match ? match[1] : 'UNKNOWN_IP';
    }
}

const agentCache = new Map();

export function getProxyAgent(proxyUrl) {
    if (!proxyUrl) return null;

    const formatted = formatProxy(proxyUrl);

    // Check cache first
    if (agentCache.has(formatted)) {
        return agentCache.get(formatted);
    }

    let agent;
    if (formatted.startsWith('socks')) {
        agent = new SocksProxyAgent(formatted, { keepAlive: true });
    } else {
        agent = new HttpsProxyAgent(formatted, { keepAlive: true });
    }

    // Cache the new agent
    agentCache.set(formatted, agent);
    return agent;
}
