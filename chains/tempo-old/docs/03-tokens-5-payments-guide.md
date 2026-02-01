# Send a Payment ⋅ Tempo
Send stablecoin payments between accounts on Tempo. Payments can include optional memos for reconciliation and tracking.

Demo[](https://docs.tempo.xyz/guide/payments/send-a-payment#demo)
-----------------------------------------------------------------

By the end of this guide you will be able to send payments on Tempo with an optional memo.

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Send 100 AlphaUSD to a recipient.

Steps[](https://docs.tempo.xyz/guide/payments/send-a-payment#steps)
-------------------------------------------------------------------

### Set up Wagmi & integrate accounts[](https://docs.tempo.xyz/guide/payments/send-a-payment#set-up-wagmi--integrate-accounts)

### Add testnet funds¹[](https://docs.tempo.xyz/guide/payments/send-a-payment#add-testnet-funds)

Before you can send a payment, you need to fund your account. In this guide you will be sending `AlphaUSD` (`0x20c000…0001`).

The built-in Tempo testnet faucet funds accounts with `AlphaUSD`.

#### Add Funds

1

Add testnet funds to your account.

```
import {  } from 'wagmi/tempo'
import {  } from 'wagmi'
 
function () {
  const {  } = ()
  const { ,  } = ..()
 
  return (
    < ={() => ({ :  })} ={}>
      Add Funds
    </>
  )
}
```


### Add send payment logic[](https://docs.tempo.xyz/guide/payments/send-a-payment#add-send-payment-logic)

Now that you have `AlphaUSD` you are ready to add logic to send a payment with an optional memo.

#### Send Payment

1

Add testnet funds to your account.

2

Send 100 AlphaUSD to a recipient.

```
import {  } from 'wagmi/tempo'
import { , ,  } from 'viem'
 
function () {
  const  = ..() 
 
  return (
    < ={
      () => {
        .()
        const  = new (. as HTMLFormElement)
 
        const  = (.('recipient') ||
          '0x0000000000000000000000000000000000000000') as `0x${string}`
        const  = (.('memo') || '') as string
 
        .({ 
          : ('100', 6), 
          : , 
          : '0x20c0000000000000000000000000000000000001', 
          :  ? ((), { : 32 }) : , 
        }) 
      }
    }>
      <>
        < ="recipient">Recipient address</>
        < ="text" ="recipient" ="0x..." />
      </>
      <>
        < ="memo">Memo (optional)</>
        < ="text" ="memo" ="Optional memo" />
      </>
      <
        ="submit"
        ={.}
      > 
        Send Payment 
      </> 
    </>
  )
}
```


### Display receipt[](https://docs.tempo.xyz/guide/payments/send-a-payment#display-receipt)

Now that you can send a payment, you can display the transaction receipt on success.

```
import {  } from 'wagmi/tempo'
import { , ,  } from 'viem'
 
function () {
  const  = ..()
 
  return (
    <>
      {/* ... your payment form ... */}
      {. && ( 
        < ={`https://explore.tempo.xyz/tx/${...}`}> {/* // [!code ++] */}
          View receipt {/* // [!code ++] */}
        </> 
      )} {/* // [!code ++] */}
    </>
  )
}
```


### Next steps[](https://docs.tempo.xyz/guide/payments/send-a-payment#next-steps)

Recipes[](https://docs.tempo.xyz/guide/payments/send-a-payment#recipes)
-----------------------------------------------------------------------

### Basic transfer[](https://docs.tempo.xyz/guide/payments/send-a-payment#basic-transfer)

Send a payment using the standard `transfer` function:

```
import { parseUnits } from 'viem'
import { client } from './viem.config'
 
const { receipt } = await client.token.transferSync({
  amount: parseUnits('100', 6), // 100 tokens (6 decimals)
  to: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEbb',
  token: '0x20c0000000000000000000000000000000000001', // AlphaUSD
})
```


### Transfer with memo[](https://docs.tempo.xyz/guide/payments/send-a-payment#transfer-with-memo)

Include a memo for payment reconciliation and tracking:

```
import { parseUnits } from 'viem'
import { client } from './viem.config'
 
const invoiceId = pad(stringToHex('INV-12345'), { size: 32 })
 
const { receipt } = await client.token.transferSync({
  amount: parseUnits('100', 6),
  to: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEbb',
  token: '0x20c0000000000000000000000000000000000001',
  memo: invoiceId,
})
```


The memo is a 32-byte value that can store payment references, invoice IDs, order numbers, or any other metadata.

### Using Solidity[](https://docs.tempo.xyz/guide/payments/send-a-payment#using-solidity)

If you're building a smart contract that sends payments:

```
interface ITIP20 {
    function transfer(address to, uint256 amount) external returns (bool);
    function transferWithMemo(address to, uint256 amount, bytes32 memo) external;
}
 
contract PaymentSender {
    ITIP20 public token;
    
    function sendPayment(address recipient, uint256 amount) external {
        token.transfer(recipient, amount);
    }
    
    function sendPaymentWithMemo(
        address recipient, 
        uint256 amount, 
        bytes32 invoiceId
    ) external {
        token.transferWithMemo(recipient, amount, invoiceId);
    }
}
```


### Batch payment transactions[](https://docs.tempo.xyz/guide/payments/send-a-payment#batch-payment-transactions)

Send multiple payments in a single transaction using batch transactions:

```
import { encodeFunctionData, parseUnits } from 'viem'
import { Abis } from 'viem/tempo'
import { client } from './viem.config'
 
const tokenABI = Abis.tip20
const recipient1 = '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEbb'
const recipient2 = '0x70997970C51812dc3A010C7d01b50e0d17dc79C8'
 
const calls = [
  {
    to: '0x20c0000000000000000000000000000000000001', // AlphaUSD address
    data: encodeFunctionData({
      abi: tokenABI,
      functionName: 'transfer',
      args: [recipient1, parseUnits('100', 6)],
    }),
  },
  {
    to: '0x20c0000000000000000000000000000000000001',
    data: encodeFunctionData({
      abi: tokenABI,
      functionName: 'transfer',
      args: [recipient2, parseUnits('50', 6)],
    }),
  },
]
 
const hash = await client.sendTransaction({ calls })
```


### Payment events[](https://docs.tempo.xyz/guide/payments/send-a-payment#payment-events)

When you send a payment, the token contract emits events:

*   **Transfer**: Standard ERC-20 transfer event
*   **TransferWithMemo**: Additional event with memo (if using `transferWithMemo`)

You can filter these events to track payments in your off-chain systems.

Best practices[](https://docs.tempo.xyz/guide/payments/send-a-payment#best-practices)
-------------------------------------------------------------------------------------

### Loading state[](https://docs.tempo.xyz/guide/payments/send-a-payment#loading-state)

Users should see a loading state when the payment is being processed.

You can use the `isPending` property from the `useTransferSync` hook to show pending state to the user.

### Error handling[](https://docs.tempo.xyz/guide/payments/send-a-payment#error-handling)

If an error unexpectedly occurs, you can display an error message to the user by using the `error` property from the `useTransferSync` hook.

```
import { Hooks } from 'wagmi/tempo'
 
function SendPayment() {
  const sendPayment = Hooks.token.useTransferSync()
 
  return (
    <>
      {/* ... your paymentform ... */}
      {sendPayment.error && <div>Error: {sendPayment.error.message}</div>}
    </>
  )
}
```


Learning resources[](https://docs.tempo.xyz/guide/payments/send-a-payment#learning-resources)
---------------------------------------------------------------------------------------------
# Accept a Payment ⋅ Tempo
Accept stablecoin payments in your application. Learn how to receive payments, verify transactions, and reconcile payments using memos.

Receiving Payments[](https://docs.tempo.xyz/guide/payments/accept-a-payment#receiving-payments)
-----------------------------------------------------------------------------------------------

Payments are automatically credited to the recipient's address when a transfer is executed. You don't need to do anything special to "accept" a payment, it happens automatically onchain.

In this basic receiving demo you can see the balances update after you add funds to your account, using the `getBalance` and `watchEvent` calls documented below.

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

Verifying Payments[](https://docs.tempo.xyz/guide/payments/accept-a-payment#verifying-payments)
-----------------------------------------------------------------------------------------------

Check if a payment has been received by querying the token balance or listening for transfer events:

### Check Balance[](https://docs.tempo.xyz/guide/payments/accept-a-payment#check-balance)

```
import { client } from './viem.config'
 
const balance = await client.token.getBalance({
  token: '0x20c0000000000000000000000000000000000001', // AlphaUSD
  address: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEbb',
})
 
console.log('Balance:', balance)
```


### Listen for Transfer Events[](https://docs.tempo.xyz/guide/payments/accept-a-payment#listen-for-transfer-events)

```
import { watchEvent } from 'viem'
 
// Watch for incoming transfers
const unwatch = watchEvent(client, {
  address: '0x20c0000000000000000000000000000000000001',
  event: {
    type: 'event',
    name: 'Transfer',
    inputs: [
      { name: 'from', type: 'address', indexed: true },
      { name: 'to', type: 'address', indexed: true },
      { name: 'value', type: 'uint256' },
    ],
  },
  onLogs: (logs) => {
    logs.forEach((log) => {
      if (log.args.to === yourAddress) {
        console.log('Received payment:', {
          from: log.args.from,
          amount: log.args.value,
        })
      }
    })
  },
})
```


Payment Reconciliation with Memos[](https://docs.tempo.xyz/guide/payments/accept-a-payment#payment-reconciliation-with-memos)
-----------------------------------------------------------------------------------------------------------------------------

If payments include memos (invoice IDs, order numbers, etc.), you can reconcile them automatically:

```
// Watch for TransferWithMemo events
const unwatch = watchEvent(client, {
  address: tokenAddress,
  event: {
    type: 'event',
    name: 'TransferWithMemo',
    inputs: [
      { name: 'from', type: 'address', indexed: true },
      { name: 'to', type: 'address', indexed: true },
      { name: 'value', type: 'uint256' },
      { name: 'memo', type: 'bytes32', indexed: true },
    ],
  },
  onLogs: (logs) => {
    logs.forEach((log) => {
      if (log.args.to === yourAddress) {
        const invoiceId = log.args.memo
        // Mark invoice as paid in your database
        markInvoiceAsPaid(invoiceId, log.args.value)
      }
    })
  },
})
```


Smart Contract Integration[](https://docs.tempo.xyz/guide/payments/accept-a-payment#smart-contract-integration)
---------------------------------------------------------------------------------------------------------------

If you're building a smart contract that accepts payments:

```
contract PaymentReceiver {
    ITIP20 public token;
    mapping(bytes32 => bool) public paidInvoices;
    
    event PaymentReceived(
        address indexed payer,
        uint256 amount,
        bytes32 indexed invoiceId
    );
    
    function receivePayment(
        address payer,
        uint256 amount,
        bytes32 invoiceId
    ) external {
        require(!paidInvoices[invoiceId], "Invoice already paid");
        
        // Transfer tokens from payer to this contract
        token.transferFrom(payer, address(this), amount);
        
        paidInvoices[invoiceId] = true;
        emit PaymentReceived(payer, amount, invoiceId);
    }
}
```


Payment Verification Best Practices[](https://docs.tempo.xyz/guide/payments/accept-a-payment#payment-verification-best-practices)
---------------------------------------------------------------------------------------------------------------------------------

1.  **Verify onchain**: Always verify payments onchain before marking orders as paid
2.  **Use memos**: Request memos from payers to link payments to invoices or orders
3.  **Check confirmations**: Wait for transaction finality (~1 second on Tempo) before processing
4.  **Handle edge cases**: Account for partial payments, refunds, and failed transactions

Cross-Stablecoin Payments[](https://docs.tempo.xyz/guide/payments/accept-a-payment#cross-stablecoin-payments)
-------------------------------------------------------------------------------------------------------------

If you need to accept payments in a specific stablecoin but receive a different one, use the exchange to swap:

```
// User sends USDC, but you need USDT
// Swap USDC to USDT using the exchange
const { receipt } = await client.dex.sellSync({
  tokenIn: usdcAddress,
  tokenOut: usdtAddress,
  amountIn: receivedAmount,
  minAmountOut: receivedAmount * 99n / 100n, // 1% slippage
})
```


Next Steps[](https://docs.tempo.xyz/guide/payments/accept-a-payment#next-steps)
-------------------------------------------------------------------------------

*   **[Send a payment](https://docs.tempo.xyz/guide/payments/send-a-payment)** to learn how to send payments
*   Learn more about [Exchange](https://docs.tempo.xyz/guide/stablecoin-dex) for cross-stablecoin payments
# Attach a Transfer Memo ⋅ Tempo
Attach 32-byte references to [TIP-20](https://docs.tempo.xyz/protocol/tip20/overview) transfers for payment reconciliation. Use memos to link onchain transactions to your internal records—customer IDs, invoice numbers, or any identifier that helps you match payments to your database.

Demo[](https://docs.tempo.xyz/guide/payments/transfer-memos#demo)
-----------------------------------------------------------------

1

Create an account, or use an existing one.

2

Add testnet funds to your account.

3

Send a payment with a memo for reconciliation.

Steps[](https://docs.tempo.xyz/guide/payments/transfer-memos#steps)
-------------------------------------------------------------------

### Set up your project[](https://docs.tempo.xyz/guide/payments/transfer-memos#set-up-your-project)

### Send a transfer with memo[](https://docs.tempo.xyz/guide/payments/transfer-memos#send-a-transfer-with-memo)

Use `transferWithMemo` to attach a reference to your payment. The memo is a 32-byte value that gets emitted in the `TransferWithMemo` event.

```
import {  } from 'wagmi/tempo'
import { ,  } from 'viem'
import {  } from 'wagmi'
 
export function () {
  const {  } = ()
  const  = ..()
 
  const  = () => {
    .({
      : '0x20c0000000000000000000000000000000000001',
      : '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEbb',
      : ('100', 6),
      : ('INV-12345', { : 32 }),
    })
  }
 
  return (
    < ={} ={.}>
      {. ? 'Sending...' : 'Send Payment'}
    </>
  )
}
```


### Watch for transfers with memos[](https://docs.tempo.xyz/guide/payments/transfer-memos#watch-for-transfers-with-memos)

Listen for `TransferWithMemo` events to reconcile incoming payments. The memo is indexed, so you can filter by specific values.

```
import {  } from 'wagmi'
import {  } from 'viem'
import {  } from 'viem/tempo'
 
export function ({  }: { : `0x${string}` }) {
  ({
    : '0x20c0000000000000000000000000000000000001',
    : .TIP20,
    : 'TransferWithMemo',
    : () => {
      for (const  of ) {
        if (.args.to === ) {
          const  = (.args.memo, 'string').(/\0/g, '')
          .(`Received ${.args.value} with memo: ${}`)
        }
      }
    },
  })
 
  return <>Watching for deposits...</>
}
```


Recipes[](https://docs.tempo.xyz/guide/payments/transfer-memos#recipes)
-----------------------------------------------------------------------

### Exchange deposit reconciliation[](https://docs.tempo.xyz/guide/payments/transfer-memos#exchange-deposit-reconciliation)

As an exchange, use a single master hot wallet for all customer deposits. Customers include their customer ID as the memo, and you credit their account by parsing the event.

```
import { Actions } from 'viem/tempo'
import { parseUnits, stringToHex, pad } from 'viem'
 
// Customer deposits with their customer ID
await Actions.token.transferSync(walletClient, {
  token: tokenAddress,
  to: exchangeHotWallet,
  amount: parseUnits('500', 6),
  memo: pad(stringToHex('CUST-12345'), { size: 32 }),
})
```


### Payroll batch payments[](https://docs.tempo.xyz/guide/payments/transfer-memos#payroll-batch-payments)

Batch multiple payments in a single Tempo transaction with employee IDs in each memo for clear accounting records.

```
import { Abis } from 'viem/tempo'
import { encodeFunctionData, parseUnits, stringToHex, pad } from 'viem'
 
const calls = employees.map(emp => ({
  to: tokenAddress,
  data: encodeFunctionData({
    abi: Abis.TIP20,
    functionName: 'transferWithMemo',
    args: [emp.wallet, parseUnits(emp.salary, 6), pad(stringToHex(emp.id), { size: 32 })]
  })
}))
 
await walletClient.sendCalls({ calls })
```


### Refund address in memo[](https://docs.tempo.xyz/guide/payments/transfer-memos#refund-address-in-memo)

Include a refund address in the memo so the recipient knows where to send funds if a reversal is needed.

```
import { Actions } from 'viem/tempo'
import { parseUnits, stringToHex, pad } from 'viem'
 
const refundMemo = pad(stringToHex('REFUND 0x742d35Cc6634C0532925a3b8'), { size: 32 })
 
await Actions.token.transferSync(walletClient, {
  token: tokenAddress,
  to: merchantAddress,
  amount: parseUnits('100', 6),
  memo: refundMemo,
})
```


Best Practices[](https://docs.tempo.xyz/guide/payments/transfer-memos#best-practices)
-------------------------------------------------------------------------------------

### Use consistent memo formats[](https://docs.tempo.xyz/guide/payments/transfer-memos#use-consistent-memo-formats)

Establish a naming convention for your memos (e.g., `CUST-{id}`, `INV-{number}`, `REFUND-{id}`) to make parsing and filtering reliable across your system.

### Keep memos under 32 bytes[](https://docs.tempo.xyz/guide/payments/transfer-memos#keep-memos-under-32-bytes)

Memos are `bytes32` values. Use `toHex(string, { size: 32 })` to convert strings—if your string exceeds 32 bytes, it will be truncated. For longer references, store the full data offchain and use a hash or short ID as the memo.

### Index memos for efficient queries[](https://docs.tempo.xyz/guide/payments/transfer-memos#index-memos-for-efficient-queries)

The `TransferWithMemo` event has `memo` as an indexed parameter. Use `getLogs` with the `args` filter to query transactions by memo without scanning all events.

```
import { parseAbiItem, stringToHex, pad } from 'viem'
 
const logs = await client.getLogs({
  address: tokenAddress,
  event: parseAbiItem('event TransferWithMemo(address indexed from, address indexed to, uint256 value, bytes32 indexed memo)'),
  args: { memo: pad(stringToHex('INV-12345'), { size: 32 }) },
})
```


Learning Resources[](https://docs.tempo.xyz/guide/payments/transfer-memos#learning-resources)
---------------------------------------------------------------------------------------------
