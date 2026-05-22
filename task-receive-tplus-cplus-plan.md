# Plan: `receiveTplus` and `receiveCplus` Tasks

## Goal

Two new tasks that test the full send/receive cycle of T+ (USDT+) and C+ (USDC+) tokens by sending them to a fresh ephemeral wallet and having that wallet send them back.

## Flow

```
Main Wallet → (token + ETH gas) → Proxy Wallet → (token) → Main Wallet
                                   ↑ generated fresh, ephemeral
```

1. Generate a random `LocalWallet` (the proxy)
2. Calculate:
   - Token amount (e.g. 0.5% of balance)
   - ETH to send to proxy (enough for proxy's return tx gas, e.g. 0.01 ETH)
3. Transfer token from main → proxy
4. Transfer ETH from main → proxy
5. Wait until proxy confirms receipt of both
6. Create a new SignerMiddleware for the proxy wallet
7. Proxy transfers the token back to main wallet
8. Return success/failure message

## Files to modify

### 1. Create `chains/sepolia-overlayer/src/task/t18_receive_tplus.rs`
### 2. Create `chains/sepolia-overlayer/src/task/t19_receive_cplus.rs`
### 3. Modify `chains/sepolia-overlayer/src/task/mod.rs` — register both modules
### 4. Modify `chains/sepolia-overlayer/src/spammer/mod.rs` — add to task list
### 5. Modify `chains/sepolia-overlayer/src/bin/debug_task.rs` — add to debug list
### 6. Modify `chains/sepolia-overlayer/src/daily_runner/mod.rs` — add to `ALL_TASK_NAMES` + `all_tasks()`

## Task implementation details

### ReceiveTplusTask / ReceiveCplusTask

```rust
use super::{SepoliaTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use crate::utils::calc::calc_pct_rounded;

const USDT_PLUS: &str = "0xe20534a32f9162488a90026f268a74fbe28d272d";
const USDC_PLUS: &str = "0xe815718d44694ec4637cb775c468d87f6e15b538";

const ERC20_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":false,"inputs":[{"name":"to","type":"address"},{"name":"amount","type":"uint256"}],"name":"transfer","outputs":[{"name":"","type":"bool"}],"type":"function"}
]"#;

/// ETH to send to proxy for gas (the proxy needs ~0.003 ETH for a transfer)
const PROXY_GAS_ETH: &str = "0.005";

struct ReceiveTplusTask;  // (and ReceiveCplusTask)

#[async_trait]
impl SepoliaTask for ReceiveTplusTask {
    fn name(&self) -> &str { "18_receiveTplus" }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let main_wallet = ctx.wallet;
        let main_addr = main_wallet.address();
        let provider = &ctx.provider;
        let token_addr: Address = USDT_PLUS.parse()?;  // or USDC_PLUS for C+

        // --- 1. Generate proxy wallet ---
        let mut rng = rand::thread_rng();
        let proxy_wallet = LocalWallet::new(&mut rng);
        let proxy_addr = proxy_wallet.address();

        // --- 2. Get token balance ---
        let contract = Contract::new(
            token_addr,
            serde_json::from_str::<ethers::abi::Abi>(ERC20_ABI)?,
            Arc::new(provider.clone()),
        );
        let balance: U256 = contract
            .method::<_, U256>("balanceOf", main_addr)?
            .call()
            .await
            .context("Failed to query token balance")?;

        // Calculate 0.5%
        let amount = balance * U256::from(5) / U256::from(1000);
        if amount.is_zero() {
            return Ok(TaskResult { success: false, message: "Balance too low for 0.5% transfer".into() });
        }

        // --- 3. Check main wallet has enough ETH ---
        let main_eth: U256 = provider
            .get_balance(main_addr, None)
            .await
            .context("Failed to check ETH balance")?;
        let proxy_gas: U256 = parse_units(PROXY_GAS_ETH, "ether")?;
        // Estimate: transfer token (~70k gas at ~10 gwei = 0.0007) + ETH transfer (21k gas)
        // But we just check main has enough: balance > proxy_gas + some buffer
        if main_eth < proxy_gas + U256::from(10).pow(U256::from(15)) { // 0.001 ETH buffer
            return Ok(TaskResult { success: false, message: "Not enough ETH for gas".into() });
        }

        // --- 4. Send token from main → proxy ---
        let client = Arc::new(SignerMiddleware::new(
            provider.clone(),
            main_wallet.clone().with_chain_id(ctx.config.chain_id),
        ));
        let token_tx = contract
            .connect(client.clone())
            .method::<_, ethers::types::Bytes>("transfer", (proxy_addr, amount))?
            .send()
            .await
            .context("Failed to send token to proxy")?
            .await
            .context("Token transfer to proxy failed")?;
        // ... wait for confirmation ...

        // --- 5. Send ETH from main → proxy ---
        let eth_tx = client
            .send_transaction(
                ethers::types::TransactionRequest::pay(proxy_addr, proxy_gas),
                None,
            )
            .await
            .context("Failed to send ETH to proxy")?
            .await
            .context("ETH transfer to proxy failed")?;

        // --- 6. Wait for proxy to have received both ---
        // Poll until proxy balance >= amount (token) and proxy_eth >= some threshold
        // ...

        // --- 7. Proxy sends token back ---
        let proxy_client = Arc::new(SignerMiddleware::new(
            provider.clone(),
            proxy_wallet.with_chain_id(ctx.config.chain_id),
        ));
        let proxy_contract = Contract::new(
            token_addr,
            serde_json::from_str::<ethers::abi::Abi>(ERC20_ABI)?,
            proxy_client.clone(),
        );
        let proxy_token_balance: U256 = proxy_contract
            .method::<_, U256>("balanceOf", proxy_addr)?
            .call()
            .await
            .context("Failed to query proxy token balance")?;

        if proxy_token_balance.is_zero() {
            return Ok(TaskResult { success: false, message: "Proxy has no tokens to return".into() });
        }

        let return_tx = proxy_contract
            .method::<_, ethers::types::Bytes>("transfer", (main_addr, proxy_token_balance))?
            .send()
            .await
            .context("Failed to send token back from proxy")?
            .await
            .context("Proxy return transfer failed")?;

        // --- 8. Success ---
        Ok(TaskResult {
            success: true,
            message: format!("Received {amount_formatted} {symbol} back via proxy {proxy_addr:#x}"),
        })
    }
}
```

## Verification

- `cargo check -p sepolia-overlayer` — no warnings
- `cargo test -p sepolia-overlayer` — all 193 existing tests pass
- Name test: `test_name_is_correct` for each
- The tasks are RPC-dependent so E2E requires running the daily runner

## Risk

| Risk | Mitigation |
|------|------------|
| Proxy runs out of ETH mid-tx | Send enough gas (0.005 ETH), which covers ~50 simple transfers |
| Token transfer fails | Returns `success: false` with descriptive message |
| Timeout waiting for proxy receipt | Loop with timeout, warn on failure |
| Proxy wallet generation failure | `LocalWallet::new(&mut rng)` never fails |
