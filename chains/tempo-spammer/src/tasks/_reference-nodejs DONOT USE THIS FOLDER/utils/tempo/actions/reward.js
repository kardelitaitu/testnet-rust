
import * as Hex from 'ox/Hex';
import { TokenId } from 'ox/tempo'; // Assuming needed or removed if not
import { readContract, writeContract, watchContractEvent } from 'viem/actions';
import { writeContractSync } from 'viem/actions';
import { parseEventLogs } from 'viem/utils';
import { defineCall } from '../internal/utils.js';
import * as Abis from '../Abis.js';
import * as Addresses from '../Addresses.js';

export async function claim(client, parameters) {
    return claimInner(writeContract, client, parameters);
}
claim.call = claimCall;

function claimCall(args) {
    const { token } = args;
    return defineCall({
        address: token,
        abi: Abis.tip20,
        functionName: 'claimRewards',
        args: [],
    });
}
async function claimInner(action, client, parameters) {
    const { token, ...rest } = parameters;
    const call = claimCall({ token });
    return await action(client, { ...rest, ...call });
}

export async function distribute(client, parameters) {
    return distributeInner(writeContract, client, parameters);
}
distribute.call = distributeCall;
distribute.extractEvent = distributeExtractEvent;

function distributeCall(args) {
    const { amount, token } = args;
    return defineCall({
        address: token,
        abi: Abis.tip20,
        functionName: 'distributeReward',
        args: [amount],
    });
}

function distributeExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.tip20,
        logs,
        eventName: 'RewardDistributed',
        strict: true,
    });
    if (!log) throw new Error('`RewardDistributed` event not found.');
    return log;
}

async function distributeInner(action, client, parameters) {
    const { amount, token, ...rest } = parameters;
    const call = distributeCall({ amount, token });
    return await action(client, { ...rest, ...call });
}

export async function getGlobalRewardPerToken(client, parameters) {
    const { token, ...rest } = parameters;
    return readContract(client, {
        ...rest,
        ...getGlobalRewardPerTokenCall({ token }),
    });
}
getGlobalRewardPerToken.call = getGlobalRewardPerTokenCall;
function getGlobalRewardPerTokenCall(args) {
    const { token } = args;
    return defineCall({
        address: token,
        abi: Abis.tip20,
        functionName: 'globalRewardPerToken',
        args: [],
    });
}

export async function getPendingRewards(client, parameters) {
    const { token, account, ...rest } = parameters;
    return readContract(client, {
        ...rest,
        ...getPendingRewardsCall({ token, account }),
    });
}
getPendingRewards.call = getPendingRewardsCall;
function getPendingRewardsCall(args) {
    const { token, account } = args;
    return defineCall({
        address: token,
        abi: Abis.tip20,
        functionName: 'getPendingRewards',
        args: [account],
    });
}

export async function getUserRewardInfo(client, parameters) {
    const { token, account, ...rest } = parameters;
    return readContract(client, {
        ...rest,
        ...getUserRewardInfoCall({ token, account }),
    });
}
getUserRewardInfo.call = getUserRewardInfoCall;
function getUserRewardInfoCall(args) {
    const { token, account } = args;
    return defineCall({
        address: token,
        abi: Abis.tip20,
        functionName: 'getUserRewardInfo',
        args: [account],
    });
}

export async function setRecipient(client, parameters) {
    return setRecipientInner(writeContract, client, parameters);
}
setRecipient.call = setRecipientCall;
setRecipient.extractEvent = setRecipientExtractEvent;

function setRecipientCall(args) {
    const { recipient, token } = args;
    return defineCall({
        address: token,
        abi: Abis.tip20,
        functionName: 'setRewardRecipient',
        args: [recipient],
    });
}

function setRecipientExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.tip20,
        logs,
        eventName: 'RewardRecipientChanged',
        strict: true,
    });
    if (!log) throw new Error('`RewardRecipientChanged` event not found.');
    return log;
}

async function setRecipientInner(action, client, parameters) {
    const { recipient, token, ...rest } = parameters;
    const call = setRecipientCall({ recipient, token });
    return await action(client, { ...rest, ...call });
}
