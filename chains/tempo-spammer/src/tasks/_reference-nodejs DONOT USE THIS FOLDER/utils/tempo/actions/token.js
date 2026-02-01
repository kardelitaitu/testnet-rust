
import * as Hex from 'ox/Hex';
import { TokenId } from 'ox/tempo';
import { parseEventLogs } from 'viem';
import { parseAccount } from 'viem/utils';
import { defineCall } from '../internal/utils.js';
import * as Abis from '../Abis.js';
import * as Addresses from '../Addresses.js';

const readContract = (client, args) => client.readContract(args);
const writeContract = (client, args) => client.writeContract(args);
const watchContractEvent = (client, args) => client.watchContractEvent(args);
const multicall = (client, args) => client.multicall(args);

export async function approve(client, parameters) {
    return approveInner(writeContract, client, parameters);
}
approve.call = approveCall;
approve.extractEvent = approveExtractEvent;

function approveCall(args) {
    const { spender, amount, token } = args;
    return defineCall({
        address: TokenId.toAddress(token),
        abi: Abis.tip20,
        functionName: 'approve',
        args: [spender, amount],
    });
}

function approveExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.tip20,
        logs,
        eventName: 'Approval',
    });
    if (!log) throw new Error('`Approval` event not found.');
    return log;
}

async function approveInner(action, client, parameters) {
    const { token, spender, amount, ...rest } = parameters;
    const call = approveCall({ spender, amount, token });
    return await action(client, { ...rest, ...call });
}

export async function burnBlocked(client, parameters) {
    return burnBlockedInner(writeContract, client, parameters);
}
burnBlocked.call = burnBlockedCall;
burnBlocked.extractEvent = burnBlockedExtractEvent;

function burnBlockedCall(args) {
    const { from, amount, token } = args;
    return defineCall({
        address: TokenId.toAddress(token),
        abi: Abis.tip20,
        functionName: 'burnBlocked',
        args: [from, amount],
    });
}

function burnBlockedExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.tip20,
        logs,
        eventName: 'BurnBlocked',
    });
    if (!log) throw new Error('`BurnBlocked` event not found.');
    return log;
}

async function burnBlockedInner(action, client, parameters) {
    const { amount, from, token, ...rest } = parameters;
    const call = burnBlockedCall({ amount, from, token });
    return await action(client, { ...rest, ...call });
}

export async function burn(client, parameters) {
    return burnInner(writeContract, client, parameters);
}
burn.call = burnCall;
burn.extractEvent = burnExtractEvent;

function burnCall(args) {
    const { amount, memo, token } = args;
    const callArgs = memo
        ? {
            functionName: 'burnWithMemo',
            args: [amount, Hex.padLeft(memo, 32)],
        }
        : {
            functionName: 'burn',
            args: [amount],
        };
    return defineCall({
        address: TokenId.toAddress(token),
        abi: Abis.tip20,
        ...callArgs,
    });
}

function burnExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.tip20,
        logs,
        eventName: 'Burn',
    });
    if (!log) throw new Error('`Burn` event not found.');
    return log;
}

async function burnInner(action, client, parameters) {
    const { amount, memo, token, ...rest } = parameters;
    const call = burnCall({ amount, memo, token });
    return await action(client, { ...rest, ...call });
}

export async function changeTransferPolicy(client, parameters) {
    return changeTransferPolicyInner(writeContract, client, parameters);
}
changeTransferPolicy.call = changeTransferPolicyCall;
changeTransferPolicy.extractEvent = changeTransferPolicyExtractEvent;

function changeTransferPolicyCall(args) {
    const { policyId, token } = args;
    return defineCall({
        address: TokenId.toAddress(token),
        abi: Abis.tip20,
        functionName: 'changeTransferPolicyId',
        args: [policyId],
    });
}

function changeTransferPolicyExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.tip20,
        logs,
        eventName: 'TransferPolicyUpdate',
    });
    if (!log) throw new Error('`TransferPolicyUpdate` event not found.');
    return log;
}

async function changeTransferPolicyInner(action, client, parameters) {
    const { policyId, token, ...rest } = parameters;
    const call = changeTransferPolicyCall({ policyId, token });
    return await action(client, { ...rest, ...call });
}

export async function create(client, parameters) {
    return createInner(writeContract, client, parameters);
}
create.call = createCall;
create.extractEvent = createExtractEvent;

function createCall(args) {
    const {
        name,
        symbol,
        currency,
        quoteToken = Addresses.pathUsd,
        admin,
        salt = Hex.random(32),
    } = args;
    return defineCall({
        address: Addresses.tip20Factory,
        abi: Abis.tip20Factory,
        args: [
            name,
            symbol,
            currency,
            TokenId.toAddress(quoteToken),
            admin,
            salt,
        ],
        functionName: 'createToken',
    });
}

function createExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.tip20Factory,
        logs,
        eventName: 'TokenCreated',
        strict: true,
    });
    if (!log) throw new Error('`TokenCreated` event not found.');
    return log;
}

async function createInner(action, client, parameters) {
    const {
        account = client.account,
        admin: admin_ = client.account,
        chain = client.chain,
        ...rest
    } = parameters;
    const admin = admin_ ? (typeof admin_ === 'string' ? admin_ : parseAccount(admin_).address) : undefined;
    if (!admin) throw new Error('admin is required.');

    // We pass admin correctly to call
    const call = createCall({ ...rest, admin });

    return await action(client, {
        ...parameters,
        ...call,
    });
}

export async function getAllowance(client, parameters) {
    const { account = client.account, spender, token } = parameters;
    const address = account ? (typeof account === 'string' ? account : account.address) : undefined;
    if (!address) throw new Error('account is required.');

    return readContract(client, {
        ...parameters,
        ...getAllowanceCall({ account: address, spender, token }),
    });
}
getAllowance.call = getAllowanceCall;
function getAllowanceCall(args) {
    const { account, spender, token } = args;
    return defineCall({
        address: TokenId.toAddress(token),
        abi: Abis.tip20,
        functionName: 'allowance',
        args: [account, spender],
    });
}

export async function getBalance(client, parameters) {
    const { account = client.account, token, ...rest } = parameters;
    const address = account ? (typeof account === 'string' ? account : account.address) : undefined;
    if (!address) throw new Error('account is required.');

    return readContract(client, {
        ...rest,
        ...getBalanceCall({ account: address, token }),
    });
}
getBalance.call = getBalanceCall;
function getBalanceCall(args) {
    const { account, token } = args;
    return defineCall({
        address: TokenId.toAddress(token),
        abi: Abis.tip20,
        functionName: 'balanceOf',
        args: [account],
    });
}

export async function getMetadata(client, parameters) {
    const { token, ...rest } = parameters;
    const address = TokenId.toAddress(token);
    const abi = Abis.tip20;

    // Special case for PathUSD or standard multicall
    // Ignoring PathUSD optimization for brevity, just doing standard multicall
    // But keeping logic similar to original is better.
    // Original implementation splits logic. Check if TokenId matches PathUSD.
    // Assuming TokenId.fromAddress exists or similar comparison.
    // `TokenId.from(token) === TokenId.fromAddress(Addresses.pathUsd)`

    // I'll implement the full multicall logic.
    const calls = [
        { address, abi, functionName: 'currency' },
        { address, abi, functionName: 'decimals' },
        { address, abi, functionName: 'quoteToken' }, // May fail for PathUSD? No, PathUSD has simpler metadata.
        { address, abi, functionName: 'name' },
        { address, abi, functionName: 'paused' },
        { address, abi, functionName: 'supplyCap' },
        { address, abi, functionName: 'symbol' },
        { address, abi, functionName: 'totalSupply' },
        { address, abi, functionName: 'transferPolicyId' },
    ];

    // If PathUSD, some might fail or not exist? 
    // The original code branches.
    // I will branch if I can import Addresses properly.
    // Imported Addresses.pathUsd.

    const isPathUSD = address.toLowerCase() === Addresses.pathUsd.toLowerCase(); // simplified check

    if (isPathUSD) {
        return multicall(client, {
            ...rest,
            contracts: [
                { address, abi, functionName: 'currency' },
                { address, abi, functionName: 'decimals' },
                { address, abi, functionName: 'name' },
                { address, abi, functionName: 'symbol' },
                { address, abi, functionName: 'totalSupply' },
            ],
            allowFailure: false,
        }).then(([currency, decimals, name, symbol, totalSupply]) => ({
            name, symbol, currency, decimals, totalSupply
        }));
    }

    return multicall(client, {
        ...rest,
        contracts: calls,
        allowFailure: false,
    }).then(([currency, decimals, quoteToken, name, paused, supplyCap, symbol, totalSupply, transferPolicyId]) => ({
        currency, decimals, quoteToken, name, paused, supplyCap, symbol, totalSupply, transferPolicyId
    }));
}
