
export const tip20 = [
    {
        name: 'name',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'string' }],
    },
    {
        name: 'symbol',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'string' }],
    },
    {
        name: 'decimals',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'uint8' }],
    },
    {
        name: 'totalSupply',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'uint256' }],
    },
    {
        name: 'quoteToken',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'address' }],
    },
    {
        name: 'nextQuoteToken',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'address' }],
    },
    {
        name: 'balanceOf',
        type: 'function',
        stateMutability: 'view',
        inputs: [{ type: 'address', name: 'account' }],
        outputs: [{ type: 'uint256' }],
    },
    {
        name: 'transfer',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'address', name: 'to' },
            { type: 'uint256', name: 'amount' },
        ],
        outputs: [{ type: 'bool' }],
    },
    {
        name: 'approve',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'address', name: 'spender' },
            { type: 'uint256', name: 'amount' },
        ],
        outputs: [{ type: 'bool' }],
    },
    {
        name: 'allowance',
        type: 'function',
        stateMutability: 'view',
        inputs: [
            { type: 'address', name: 'owner' },
            { type: 'address', name: 'spender' },
        ],
        outputs: [{ type: 'uint256' }],
    },
    {
        name: 'transferFrom',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'address', name: 'from' },
            { type: 'address', name: 'to' },
            { type: 'uint256', name: 'amount' },
        ],
        outputs: [{ type: 'bool' }],
    },
    {
        name: 'mint',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'address', name: 'to' },
            { type: 'uint256', name: 'amount' },
        ],
        outputs: [],
    },
    {
        name: 'burn',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [{ type: 'uint256', name: 'amount' }],
        outputs: [],
    },
    {
        name: 'currency',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'string' }],
    },
    {
        name: 'supplyCap',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'uint256' }],
    },
    {
        name: 'paused',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'bool' }],
    },
    {
        name: 'transferPolicyId',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'uint64' }],
    },
    {
        name: 'burnBlocked',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'address', name: 'from' },
            { type: 'uint256', name: 'amount' },
        ],
        outputs: [],
    },
    {
        name: 'mintWithMemo',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'address', name: 'to' },
            { type: 'uint256', name: 'amount' },
            { type: 'bytes32', name: 'memo' },
        ],
        outputs: [],
    },
    {
        name: 'burnWithMemo',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'uint256', name: 'amount' },
            { type: 'bytes32', name: 'memo' },
        ],
        outputs: [],
    },
    {
        name: 'transferWithMemo',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'address', name: 'to' },
            { type: 'uint256', name: 'amount' },
            { type: 'bytes32', name: 'memo' },
        ],
        outputs: [],
    },
    {
        name: 'transferFromWithMemo',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'address', name: 'from' },
            { type: 'address', name: 'to' },
            { type: 'uint256', name: 'amount' },
            { type: 'bytes32', name: 'memo' },
        ],
        outputs: [{ type: 'bool' }],
    },
    {
        name: 'changeTransferPolicyId',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [{ type: 'uint64', name: 'newPolicyId' }],
        outputs: [],
    },
    {
        name: 'setSupplyCap',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [{ type: 'uint256', name: 'newSupplyCap' }],
        outputs: [],
    },
    {
        name: 'pause',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [],
        outputs: [],
    },
    {
        name: 'unpause',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [],
        outputs: [],
    },
    {
        name: 'setNextQuoteToken',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [{ type: 'address', name: 'newQuoteToken' }],
        outputs: [],
    },
    {
        name: 'completeQuoteTokenUpdate',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [],
        outputs: [],
    },
    {
        name: 'PAUSE_ROLE',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'bytes32' }],
    },
    {
        name: 'UNPAUSE_ROLE',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'bytes32' }],
    },
    {
        name: 'ISSUER_ROLE',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'bytes32' }],
    },
    {
        name: 'BURN_BLOCKED_ROLE',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'bytes32' }],
    },
    {
        name: 'distributeReward',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [{ type: 'uint256', name: 'amount' }],
        outputs: [],
    },
    {
        name: 'setRewardRecipient',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [{ type: 'address', name: 'recipient' }],
        outputs: [],
    },
    {
        name: 'claimRewards',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [],
        outputs: [{ type: 'uint256' }],
    },
    {
        name: 'optedInSupply',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'uint128' }],
    },
    {
        name: 'globalRewardPerToken',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'uint256' }],
    },
    {
        name: 'userRewardInfo',
        type: 'function',
        stateMutability: 'view',
        inputs: [{ type: 'address', name: 'account' }],
        outputs: [
            {
                type: 'tuple',
                components: [
                    { type: 'address', name: 'rewardRecipient' },
                    { type: 'uint256', name: 'rewardPerToken' },
                    { type: 'uint256', name: 'rewardBalance' },
                ],
            },
        ],
    },
    {
        name: 'getPendingRewards',
        type: 'function',
        stateMutability: 'view',
        inputs: [{ type: 'address', name: 'account' }],
        outputs: [{ type: 'uint128' }],
    },
    {
        name: 'Transfer',
        type: 'event',
        inputs: [
            { type: 'address', name: 'from', indexed: true },
            { type: 'address', name: 'to', indexed: true },
            { type: 'uint256', name: 'amount' },
        ],
    },
    {
        name: 'Approval',
        type: 'event',
        inputs: [
            { type: 'address', name: 'owner', indexed: true },
            { type: 'address', name: 'spender', indexed: true },
            { type: 'uint256', name: 'amount' },
        ],
    },
    {
        name: 'Mint',
        type: 'event',
        inputs: [
            { type: 'address', name: 'to', indexed: true },
            { type: 'uint256', name: 'amount' },
        ],
    },
    {
        name: 'Burn',
        type: 'event',
        inputs: [
            { type: 'address', name: 'from', indexed: true },
            { type: 'uint256', name: 'amount' },
        ],
    },
    {
        name: 'BurnBlocked',
        type: 'event',
        inputs: [
            { type: 'address', name: 'from', indexed: true },
            { type: 'uint256', name: 'amount' },
        ],
    },
    {
        name: 'TransferWithMemo',
        type: 'event',
        inputs: [
            { type: 'address', name: 'from', indexed: true },
            { type: 'address', name: 'to', indexed: true },
            { type: 'uint256', name: 'amount' },
            { type: 'bytes32', name: 'memo', indexed: true },
        ],
    },
    {
        name: 'TransferPolicyUpdate',
        type: 'event',
        inputs: [
            { type: 'address', name: 'updater', indexed: true },
            { type: 'uint64', name: 'newPolicyId', indexed: true },
        ],
    },
    {
        name: 'SupplyCapUpdate',
        type: 'event',
        inputs: [
            { type: 'address', name: 'updater', indexed: true },
            { type: 'uint256', name: 'newSupplyCap', indexed: true },
        ],
    },
    {
        name: 'PauseStateUpdate',
        type: 'event',
        inputs: [
            { type: 'address', name: 'updater', indexed: true },
            { type: 'bool', name: 'isPaused' },
        ],
    },
    {
        name: 'NextQuoteTokenSet',
        type: 'event',
        inputs: [
            { type: 'address', name: 'updater', indexed: true },
            { type: 'address', name: 'nextQuoteToken', indexed: true },
        ],
    },
    {
        name: 'QuoteTokenUpdate',
        type: 'event',
        inputs: [
            { type: 'address', name: 'updater', indexed: true },
            { type: 'address', name: 'newQuoteToken', indexed: true },
        ],
    },
    {
        name: 'RewardDistributed',
        type: 'event',
        inputs: [
            { type: 'address', name: 'funder', indexed: true },
            { type: 'uint256', name: 'amount' },
        ],
    },
    {
        name: 'RewardRecipientSet',
        type: 'event',
        inputs: [
            { type: 'address', name: 'holder', indexed: true },
            { type: 'address', name: 'recipient', indexed: true },
        ],
    },
    {
        name: 'InsufficientBalance',
        type: 'error',
        inputs: [
            { type: 'uint256', name: 'available' },
            { type: 'uint256', name: 'required' },
            { type: 'address', name: 'token' },
        ],
    },
    { name: 'InsufficientAllowance', type: 'error', inputs: [] },
    { name: 'SupplyCapExceeded', type: 'error', inputs: [] },
    { name: 'InvalidSupplyCap', type: 'error', inputs: [] },
    { name: 'InvalidPayload', type: 'error', inputs: [] },
    { name: 'StringTooLong', type: 'error', inputs: [] },
    { name: 'PolicyForbids', type: 'error', inputs: [] },
    { name: 'InvalidRecipient', type: 'error', inputs: [] },
    { name: 'ContractPaused', type: 'error', inputs: [] },
    { name: 'InvalidCurrency', type: 'error', inputs: [] },
    { name: 'InvalidQuoteToken', type: 'error', inputs: [] },
    { name: 'TransfersDisabled', type: 'error', inputs: [] },
    { name: 'InvalidAmount', type: 'error', inputs: [] },
    { name: 'NoOptedInSupply', type: 'error', inputs: [] },
    { name: 'Unauthorized', type: 'error', inputs: [] },
    { name: 'ProtectedAddress', type: 'error', inputs: [] },
    { name: 'InvalidToken', type: 'error', inputs: [] },
    { name: 'Uninitialized', type: 'error', inputs: [] },
    { name: 'InvalidTransferPolicyId', type: 'error', inputs: [] },
    {
        name: 'hasRole',
        type: 'function',
        stateMutability: 'view',
        inputs: [
            { type: 'address', name: 'account' },
            { type: 'bytes32', name: 'role' },
        ],
        outputs: [{ type: 'bool' }],
    },
    {
        name: 'getRoleAdmin',
        type: 'function',
        stateMutability: 'view',
        inputs: [{ type: 'bytes32', name: 'role' }],
        outputs: [{ type: 'bytes32' }],
    },
    {
        name: 'grantRole',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'bytes32', name: 'role' },
            { type: 'address', name: 'account' },
        ],
        outputs: [],
    },
    {
        name: 'revokeRole',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'bytes32', name: 'role' },
            { type: 'address', name: 'account' },
        ],
        outputs: [],
    },
    {
        name: 'renounceRole',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [{ type: 'bytes32', name: 'role' }],
        outputs: [],
    },
    {
        name: 'setRoleAdmin',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'bytes32', name: 'role' },
            { type: 'bytes32', name: 'adminRole' },
        ],
        outputs: [],
    },
    {
        name: 'RoleMembershipUpdated',
        type: 'event',
        inputs: [
            { type: 'bytes32', name: 'role', indexed: true },
            { type: 'address', name: 'account', indexed: true },
            { type: 'address', name: 'sender', indexed: true },
            { type: 'bool', name: 'hasRole' },
        ],
    },
    {
        name: 'RoleAdminUpdated',
        type: 'event',
        inputs: [
            { type: 'bytes32', name: 'role', indexed: true },
            { type: 'bytes32', name: 'newAdminRole', indexed: true },
            { type: 'address', name: 'sender', indexed: true },
        ],
    },
    { name: 'Unauthorized', type: 'error', inputs: [] },
];

export const stablecoinDex = [
    {
        name: 'createPair',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [{ type: 'address', name: 'base' }],
        outputs: [{ type: 'bytes32', name: 'key' }],
    },
    {
        name: 'place',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'address', name: 'token' },
            { type: 'uint128', name: 'amount' },
            { type: 'bool', name: 'isBid' },
            { type: 'int16', name: 'tick' },
        ],
        outputs: [{ type: 'uint128', name: 'orderId' }],
    },
    {
        name: 'placeFlip',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'address', name: 'token' },
            { type: 'uint128', name: 'amount' },
            { type: 'bool', name: 'isBid' },
            { type: 'int16', name: 'tick' },
            { type: 'int16', name: 'flipTick' },
        ],
        outputs: [{ type: 'uint128', name: 'orderId' }],
    },
    {
        name: 'cancel',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [{ type: 'uint128', name: 'orderId' }],
        outputs: [],
    },
    {
        name: 'cancelStaleOrder',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [{ type: 'uint128', name: 'orderId' }],
        outputs: [],
    },
    {
        name: 'swapExactAmountIn',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'address', name: 'tokenIn' },
            { type: 'address', name: 'tokenOut' },
            { type: 'uint128', name: 'amountIn' },
            { type: 'uint128', name: 'minAmountOut' },
        ],
        outputs: [{ type: 'uint128', name: 'amountOut' }],
    },
    {
        name: 'swapExactAmountOut',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'address', name: 'tokenIn' },
            { type: 'address', name: 'tokenOut' },
            { type: 'uint128', name: 'amountOut' },
            { type: 'uint128', name: 'maxAmountIn' },
        ],
        outputs: [{ type: 'uint128', name: 'amountIn' }],
    },
    {
        name: 'quoteSwapExactAmountIn',
        type: 'function',
        stateMutability: 'view',
        inputs: [
            { type: 'address', name: 'tokenIn' },
            { type: 'address', name: 'tokenOut' },
            { type: 'uint128', name: 'amountIn' },
        ],
        outputs: [{ type: 'uint128', name: 'amountOut' }],
    },
    {
        name: 'quoteSwapExactAmountOut',
        type: 'function',
        stateMutability: 'view',
        inputs: [
            { type: 'address', name: 'tokenIn' },
            { type: 'address', name: 'tokenOut' },
            { type: 'uint128', name: 'amountOut' },
        ],
        outputs: [{ type: 'uint128', name: 'amountIn' }],
    },
    {
        name: 'balanceOf',
        type: 'function',
        stateMutability: 'view',
        inputs: [
            { type: 'address', name: 'user' },
            { type: 'address', name: 'token' },
        ],
        outputs: [{ type: 'uint128' }],
    },
    {
        name: 'withdraw',
        type: 'function',
        stateMutability: 'nonpayable',
        inputs: [
            { type: 'address', name: 'token' },
            { type: 'uint128', name: 'amount' },
        ],
        outputs: [],
    },
    {
        name: 'getOrder',
        type: 'function',
        stateMutability: 'view',
        inputs: [{ type: 'uint128', name: 'orderId' }],
        outputs: [
            {
                type: 'tuple',
                components: [
                    { type: 'uint128', name: 'orderId' },
                    { type: 'address', name: 'maker' },
                    { type: 'bytes32', name: 'bookKey' },
                    { type: 'bool', name: 'isBid' },
                    { type: 'int16', name: 'tick' },
                    { type: 'uint128', name: 'amount' },
                    { type: 'uint128', name: 'remaining' },
                    { type: 'uint128', name: 'prev' },
                    { type: 'uint128', name: 'next' },
                    { type: 'bool', name: 'isFlip' },
                    { type: 'int16', name: 'flipTick' },
                ],
            },
        ],
    },
    {
        name: 'getTickLevel',
        type: 'function',
        stateMutability: 'view',
        inputs: [
            { type: 'address', name: 'base' },
            { type: 'int16', name: 'tick' },
            { type: 'bool', name: 'isBid' },
        ],
        outputs: [
            { type: 'uint128', name: 'head' },
            { type: 'uint128', name: 'tail' },
            { type: 'uint128', name: 'totalLiquidity' },
        ],
    },
    {
        name: 'pairKey',
        type: 'function',
        stateMutability: 'pure',
        inputs: [
            { type: 'address', name: 'tokenA' },
            { type: 'address', name: 'tokenB' },
        ],
        outputs: [{ type: 'bytes32' }],
    },
    {
        name: 'nextOrderId',
        type: 'function',
        stateMutability: 'view',
        inputs: [],
        outputs: [{ type: 'uint128' }],
    },
    {
        name: 'books',
        type: 'function',
        stateMutability: 'view',
        inputs: [{ type: 'bytes32', name: 'pairKey' }],
        outputs: [
            {
                type: 'tuple',
                components: [
                    { type: 'address', name: 'base' },
                    { type: 'address', name: 'quote' },
                    { type: 'int16', name: 'bestBidTick' },
                    { type: 'int16', name: 'bestAskTick' },
                ],
            },
        ],
    },
    {
        name: 'MIN_TICK',
        type: 'function',
        stateMutability: 'pure',
        inputs: [],
        outputs: [{ type: 'int16' }],
    },
    {
        name: 'MAX_TICK',
        type: 'function',
        stateMutability: 'pure',
        inputs: [],
        outputs: [{ type: 'int16' }],
    },
    {
        name: 'TICK_SPACING',
        type: 'function',
        stateMutability: 'pure',
        inputs: [],
        outputs: [{ type: 'int16' }],
    },
    {
        name: 'PRICE_SCALE',
        type: 'function',
        stateMutability: 'pure',
        inputs: [],
        outputs: [{ type: 'uint32' }],
    },
    {
        name: 'MIN_ORDER_AMOUNT',
        type: 'function',
        stateMutability: 'pure',
        inputs: [],
        outputs: [{ type: 'uint128' }],
    },
    {
        name: 'MIN_PRICE',
        type: 'function',
        stateMutability: 'pure',
        inputs: [],
        outputs: [{ type: 'uint32' }],
    },
    {
        name: 'MAX_PRICE',
        type: 'function',
        stateMutability: 'pure',
        inputs: [],
        outputs: [{ type: 'uint32' }],
    },
    {
        name: 'tickToPrice',
        type: 'function',
        stateMutability: 'pure',
        inputs: [{ type: 'int16', name: 'tick' }],
        outputs: [{ type: 'uint32', name: 'price' }],
    },
    {
        name: 'priceToTick',
        type: 'function',
        stateMutability: 'pure',
        inputs: [{ type: 'uint32', name: 'price' }],
        outputs: [{ type: 'int16', name: 'tick' }],
    },
    {
        name: 'PairCreated',
        type: 'event',
        inputs: [
            { type: 'bytes32', name: 'key', indexed: true },
            { type: 'address', name: 'base', indexed: true },
            { type: 'address', name: 'quote', indexed: true },
        ],
    },
    {
        name: 'OrderPlaced',
        type: 'event',
        inputs: [
            { type: 'uint128', name: 'orderId', indexed: true },
            { type: 'address', name: 'maker', indexed: true },
            { type: 'address', name: 'token', indexed: true },
            { type: 'uint128', name: 'amount' },
            { type: 'bool', name: 'isBid' },
            { type: 'int16', name: 'tick' },
            { type: 'bool', name: 'isFlipOrder' },
        ],
    },
    {
        "name": "OrderFilled",
        "type": "event",
        "inputs": [
            { "name": "orderId", "type": "uint128", "indexed": true },
            { "name": "token", "type": "address", "indexed": true },
            { "name": "amount", "type": "uint128" },
            { "name": "isBid", "type": "bool" }
        ]
    }, {
        "name": "OrderCancelled",
        "type": "event",
        "inputs": [
            { "name": "orderId", "type": "uint128", "indexed": true }
        ]
    }
];

export const feeAmm = [
    {
        "name": "getPool",
        "type": "function",
        "stateMutability": "view",
        "inputs": [
            { "name": "userToken", "type": "address" },
            { "name": "validatorToken", "type": "address" }
        ],
        "outputs": [
            { "name": "reserveUserToken", "type": "uint256" },
            { "name": "reserveValidatorToken", "type": "uint256" }
        ]
    },
    {
        "name": "totalSupply",
        "type": "function",
        "stateMutability": "view",
        "inputs": [
            { "name": "poolId", "type": "bytes32" }
        ],
        "outputs": [
            { "name": "totalSupply", "type": "uint256" }
        ]
    },
    {
        "name": "liquidityBalances",
        "type": "function",
        "stateMutability": "view",
        "inputs": [
            { "name": "poolId", "type": "bytes32" },
            { "name": "account", "type": "address" }
        ],
        "outputs": [
            { "name": "balance", "type": "uint256" }
        ]
    },
    {
        "name": "rebalanceSwap",
        "type": "function",
        "stateMutability": "nonpayable",
        "inputs": [
            { "name": "userToken", "type": "address" },
            { "name": "validatorToken", "type": "address" },
            { "name": "amountOut", "type": "uint256" },
            { "name": "to", "type": "address" }
        ],
        "outputs": []
    },
    {
        "name": "mint",
        "type": "function",
        "stateMutability": "nonpayable",
        "inputs": [
            { "name": "userToken", "type": "address" },
            { "name": "validatorToken", "type": "address" },
            { "name": "validatorTokenAmount", "type": "uint256" },
            { "name": "to", "type": "address" }
        ],
        "outputs": []
    },
    {
        "name": "burn",
        "type": "function",
        "stateMutability": "nonpayable",
        "inputs": [
            { "name": "userToken", "type": "address" },
            { "name": "validatorToken", "type": "address" },
            { "name": "liquidity", "type": "uint256" },
            { "name": "to", "type": "address" }
        ],
        "outputs": []
    },
    {
        "name": "RebalanceSwap",
        "type": "event",
        "inputs": [
            { "name": "userToken", "type": "address", "indexed": true },
            { "name": "validatorToken", "type": "address", "indexed": true },
            { "name": "amountOut", "type": "uint256" }
        ]
    },
    {
        "name": "Mint",
        "type": "event",
        "inputs": [
            { "name": "userToken", "type": "address", "indexed": true },
            { "name": "validatorToken", "type": "address", "indexed": true },
            { "name": "liquidity", "type": "uint256" },
            { "name": "validatorTokenAmount", "type": "uint256" }
        ]
    },
    {
        "name": "Burn",
        "type": "event",
        "inputs": [
            { "name": "userToken", "type": "address", "indexed": true },
            { "name": "validatorToken", "type": "address", "indexed": true },
            { "name": "liquidity", "type": "uint256" },
            { "name": "amountUserToken", "type": "uint256" },
            { "name": "amountValidatorToken", "type": "uint256" }
        ]
    }
];
