# Embed Passkey Accounts ⋅ Tempo
Create a domain-bound passkey account on Tempo using WebAuthn signatures for secure, passwordless authentication with [Tempo transactions](https://docs.tempo.xyz/protocol/transactions/spec-tempo-transaction).

Passkeys enable users to authenticate with biometrics (fingerprint, Face ID, Touch ID) they already use for other apps. Keys are stored in the device's secure enclave and sync across devices via iCloud Keychain or Google Password Manager.

Demo[](https://docs.tempo.xyz/guide/use-accounts/embed-passkeys#demo)
---------------------------------------------------------------------

By the end of this guide, you will be able to embed passkey accounts into your application.

Steps[](https://docs.tempo.xyz/guide/use-accounts/embed-passkeys#steps)
-----------------------------------------------------------------------

### Set up Wagmi[](https://docs.tempo.xyz/guide/use-accounts/embed-passkeys#set-up-wagmi)

Ensure that you have set up your project with Wagmi by following the [guide](https://docs.tempo.xyz/sdk/typescript#wagmi-setup).

### Configure the WebAuthn Connector[](https://docs.tempo.xyz/guide/use-accounts/embed-passkeys#configure-the-webauthn-connector)

Next, we will need to configure the `webAuthn` connector in our Wagmi config.

config.ts

```
import { ,  } from 'wagmi'
import {  } from 'viem/chains'
import { ,  } from 'wagmi/tempo'
 
export const  = ({
  : [],
  : [({ 
    : .(), 
  })], 
  : false,
  : {
    [tempo.id]: (),
  },
})
```


### Display Sign In Buttons[](https://docs.tempo.xyz/guide/use-accounts/embed-passkeys#display-sign-in-buttons)

After that, we will set up "Sign in" and "Sign up" buttons so that the user can create or use a passkey with the application.

We will create a new `Example.tsx` component to work in.

```
import { ,  } from 'wagmi'
 
export function () {
  const  = ()
  const [] = ()
 
  return (
    <>
      <
        ={() =>
          .({
            ,
            : { : 'sign-up' },
          })
        }
      >
        Sign up
      </>
 
      < ={() => .({  })}>
        Sign in
      </>
    </>
  )
}
```


### Display Account & Sign Out[](https://docs.tempo.xyz/guide/use-accounts/embed-passkeys#display-account--sign-out)

After the user has signed in, we can display the account information and a sign out button.

```
import { , , ,  } from 'wagmi'
 
export function () {
  const  = () 
  const  = ()
  const [] = ()
  const  = () 
 
  if (.) 
    return ( 
      <> {/* // [!code ++] */}
        <>{..(0, 6)}...{..(-4)}</> {/* // [!code ++] */}
        < ={() => .()}>Sign out</> {/* // [!code ++] */}
      </> 
    ) 
 
  return (
    <>
      <
        ={() =>
          .({
            ,
            : { : 'sign-up' },
          })
        }
      >
        Sign up
      </>
 
      < ={() => .({  })}>
        Sign in
      </>
    </>
  )
}
```


### Next Steps[](https://docs.tempo.xyz/guide/use-accounts/embed-passkeys#next-steps)

Best Practices[](https://docs.tempo.xyz/guide/use-accounts/embed-passkeys#best-practices)
-----------------------------------------------------------------------------------------

### Loading State[](https://docs.tempo.xyz/guide/use-accounts/embed-passkeys#loading-state)

When the user is logging in or signing out, we should show loading state to indicate that the process is happening.

We can use the `isPending` property from the `useConnect` hook to show pending state to the user.

Example.tsx

```
import { , , ,  } from 'wagmi'
 
export function () {
  const  = ()
  const  = ()
  const [] = ()
  const  = ()
 
  if (.) 
    return <>Check prompt...</> 
  return (/* ... */)
}
```


### Error Handling[](https://docs.tempo.xyz/guide/use-accounts/embed-passkeys#error-handling)

If an error unexpectedly occurs, we should display an error message to the user.

We can use the `error` property from the `useConnect` hook to show error state to the user.

Example.tsx

```
import { , , ,  } from 'wagmi'
 
export function () {
  const  = ()
  const  = ()
  const [] = ()
  const  = ()
 
  if (.) 
    return <>Error: {..}</> 
  return (/* ... */)
}
```


Learning Resources[](https://docs.tempo.xyz/guide/use-accounts/embed-passkeys#learning-resources)
-------------------------------------------------------------------------------------------------