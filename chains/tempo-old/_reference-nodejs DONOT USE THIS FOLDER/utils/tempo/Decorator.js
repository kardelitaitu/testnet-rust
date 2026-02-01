
import * as ammActions from './actions/amm.js';
import * as dexActions from './actions/dex.js';
import * as faucetActions from './actions/faucet.js';
import * as feeActions from './actions/fee.js';
import * as nonceActions from './actions/nonce.js';
import * as policyActions from './actions/policy.js';
import * as rewardActions from './actions/reward.js';
import * as tokenActions from './actions/token.js';

function bindActions(client, actions) {
    const bound = {};
    for (const key of Object.keys(actions)) {
        if (typeof actions[key] === 'function') {
            bound[key] = (args) => actions[key](client, args);
        }
    }
    return bound;
}

export function tempoActions(client) { // intended to be passed to client.extend? 
    // If used as client.extend(tempoActions), then `client` is passed here.
    // wait, expected usage: client.extend(tempoActions()) ?
    // In `Decorator.ts`: `export function tempoActions<...>() { return (client) => ({ ... }) }` typically.
    // The snippet showed: `client.extend(tempoActions())`.
    // So `tempoActions` returns a function that takes `client`.
    return (client) => ({
        amm: bindActions(client, ammActions),
        dex: bindActions(client, dexActions),
        faucet: bindActions(client, faucetActions),
        fee: bindActions(client, feeActions),
        nonce: bindActions(client, nonceActions),
        policy: bindActions(client, policyActions),
        reward: bindActions(client, rewardActions),
        token: bindActions(client, tokenActions),
    });
}
