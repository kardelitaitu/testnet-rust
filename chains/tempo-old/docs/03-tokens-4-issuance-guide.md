# Create a Stablecoin ⋅ Tempo
Create your own stablecoin on Tempo using the [TIP-20 token standard](https://docs.tempo.xyz/protocol/tip20/overview). TIP-20 tokens are designed specifically for payments with built-in compliance features, role-based permissions, and integration with Tempo's payment infrastructure.

Demo[](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin#demo)
----------------------------------------------------------------------

By the end of this guide, you will be able to create a stablecoin on Tempo.

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Create & deploy a token to testnet.

Steps[](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin#steps)
------------------------------------------------------------------------

### Set up Wagmi & integrate accounts[](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin#set-up-wagmi--integrate-accounts)

### Add testnet funds¹[](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin#add-testnet-funds)

Before we send off a transaction to deploy our stablecoin to the Tempo testnet, we need to make sure our account is funded with a stablecoin to cover the transaction fee.

As we have configured our project to use `AlphaUSD` (`0x20c000…0001`) as the [default fee token](https://docs.tempo.xyz/quickstart/integrate-tempo#default-fee-token), we will need to add some `AlphaUSD` to our account.

Luckily, the built-in Tempo testnet faucet supports funding accounts with `AlphaUSD`.

#### Add Funds

1

Add testnet funds to your account.

```
import {  } from 'wagmi/tempo'
import {  } from 'wagmi'
 
export function () {
  const {  } = ()
  const  = ..()
 
  return (
    < ={() => .({ :  })}>
      Add Funds
    </>
  )
}
```


### Add form fields[](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin#add-form-fields)

Now that we have some funds to cover the transaction fee in our account, we can create a stablecoin.

Let's create a new component and add some input fields for the **name** and **symbol** of our stablecoin, as shown in the demo.

1

Add testnet funds to your account.

2

Create & deploy a token to testnet.

```
export function () {
  return (
    <>
      <
        ={() => {
          .()
          const  = new (. as HTMLFormElement)
          const  = .('name') as string
          const  = .('symbol') as string
        }}
      >
        < ="text" ="name" ="demoUSD"  />
        < ="text" ="symbol" ="DEMO"  />
        < ="submit">
          Create
        </>
      </>
    </>
  )
}
```


### Add submission logic[](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin#add-submission-logic)

Now that we have some input fields, we need to add some logic to handle the submission of the form to create the stablecoin.

After this step, your users will be able to create a stablecoin by clicking the "Create" button!

1

Create & deploy a token to testnet.

```
import {  } from 'wagmi/tempo'
 
export function () {
  const  = ..() 
 
  return (
    <>
      <
        ={() => {
          .()
          const  = new (. as HTMLFormElement)
          const  = .('name') as string
          const  = .('symbol') as string
          .({ 
            , 
            , 
            : 'USD', 
          }) 
        }}
      >
        < ="text" ="name" ="demoUSD"  />
        < ="text" ="symbol" ="DEMO"  />
        < ="submit">
          Create
        </>
      </>
    </>
  )
}
```


### Add success state[](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin#add-success-state)

Now that users can submit the form and create a stablecoin, let's add a basic success state to display the name of the stablecoin and a link to the transaction receipt.

```
import {  } from 'wagmi/tempo' 
 
export function () {
  const  = ..() 
 
  return (
    <>
      <
        ={() => {
          .()
          const  = new (. as HTMLFormElement)
          const  = .('name') as string
          const  = .('symbol') as string
          .({ 
            , 
            , 
            : 'USD', 
          }) 
        }}
      >
        < ="text" ="name" ="demoUSD"  />
        < ="text" ="symbol" ="DEMO"  />
        < ="submit">
          Create
        </>
      </>
 
      {. && ( 
        <> {/* // [!code ++] */}
          {..} created successfully! {/* // [!code ++] */}
          < ={`https://explore.tempo.xyz/tx/${...}`}> {/* // [!code ++] */}
            View receipt {/* // [!code ++] */}
          </> {/* // [!code ++] */}
        </> /* // */
      )} {/* // [!code ++] */}
    </>
  )
}
```


### Next steps[](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin#next-steps)

Now that you have created your first stablecoin, you can now:

*   learn the [Best Practices](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin#best-practices) below
*   follow a guide on how to [mint](https://docs.tempo.xyz/guide/issuance/mint-stablecoins) and [more](https://docs.tempo.xyz/guide/issuance/manage-stablecoin) with your stablecoin.

Best Practices[](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin#best-practices)
------------------------------------------------------------------------------------------

### Loading State[](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin#loading-state)

When the user is creating a stablecoin, we should show loading state to indicate that the process is happening.

We can use the `isPending` property from the `useCreateSync` hook to show pending state to the user on our "Create" button.

```
<button 
  disabled={create.isPending}
  type="submit"
>
  {create.isPending ? 'Creating...' : 'Create'} {/* // [!code ++] */}
</button>
```


### Error Handling[](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin#error-handling)

If an error unexpectedly occurs, we should display an error message to the user.

We can use the `error` property from the `useCreateSync` hook to show error state to the user.

```
export function () {
  // ...
 
  if (create.error) 
    return <>Error: {create.error.message}</> 
 
  // ...
}
```


Learning Resources[](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin#learning-resources)
--------------------------------------------------------------------------------------------------
# Mint Stablecoins ⋅ Tempo
Create new tokens by minting them to a specified address. Minting increases the total supply of your stablecoin.

Steps[](https://docs.tempo.xyz/guide/issuance/mint-stablecoins#steps)
---------------------------------------------------------------------

### Create a Stablecoin[](https://docs.tempo.xyz/guide/issuance/mint-stablecoins#create-a-stablecoin)

Before you can mint tokens, you need to create a stablecoin. Follow the [Create a Stablecoin](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin) guide to deploy your token.

Once you've created your token, you can proceed to grant the issuer role and mint tokens.

### Grant the Issuer Role[](https://docs.tempo.xyz/guide/issuance/mint-stablecoins#grant-the-issuer-role)

Assign the issuer role to the address that will mint tokens. Minting requires the **`ISSUER_ROLE`**.

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Create & deploy a token to testnet.

4

Grant issuer role on token.

```
import React from 'react'
import {  } from 'wagmi/tempo'
import {  } from '@tanstack/react-query'
 
export function () {
  const  = ()
  const  = '0x...' // Your token address
  const  = '0x...' // Address to grant the issuer role
 
  const  = ..({ 
    : { 
      () { 
        .({ : ['hasRole'] }) 
      }, 
    }, 
  }) 
 
  const  = async () => { 
    await .({ 
      : , 
      : ['issuer'], 
      : , 
      : '0x20c0000000000000000000000000000000000001', 
    }) 
  } 
 
  return (
    <
      ={.}
      ={}
      ="button"
    > 
      {. ? 'Granting...' : 'Grant Issuer Role'}
    </> 
  )
}
```


### Mint Tokens to a Recipient[](https://docs.tempo.xyz/guide/issuance/mint-stablecoins#mint-tokens-to-a-recipient)

Now that the issuer role is granted, you can mint tokens to any address.

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

```
import React from 'react'
import {  } from 'wagmi/tempo'
import {  } from 'wagmi'
import { , ,  } from 'viem'
import {  } from '@tanstack/react-query'
 
export function () {
  const {  } = ()
  const  = ()
  const  = '0x...' // Your token address
  const [, ] = React.<string>('')
  const [, ] = React.<string>('')
 
  const { :  } = ..({ 
    : , 
  }) 
 
  const  = ..({ 
    : { 
      () { 
        .({ : ['getBalance'] }) 
      }, 
    }, 
  }) 
 
  const  = () => { 
    if (! || ! || !) return
    .({ 
      : ('100', .), 
      :  as `0x${string}`, 
      : , 
      :  ? ((), { : 32 }) : , 
      : '0x20c0000000000000000000000000000000000001', 
    }) 
  } 
 
  return (
    <>
      <>
        <>Recipient address</>
        <
          ="text"
          ={}
          ={() => (..)}
          ="0x..."
        />
      </>
      <>
        <>Memo (optional)</>
        <
          ="text"
          ={}
          ={() => (..)}
          ="INV-12345"
        />
      </>
      <
        ={! || .}
        ={}
        ="button"
      > 
        {. ? 'Minting...' : 'Mint'}
      </> 
    </>
  )
}
```


Recipes[](https://docs.tempo.xyz/guide/issuance/mint-stablecoins#recipes)
-------------------------------------------------------------------------

### Burning Stablecoins[](https://docs.tempo.xyz/guide/issuance/mint-stablecoins#burning-stablecoins)

To decrease supply, you can burn tokens from your own balance. Burning requires the **`ISSUER_ROLE`** and sufficient balance in the caller's account.

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

Burn 100 tokens from yourself.

```
import React from 'react'
import {  } from 'wagmi/tempo'
import {  } from 'wagmi'
import { , ,  } from 'viem'
import {  } from '@tanstack/react-query'
 
export function () {
  const {  } = ()
  const  = ()
  const  = '0x...' // Your token address
  const [, ] = React.<string>('')
 
  const { :  } = ..({ 
    : , 
  }) 
 
  const  = ..({ 
    : { 
      () { 
        .({ : ['getBalance'] }) 
      }, 
    }, 
  }) 
 
  const  = () => { 
    if (! || ! || !) return
    .({ 
      : ('100', .), 
      : , 
      :  ? ((), { : 32 }) : , 
      : '0x20c0000000000000000000000000000000000001', 
    }) 
  } 
 
  return (
    <>
      <>
        <>Memo (optional)</>
        <
          ="text"
          ={}
          ={() => (..)}
          ="INV-12345"
        />
      </>
      <
        ={! || .}
        ={}
        ="button"
      > 
        {. ? 'Burning...' : 'Burn'}
      </> 
    </>
  )
}
```


Best Practices[](https://docs.tempo.xyz/guide/issuance/mint-stablecoins#best-practices)
---------------------------------------------------------------------------------------

### Monitor Supply Caps[](https://docs.tempo.xyz/guide/issuance/mint-stablecoins#monitor-supply-caps)

If your token has a supply cap set, any `mint()` or `mintWithMemo()` call that would exceed the cap will revert with `SupplyCapExceeded()`. You must either:

*   Burn tokens to reduce total supply below the cap
*   Increase the supply cap (requires `DEFAULT_ADMIN_ROLE`)
*   Remove the cap entirely by setting it to `type(uint256).max`

Use `[getMetadata](https://viem.sh/tempo/actions/token.getMetadata)` to check your token's total supply before minting.

### Role Separation[](https://docs.tempo.xyz/guide/issuance/mint-stablecoins#role-separation)

Assign the issuer role to dedicated treasury or minting addresses separate from your admin address. This enhances security by limiting the privileges of any single address.

Learning Resources[](https://docs.tempo.xyz/guide/issuance/mint-stablecoins#learning-resources)
-----------------------------------------------------------------------------------------------
# Manage Your Stablecoin ⋅ Tempo
Configure your stablecoin's permissions, supply limits, and compliance policies after deployment. This guide covers granting roles to manage token operations, setting supply caps, configuring transfer policies, and controlling token transfers through pause/unpause functionality.

TIP-20 tokens use a role-based access control system that allows you to delegate different administrative functions to different addresses. For detailed information about the role system, see the [TIP-20 specification](https://docs.tempo.xyz/protocol/tip20/spec#role-based-access-control).

Steps[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#steps)
----------------------------------------------------------------------

In this guide, we'll walk through how to assign and check the **`issuer`** role, but the process is identical for other roles like `pause`, `unpause`, `burnBlocked`, and `defaultAdmin`.

### Setup[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#setup)

Before you can manage roles on your stablecoin, you need to create one. Follow the [Create a Stablecoin](https://docs.tempo.xyz/guide/issuance/create-a-stablecoin) guide to deploy your token.

Once you've created your token, you can proceed to grant roles to specific addresses.

### Grant Roles to an Address[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#grant-roles-to-an-address)

Assign roles to specific addresses to delegate token management capabilities.

#### Grant Roles to an Address

demo

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Create & deploy a token to testnet.

4

Grant issuer role on token.

```
import React from 'react'
import {  } from 'wagmi/tempo'
import {  } from '@tanstack/react-query'
 
export function () {
  const  = ()
  const  = '0x...' // Your token address
  const  = '0x...' // Address to grant the issuer role
 
  const  = ..({}) 
 
  const  = async () => { 
    await .({ 
      : , 
      : ['issuer'], 
      : , 
    }) 
  } 
 
  return (
    <
      ={.}
      ={}
      ="button"
    > 
      {. ? 'Granting...' : 'Grant Issuer Role'}
    </> {}
  )
}
```


### Check if an Address Has a Role[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#check-if-an-address-has-a-role)

Use `hasRole` to verify whether an address has been granted a specific role.

```
import React from 'react'
import {  } from 'wagmi/tempo'
import {  } from '@tanstack/react-query'
 
export function () {
  const  = ()
  const  = '0x...' // Your token address
  const  = '0x...' // Address to grant the issuer role
 
  // Grant the issuer role
  const  = ..({
    : { 
      () { 
        .({ : ['hasRole'] }) 
      }, 
    }, 
  })
 
  const  = async () => {
    await .({
      : ,
      : ['issuer'],
      : ,
    })
  }
 
  const { :  } = ..({ 
    : , 
    : , 
    : 'issuer', 
  }) 
 
  return (
    <>
      <
        ={.}
        ={}
        ="button"
      >
        {. ? 'Granting...' : 'Grant Issuer Role'}
      </>
 
      { !==  && ( 
        <> 
          Treasury { ? 'has' : 'does not have'} the issuer role 
        </> {}
      )}
    </>
  )
}
```


### Revoke the Issuer Role[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#revoke-the-issuer-role)

Revoke roles from addresses when you need to remove their permissions.

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Create & deploy a token to testnet.

4

Grant issuer role on token.

5

Revoke issuer role on token.

```
import React from 'react'
import {  } from 'wagmi/tempo'
import {  } from '@tanstack/react-query'
 
export function () {
  const  = ()
  const  = '0x...' // Your token address
  const  = '0x...' // Address to grant/revoke the issuer role
 
  // Grant the issuer role
  const  = ..({
    : {
      () {
        .({ : ['hasRole'] })
      },
    },
  })
 
  const  = async () => {
    await .({
      : ,
      : ['issuer'],
      : ,
    })
  }
 
  // Check if the treasury has the issuer role
  const { :  } = ..({
    : ,
    : ,
    : 'issuer',
  })
 
  // Revoke the issuer role
  const  = ..({ 
    : { 
      () { 
        .({ : ['hasRole'] }) 
      }, 
    }, 
  }) 
 
  const  = async () => { 
    await .({ 
      : , 
      : ['issuer'], 
      : , 
    }) 
  } 
 
  return (
    <>
      <
        ={.}
        ={}
        ="button"
      >
        {. ? 'Granting...' : 'Grant Issuer Role'}
      </>
 
      <
        ={. || !}
        ={}
        ="button"
      > 
        {. ? 'Revoking...' : 'Revoke Issuer Role'}
      </> 
 
      { !==  && (
        <>
          Treasury { ? 'has' : 'does not have'} the issuer role
        </>
      )}
    </>
  )
}
```


Recipes[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#recipes)
--------------------------------------------------------------------------

### Set Supply Cap[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#set-supply-cap)

Limit the maximum total supply of your token. Setting supply caps requires the **`DEFAULT_ADMIN_ROLE`**. The new cap cannot be less than the current total supply.

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Create & deploy a token to testnet.

4

Set supply cap to 1,000 tokens.

```
import React from 'react'
import {  } from 'wagmi/tempo'
import {  } from 'viem'
 
export function () {
  const  = '0x...' // Your token address
 
  const { : , :  } =
    ..({ :  }) 
 
  const  = ..({ 
    : { : () => () }, 
  }) 
 
  const  = () => { 
    .({ 
      : , 
      : ('1000', ?. || 6), 
    }) 
  } 
 
  return (
    <
      ={.}
      ={}
      ="button"
    > 
      {. ? 'Setting...' : 'Set Cap'}
    </> {}
  )
}
```


### Configure Transfer Policies[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#configure-transfer-policies)

Control who can send and receive your stablecoin for compliance and regulatory requirements. Setting transfer policies requires the **`DEFAULT_ADMIN_ROLE`**.

Transfer policies can be:

*   **Always allow**: Anyone can send/receive (default)
*   **Always reject**: Nobody can send/receive
*   **Whitelist**: Only authorized addresses can send/receive
*   **Blacklist**: Blocked addresses cannot send/receive

Learn more about configuring transfer policies in the [TIP-403 specification](https://docs.tempo.xyz/protocol/tip403/spec).

#### Create and Link Transfer Policy

demo

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Create & deploy a token to testnet.

4

Create a transfer policy.

5

Link the policy to your token.

```
import React from 'react'
import {  } from 'wagmi/tempo'
 
export function () {
  const  = '0x...' // Your token address
 
  const  = ..({ 
    : { 
      () { 
        // Store policyId for the next step
        .('Policy ID:', .) 
      }, 
    }, 
  }) 
 
  const  = async () => { 
    await .({ 
      : [ 
        '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEbb', 
      ], 
      : 'blacklist', 
    }) 
  } 
 
  return (
    <
      ={.}
      ={}
      ="button"
    > 
      {. ? 'Creating...' : 'Create Policy'}
    </> 
  )
}
```


### Pause and Unpause Token Transfers[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#pause-and-unpause-token-transfers)

Temporarily halt all token transfers during emergency situations or maintenance windows. Pausing transfers requires the **`PAUSE_ROLE`**. Unpausing transfers requires the **`UNPAUSE_ROLE`**.

#### Pause and Unpause Your Token

demo

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Create & deploy a token to testnet.

4

Grant pause, unpause roles on token.

5

Pause transfers for token.

```
import React from 'react'
import {  } from 'wagmi/tempo'
 
export function () {
  const  = '0x...' // Your token address
 
  const { : , :  } =
    ..({ :  }) 
 
  const  = ..({ 
    : { : () => () }, 
  }) 
 
  const  = ..({ 
    : { : () => () }, 
  }) 
 
  const  = ?. || false
 
  const  = () => { 
    if () { 
      .({ 
        : , 
      }) 
    } else { 
      .({ 
        : , 
      }) 
    } 
  } 
 
  const  = . || . 
 
  return (
    <
      ={}
      ={}
      ="button"
    > 
      { ? 'Processing...' :  ? 'Unpause' : 'Pause'}
    </> 
  )
}
```


### Using the Burn Blocked Role[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#using-the-burn-blocked-role)

The Burn Blocked role allows your team to burn tokens from blocked or frozen addresses. This is useful for regulatory compliance when you need to remove tokens from addresses that violate terms of service or legal requirements.

#### Create and Link Transfer Policy

demo

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Create & deploy a token to testnet.

4

Grant issuer, burnBlocked roles on token.

5

Mint 100 tokens to recipient.

6

Create a transfer policy.

7

Link the policy to your token.

8

Burn 100 tokens from blocked address.

```
import React from 'react'
import {  } from 'wagmi/tempo'
import {  } from 'viem'
 
export function () {
  const  = '0x...' // Your token address
  const  = '0x...' // The blocked address to burn tokens from
 
  const { :  } = ..({ 
    : , 
  }) 
 
  const  = ..() 
 
  const  = async () => { 
    if (!) return
 
    await .({ 
      : , 
      : , 
      : ('100', .), 
    }) 
  } 
 
  return (
    <
      ={.}
      ={}
      ="button"
    > 
      {. ? 'Burning...' : 'Burn Blocked Tokens'}
    </> 
  )
}
```


Best Practices[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#best-practices)
----------------------------------------------------------------------------------------

### Role Separation[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#role-separation)

Use different addresses for different roles to enhance security. For example, assign the issuer role to your treasury address for minting, and the pause role to your security team for emergency controls.

### Event Monitoring[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#event-monitoring)

Monitor onchain events for role changes, mints, burns, and administrative actions to maintain visibility into token operations and detect unauthorized activities.

### Emergency Procedures[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#emergency-procedures)

Ensure pause and unpause roles are assigned to trusted addresses and that your team has documented procedures for responding to security incidents requiring token transfers to be halted.

Learning Resources[](https://docs.tempo.xyz/guide/issuance/manage-stablecoin#learning-resources)
------------------------------------------------------------------------------------------------
