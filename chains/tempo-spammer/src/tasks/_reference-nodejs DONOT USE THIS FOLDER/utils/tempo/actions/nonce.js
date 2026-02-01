
import { readContract, watchContractEvent } from 'viem/actions';
import { defineCall } from '../internal/utils.js';
import * as Abis from '../Abis.js';
import * as Addresses from '../Addresses.js';

export async function getNonce(client, parameters) {
    const { account, nonceKey, ...rest } = parameters;
    return readContract(client, {
        ...rest,
        ...getNonceCall({ account, nonceKey }),
    });
}
getNonce.call = getNonceCall;

function getNonceCall(args) {
    const { account, nonceKey } = args;
    return defineCall({
        address: Addresses.nonceManager,
        abi: Abis.nonce,
        args: [account, nonceKey],
        functionName: 'getNonce',
    });
}

export function watchNonceIncremented(client, parameters) {
    const { onNonceIncremented, ...rest } = parameters;
    return watchContractEvent(client, {
        ...rest,
        address: Addresses.nonceManager,
        abi: Abis.nonce,
        eventName: 'NonceIncremented',
        onLogs: (logs) => {
            for (const log of logs) onNonceIncremented(log.args, log);
        },
        strict: true,
    });
}
