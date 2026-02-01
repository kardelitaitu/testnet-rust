# TIP-20 Rewards ⋅ Tempo
TIP-20 Rewards is a built-in mechanism that allows for efficient distribution of rewards to opted-in token holders proportional to their holdings, while maintaining low gas costs at scale and complying with [TIP-403 transfer policies](https://docs.tempo.xyz/protocol/tip403/spec).

Traditional reward mechanisms require tokens to be staked in separate contracts, which fragments user holdings and adds complexity to wallet implementations. TIP-20 Rewards solves this by:

*   **Built-in Distribution**: Rewards are integrated directly into the token contract, no separate staking required
*   **Opt-in Participation**: Users choose whether to participate by setting a reward recipient
*   **Proportional Distribution**: Rewards accrue based on token holdings automatically
*   **Instant Rewards**: Distribute rewards immediately to opted-in holders
*   **Efficient at Scale**: Constant-time updates regardless of the number of token holders
*   **Policy Compliant**: All reward transfers respect TIP-403 transfer policies

Links[](https://docs.tempo.xyz/protocol/tip20-rewards/overview#links)
---------------------------------------------------------------------

# TIP-20 Rewards Distribution ⋅ Tempo
Abstract[](https://docs.tempo.xyz/protocol/tip20-rewards/spec#abstract)
-----------------------------------------------------------------------

An opt-in, scalable, pro-rata reward distribution mechanism built into TIP-20 tokens. The system uses a "reward-per-token" accumulator pattern to distribute rewards proportionally to opted-in holders without requiring staking or per-holder iteration. Rewards are distributed instantly; time-based streaming distributions are planned for a future upgrade.

Motivation[](https://docs.tempo.xyz/protocol/tip20-rewards/spec#motivation)
---------------------------------------------------------------------------

Many applications require pro-rata distribution of tokens to existing holders (incentive programs, deterministic inflation, staking rewards). Building this into TIP-20 allows efficient distribution without forcing users to stake tokens elsewhere or requiring distributors to loop over all holders.

Specification[](https://docs.tempo.xyz/protocol/tip20-rewards/spec#specification)
---------------------------------------------------------------------------------

The rewards mechanism allows anyone to distribute token rewards to opted-in holders proportionally based on holdings. Users must opt in to receiving rewards and may delegate rewards to a recipient address.

TIP-20 Rewards Functions[](https://docs.tempo.xyz/protocol/tip20-rewards/spec#tip-20-rewards-functions)
-------------------------------------------------------------------------------------------------------

These functions are part of the [ITIP20](https://docs.tempo.xyz/protocol/tip20/spec) interface:

```
/// @notice Distribute rewards to opted-in token holders
/// @param amount Amount of tokens to distribute
function distributeReward(uint256 amount) external;
 
/// @notice Set the reward recipient for the caller (opt in/out of rewards)
/// @param newRewardRecipient Recipient address (address(0) to opt out)
function setRewardRecipient(address newRewardRecipient) external;
 
/// @notice Claim all pending rewards for the caller
/// @return maxAmount Amount claimed
function claimRewards() external returns (uint256 maxAmount);
 
/// @notice Get user reward info
function userRewardInfo(address user) external view returns (
    address rewardRecipient,
    uint256 rewardPerToken,
    uint256 rewardBalance
);
 
// State variables
function globalRewardPerToken() external view returns (uint256);
function optedInSupply() external view returns (uint128);
```


Accrual Mechanism[](https://docs.tempo.xyz/protocol/tip20-rewards/spec#accrual-mechanism)
-----------------------------------------------------------------------------------------

The system uses an accumulator pattern:

*   `globalRewardPerToken`: Cumulative rewards per token (scaled by 1e18)
*   Each user stores a `rewardPerToken` snapshot; pending rewards = `(globalRewardPerToken - snapshot) * balance`

Instant distributions (`seconds_ == 0`) add directly to `globalRewardPerToken` as: `deltaRPT = amount * 1e18 / optedInSupply`.

Opt-In Model[](https://docs.tempo.xyz/protocol/tip20-rewards/spec#opt-in-model)
-------------------------------------------------------------------------------

Users must call `setRewardRecipient(recipient)` to opt in. When opted in:

*   User's balance contributes to `optedInSupply`
*   Rewards accrue to `rewardBalance` on balance-changing operations
*   Users can delegate rewards to another address

Setting recipient to `address(0)` opts out.

TIP-403 Integration[](https://docs.tempo.xyz/protocol/tip20-rewards/spec#tip-403-integration)
---------------------------------------------------------------------------------------------

All token movements must pass TIP-403 policy checks:

*   `distributeReward`: Validates funder authorization
*   `setRewardRecipient`: Validates holder and recipient
*   `claimRewards`: Validates msg.sender

Invariants[](https://docs.tempo.xyz/protocol/tip20-rewards/spec#invariants)
---------------------------------------------------------------------------

*   `globalRewardPerToken` must monotonically increase
*   `optedInSupply` must equal the sum of balances for all opted-in users
*   All token movements must comply with TIP-403 policies
# Distribute Rewards ⋅ Tempo
Distribute rewards to token holders using TIP-20's built-in reward distribution mechanism. Rewards allow parties to incentivize holders of a token by distributing tokens proportionally based on their balance.

Rewards can be distributed by anyone on any TIP-20 token, and claimed by any holder who has opted in. This guide covers both the reward distributor and token holder use cases. While the demo below uses a token you create, the same principles apply to any token.

Demo[](https://docs.tempo.xyz/guide/issuance/distribute-rewards#demo)
---------------------------------------------------------------------

Try out the complete rewards flow: create a token, opt in to receive rewards on it, create a reward for yourself, and claim it.

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Create & deploy a token to testnet.

4

Grant issuer role on token.

5

Mint 100 tokens to yourself.

6

Opt in to receive token rewards.

7

Start a reward of 50 tokens.

Steps[](https://docs.tempo.xyz/guide/issuance/distribute-rewards#steps)
-----------------------------------------------------------------------

### \[Optional\] Create a Stablecoin[](https://docs.tempo.xyz/guide/issuance/distribute-rewards#optional-create-a-stablecoin)

If you would like to distribute rewards on a token you have created, follow the [Create a Stablecoin](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin) guide to deploy your token.

### Tell Your Users to Opt In to Rewards[](https://docs.tempo.xyz/guide/issuance/distribute-rewards#tell-your-users-to-opt-in-to-rewards)

Token holders must opt in to receive rewards by setting their reward recipient address. This is typically set to their own address.

```
import React from 'react'
import {  } from 'wagmi/tempo'
import {  } from 'wagmi'
 
export function () {
  const {  } = ()
  const  = '0x...' // Your token address
 
  const  = ..()
 
  const  = () => { 
    if (!) return
    .({ 
      : , 
      : , 
    }) 
  } 
 
  return (
    < 
      ={.}
      ={}
      ="button"
    >
      {. ? 'Opting in...' : 'Opt In to Rewards'}
    </>
  )
}
```


### Make a Reward Distribution[](https://docs.tempo.xyz/guide/issuance/distribute-rewards#make-a-reward-distribution)

Anyone can make a reward distribution that allocates tokens to all opted-in holders proportionally based on their balance.

```
import React from 'react'
import {  } from 'wagmi/tempo'
import {  } from 'viem'
 
export function () {
  const  = '0x...' // Your token address
 
  const { :  } = ..({
    : ,
  })
 
  const  = ..useStartSync()
 
  const  = () => { 
    if (!) return
    .mutate({ 
      : ('50', .), 
      : , 
    }) 
  } 
 
  return (
    <
      ={.isPending || !}
      ={}
      ="button"
    >
      {.isPending ? 'Starting...' : 'Start Reward'}
    </>
  )
}
```


### Your Users Can Claim Rewards[](https://docs.tempo.xyz/guide/issuance/distribute-rewards#your-users-can-claim-rewards)

Once a reward is distributed, opted-in holders can claim their share.

```
import React from 'react'
import {  } from 'wagmi/tempo'
 
export function () {
  const  = '0x...' // Your token address
 
  const  = ..()
 
  const  = () => { 
    .({ 
      : , 
    }) 
  } 
 
  return (
    <
      ={.}
      ={}
      ="button"
    >
      {. ? 'Claiming...' : 'Claim Rewards'}
    </>
  )
}
```


Recipes[](https://docs.tempo.xyz/guide/issuance/distribute-rewards#recipes)
---------------------------------------------------------------------------

### Watch for new reward distributions[](https://docs.tempo.xyz/guide/issuance/distribute-rewards#watch-for-new-reward-distributions)

Use `useWatchRewardDistributed` to listen for new reward distributions on a token. This is useful for updating your UI when a reward is distributed.

```
import {  } from 'wagmi/tempo'
 
function () {
  const  = '0x...' // Your token address
 
  ..({ 
    : , 
    () { 
      .('New reward scheduled:', ) 
      // Update UI, refetch balances, show notification, etc.
    }, 
  }) 
 
  return <>Watching for reward distributions...</>
}
```


### Watch for reward opt-ins[](https://docs.tempo.xyz/guide/issuance/distribute-rewards#watch-for-reward-opt-ins)

Use `useWatchRewardRecipientSet` to listen for when users opt in to rewards by setting their recipient address. This is useful for tracking opt-in activity.

```
import {  } from 'wagmi/tempo'
 
function () {
  const  = '0x...' // Your token address
 
  ..({ 
    : , 
    () { 
      .('User opted in:', ) 
      // Update UI, track analytics, etc.
    }, 
  }) 
 
  return <>Watching for reward opt-ins...</>
}
```


Learning Resources[](https://docs.tempo.xyz/guide/issuance/distribute-rewards#learning-resources)
-------------------------------------------------------------------------------------------------
