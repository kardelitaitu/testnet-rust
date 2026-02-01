
import { TokenId } from 'ox/tempo';
import { parseAccount } from 'viem/utils';
import { parseEventLogs, zeroAddress as viemZeroAddress } from 'viem';

import { defineCall } from '../internal/utils.js';
import * as Abis from '../Abis.js';
import * as Addresses from '../Addresses.js';

const readContract = (client, args) => client.readContract(args);
const writeContract = (client, args) => client.writeContract(args);
const watchContractEvent = (client, args) => client.watchContractEvent(args);

export async function getUserToken(client, ...parameters) {
    // Support optional params handling similar to TS
    const { account: account_ = client.account, ...rest } = parameters[0] ?? {};
    if (!account_) throw new Error('account is required.');
    const account = parseAccount(account_);
    const address = await readContract(client, {
        ...rest,
        ...getUserTokenCall({ account: account.address }),
    });
    if (address === viemZeroAddress) return null;
    return {
        address,
        id: TokenId.fromAddress(address),
    };
}
getUserToken.call = getUserTokenCall;
function getUserTokenCall(args) {
    const { account } = args;
    return defineCall({
        address: Addresses.feeManager,
        abi: Abis.feeManager,
        args: [account],
        functionName: 'userTokens',
    });
}

export async function setUserToken(client, parameters) {
    return setUserTokenInner(writeContract, client, parameters);
}
setUserToken.call = setUserTokenCall;
setUserToken.extractEvent = setUserTokenExtractEvent;

function setUserTokenCall(args) {
    const { token } = args;
    return defineCall({
        address: Addresses.feeManager,
        abi: Abis.feeManager,
        functionName: 'setUserToken',
        args: [TokenId.toAddress(token)],
    });
}

function setUserTokenExtractEvent(logs) {
    const [log] = parseEventLogs({
        abi: Abis.feeManager,
        logs,
        eventName: 'UserTokenSet',
        strict: true,
    });
    if (!log) throw new Error('`UserTokenSet` event not found.');
    return log;
}

async function setUserTokenInner(action, client, parameters) {
    const { token, ...rest } = parameters;
    const call = setUserTokenCall({ token });
    return await action(client, { ...rest, ...call });
}

export function watchSetUserToken(client, parameters) {
    const { onUserTokenSet, ...rest } = parameters;
    return watchContractEvent(client, {
        ...rest,
        address: Addresses.feeManager,
        abi: Abis.feeManager,
        eventName: 'UserTokenSet',
        onLogs: (logs) => {
            for (const log of logs) onUserTokenSet(log.args, log);
        },
        strict: true,
    });
}
