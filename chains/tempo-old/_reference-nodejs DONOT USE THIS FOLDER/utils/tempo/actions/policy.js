
import { parseAccount } from 'viem/utils';
import { readContract, writeContract, watchContractEvent } from 'viem/actions';
import { writeContractSync } from 'viem/actions';
import { parseEventLogs } from 'viem/utils';
import { defineCall } from '../internal/utils.js';
import * as Abis from '../Abis.js';
import * as Addresses from '../Addresses.js';

const policyTypeMap = {
    whitelist: 0,
    blacklist: 1,
};

export async function create(client, parameters) {
    return createInner(writeContract, client, parameters);
}
create.call = createCall;
create.extractEvent = createExtractEvent;

function createCall(args) {
    const { admin, type, addresses } = args;
    const config = addresses
        ? {
            functionName: 'createPolicyWithAccounts',
            args: [admin, policyTypeMap[type], addresses],
        }
        : {
            functionName: 'createPolicy',
            args: [admin, policyTypeMap[type]],
        };
    return defineCall({
        address: Addresses.tip403Registry,
        abi: Abis.tip403Registry,
        ...config,
    });
}

function createExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.tip403Registry,
        logs,
        eventName: 'PolicyCreated',
        strict: true,
    });
    if (!log) throw new Error('`PolicyCreated` event not found.');
    return log;
}

async function createInner(action, client, parameters) {
    const { account = client.account, addresses, chain = client.chain, type, ...rest } = parameters;
    if (!account) throw new Error('`account` is required');
    const admin = parseAccount(account).address; // Using account as admin by default logic in TS?
    // TS: const admin = parseAccount(account).address!
    // But wait, creates policy with `admin` param.
    // In TS: `const admin = parseAccount(account).address!` is used for... 
    // Ah, inner function uses `account` to derive `admin` if not passed? 
    // No, `create.call` takes `admin`.
    // Let's look at TS again.
    // parameters includes `admin` (optional in sync wrapper logic? No).
    // In `create.Parameters`: `Omit<Args, 'admin'> & { admin?: Address }`.
    // `inner`: `const admin = parseAccount(account).address!`
    // It seems it ignores the passed `admin` param in `parameters` and uses the `account` as admin?
    // Wait, line 112: `const admin = parseAccount(account).address!`
    // line 114: `create.call({ admin, type, addresses })`
    // So if I pass `admin` in parameters, it is IGNORED and `account` is used?
    // That seems like a bug or specific design in the TS file.
    // "Address of the policy admin" is in `Args`, but `Parameters` overrides it.
    // I will check `create.Parameters` definition again.
    // `Omit<Args, 'admin'> & { admin?: Address }`
    // But `inner` logic:
    // `const { account = client.account, ... } = parameters`
    // `const admin = parseAccount(account).address!`
    // It forces admin to be the caller.

    // I will replicate this behavior.
    const derivedAdmin = parseAccount(account).address;
    const call = createCall({ admin: derivedAdmin, type, addresses });
    return await action(client, { ...rest, account, chain, ...call });
}

export async function setAdmin(client, parameters) {
    return setAdminInner(writeContract, client, parameters);
}
setAdmin.call = setAdminCall;
setAdmin.extractEvent = setAdminExtractEvent;

function setAdminCall(args) {
    const { policyId, admin } = args;
    return defineCall({
        address: Addresses.tip403Registry,
        abi: Abis.tip403Registry,
        functionName: 'setPolicyAdmin',
        args: [policyId, admin],
    });
}

function setAdminExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.tip403Registry,
        logs,
        eventName: 'PolicyAdminUpdated',
        strict: true,
    });
    if (!log) throw new Error('`PolicyAdminUpdated` event not found.');
    return log;
}

async function setAdminInner(action, client, parameters) {
    const { policyId, admin, ...rest } = parameters;
    const call = setAdminCall({ policyId, admin });
    return await action(client, { ...rest, ...call });
}

export async function modifyWhitelist(client, parameters) {
    return modifyWhitelistInner(writeContract, client, parameters);
}
modifyWhitelist.call = modifyWhitelistCall;
modifyWhitelist.extractEvent = modifyWhitelistExtractEvent;

function modifyWhitelistCall(args) {
    const { policyId, address, allowed } = args;
    return defineCall({
        address: Addresses.tip403Registry,
        abi: Abis.tip403Registry,
        functionName: 'modifyPolicyWhitelist',
        args: [policyId, address, allowed],
    });
}

function modifyWhitelistExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.tip403Registry,
        logs,
        eventName: 'WhitelistUpdated',
        strict: true,
    });
    if (!log) throw new Error('`WhitelistUpdated` event not found.');
    return log;
}

async function modifyWhitelistInner(action, client, parameters) {
    const { address, allowed, policyId, ...rest } = parameters;
    const call = modifyWhitelistCall({ address, allowed, policyId });
    return await action(client, { ...rest, ...call });
}

export async function modifyBlacklist(client, parameters) {
    return modifyBlacklistInner(writeContract, client, parameters);
}
modifyBlacklist.call = modifyBlacklistCall;
modifyBlacklist.extractEvent = modifyBlacklistExtractEvent;

function modifyBlacklistCall(args) {
    const { policyId, address, restricted } = args;
    return defineCall({
        address: Addresses.tip403Registry,
        abi: Abis.tip403Registry,
        functionName: 'modifyPolicyBlacklist',
        args: [policyId, address, restricted],
    });
}

function modifyBlacklistExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.tip403Registry,
        logs,
        eventName: 'BlacklistUpdated',
        strict: true,
    });
    if (!log) throw new Error('`BlacklistUpdated` event not found.');
    return log;
}

async function modifyBlacklistInner(action, client, parameters) {
    const { address, policyId, restricted, ...rest } = parameters;
    const call = modifyBlacklistCall({ address, policyId, restricted });
    return await action(client, { ...rest, ...call });
}

export async function getData(client, parameters) {
    const { policyId, ...rest } = parameters;
    const result = await readContract(client, {
        ...rest,
        ...getDataCall({ policyId }),
    });
    return {
        admin: result[1],
        type: result[0] === 0 ? 'whitelist' : 'blacklist',
    };
}
getData.call = getDataCall;
function getDataCall(args) {
    const { policyId } = args;
    return defineCall({
        address: Addresses.tip403Registry,
        abi: Abis.tip403Registry,
        args: [policyId],
        functionName: 'policyData',
    });
}

export async function isAuthorized(client, parameters) {
    const { policyId, user, ...rest } = parameters;
    return readContract(client, {
        ...rest,
        ...isAuthorizedCall({ policyId, user }),
    });
}
isAuthorized.call = isAuthorizedCall;
function isAuthorizedCall(args) {
    const { policyId, user } = args;
    return defineCall({
        address: Addresses.tip403Registry,
        abi: Abis.tip403Registry,
        args: [policyId, user],
        functionName: 'isAuthorized',
    });
}

export function watchCreate(client, parameters) {
    const { onPolicyCreated, ...rest } = parameters;
    return watchContractEvent(client, {
        ...rest,
        address: Addresses.tip403Registry,
        abi: Abis.tip403Registry,
        eventName: 'PolicyCreated',
        onLogs: (logs) => {
            for (const log of logs)
                onPolicyCreated(
                    {
                        ...log.args,
                        type: log.args.policyType === 0 ? 'whitelist' : 'blacklist',
                    },
                    log,
                );
        },
        strict: true,
    });
}

export function watchAdminUpdated(client, parameters) {
    const { onAdminUpdated, ...rest } = parameters;
    return watchContractEvent(client, {
        ...rest,
        address: Addresses.tip403Registry,
        abi: Abis.tip403Registry,
        eventName: 'PolicyAdminUpdated',
        onLogs: (logs) => {
            for (const log of logs) onAdminUpdated(log.args, log);
        },
        strict: true,
    });
}

export function watchWhitelistUpdated(client, parameters) {
    const { onWhitelistUpdated, ...rest } = parameters;
    return watchContractEvent(client, {
        ...rest,
        address: Addresses.tip403Registry,
        abi: Abis.tip403Registry,
        eventName: 'WhitelistUpdated',
        onLogs: (logs) => {
            for (const log of logs) onWhitelistUpdated(log.args, log);
        },
        strict: true,
    });
}

export function watchBlacklistUpdated(client, parameters) {
    const { onBlacklistUpdated, ...rest } = parameters;
    return watchContractEvent(client, {
        ...rest,
        address: Addresses.tip403Registry,
        abi: Abis.tip403Registry,
        eventName: 'BlacklistUpdated',
        onLogs: (logs) => {
            for (const log of logs) onBlacklistUpdated(log.args, log);
        },
        strict: true,
    });
}
