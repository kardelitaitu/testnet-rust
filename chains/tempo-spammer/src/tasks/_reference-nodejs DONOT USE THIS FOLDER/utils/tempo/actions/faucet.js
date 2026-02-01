
import { parseAccount } from 'viem/utils';
import { waitForTransactionReceipt } from 'viem/actions';

export async function fund(client, parameters) {
    const account = parseAccount(parameters.account);
    return client.request({
        method: 'tempo_fundAddress',
        params: [account.address],
    });
}

export async function fundSync(client, parameters) {
    const { timeout = 10_000, ...rest } = parameters;
    const account = parseAccount(parameters.account);
    const hashes = await client.request({
        method: 'tempo_fundAddress',
        params: [account.address],
    });

    // hashes is readonly Hash[]
    const receipts = await Promise.all(
        hashes.map((hash) =>
            waitForTransactionReceipt(client, {
                hash,
                checkReplacement: false,
                timeout,
            }),
        ),
    );
    return receipts;
}
