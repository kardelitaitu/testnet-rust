use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::abi;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

pub struct RiseToWethTask;

#[async_trait]
impl Task<TaskContext> for RiseToWethTask {
    fn name(&self) -> &str {
        "60_riseToWeth"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();
        let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.clone()));

        let rise_address: Address = "0xd6e1afe5cA8D00A2EFC01B89997abE2De47fdfAf".parse()?;
        let weth_address: Address = "0x4200000000000000000000000000000000000006".parse()?;
        let target_address: Address = "0xB7F4EF15e5A9be9047514D03376F332eaE93EAD4".parse()?;

        // Gas Fees
        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;

        debug!("Checking Identity of Target: {:?}", target_address);

        let erc20_abi = r#"[
            {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
            {"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"type":"function"},
            {"constant":false,"inputs":[{"name":"_to","type":"address"},{"name":"_value","type":"uint256"}],"name":"transfer","outputs":[{"name":"","type":"bool"}],"type":"function"},
            {"constant":false,"inputs":[{"name":"_spender","type":"address"},{"name":"_value","type":"uint256"}],"name":"approve","outputs":[{"name":"","type":"bool"}],"type":"function"}
        ]"#;
        let erc20_abi_parsed: abi::Abi = serde_json::from_str(erc20_abi)?;

        let rise_contract = Contract::new(rise_address, erc20_abi_parsed.clone(), client.clone());
        // weth_contract not used in this scope, removing to fix warning
        // let weth_contract = Contract::new(weth_address, erc20_abi_parsed.clone(), client.clone());

        // Check Identity (Pair vs Router)
        let pair_check_abi = r#"[
            {"constant":true,"inputs":[],"name":"token0","outputs":[{"name":"","type":"address"}],"type":"function"},
            {"constant":true,"inputs":[],"name":"getReserves","outputs":[{"name":"_reserve0","type":"uint112"},{"name":"_reserve1","type":"uint112"},{"name":"_blockTimestampLast","type":"uint32"}],"type":"function"}
        ]"#;
        let pair_check_abi_parsed: abi::Abi = serde_json::from_str(pair_check_abi)?;
        let pair_check = Contract::new(target_address, pair_check_abi_parsed, client.clone());

        let identity_check: Result<Address, _> =
            pair_check.method("token0", ()).unwrap().call().await;

        let is_pair = match identity_check {
            Ok(t0) => {
                debug!("Target has token0 ({:?}) -> IT IS A PAIR", t0);
                true
            }
            Err(_) => {
                debug!("Target verification failed -> Likely a Router");
                false
            }
        };

        // Get RISE Balance
        let rise_bal: U256 = rise_contract
            .method("balanceOf", address)?
            .call()
            .await
            .unwrap_or_default();
        debug!("RISE Balance: {}", rise_bal);

        if rise_bal.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: "No RISE to swap".into(),
                tx_hash: None,
            });
        }

        if is_pair {
            // DIRECT PAIR SWAP (Transfer -> Swap)
            // 1. Transfer RISE to Pair
            debug!("Transferring {} RISE to Pair...", rise_bal);
            let transfer_data = rise_contract.encode("transfer", (target_address, rise_bal))?;
            let tx = Eip1559TransactionRequest::new()
                .to(rise_address)
                .data(transfer_data)
                .gas(100_000)
                .max_fee_per_gas(max_fee)
                .max_priority_fee_per_gas(priority_fee)
                .from(address);
            let _ = client.send_transaction(tx, None).await?.await?;

            // 2. Calculate Output
            let pair_reserves = pair_check
                .method::<_, (u128, u128, u32)>("getReserves", ())?
                .call()
                .await?;
            let (r0, r1, _) = pair_reserves;

            // We need to know if RISE is token0 or token1 to know which reserve is which
            let t0: Address = pair_check.method("token0", ())?.call().await?;
            let (reserve_in, reserve_out) = if t0 == rise_address {
                (U256::from(r0), U256::from(r1))
            } else {
                (U256::from(r1), U256::from(r0))
            };

            let amount_in_with_fee = rise_bal * 997;
            let numerator = amount_in_with_fee * reserve_out;
            let denominator = (reserve_in * 1000) + amount_in_with_fee;
            let amount_out: U256 = numerator / denominator;

            debug!("Calculated Amount Out: {}", amount_out);

            // 3. Swap Call
            let amount0_out = if t0 == rise_address {
                U256::zero()
            } else {
                amount_out
            };
            let amount1_out = if t0 == rise_address {
                amount_out
            } else {
                U256::zero()
            };

            debug!("Calling swap({}, {})", amount0_out, amount1_out);

            let swap_abi = r#"[{"constant":false,"inputs":[{"name":"amount0Out","type":"uint256"},{"name":"amount1Out","type":"uint256"},{"name":"to","type":"address"},{"name":"data","type":"bytes"}],"name":"swap","outputs":[],"type":"function"}]"#;
            let swap_abi_parsed: abi::Abi = serde_json::from_str(swap_abi)?;
            let swap_contract = Contract::new(target_address, swap_abi_parsed, client.clone());
            let swap_data =
                swap_contract.encode("swap", (amount0_out, amount1_out, address, Bytes::new()))?;

            let tx_swap = Eip1559TransactionRequest::new()
                .to(target_address)
                .data(swap_data)
                .gas(200_000)
                .max_fee_per_gas(max_fee)
                .max_priority_fee_per_gas(priority_fee)
                .from(address);

            let receipt = client
                .send_transaction(tx_swap, None)
                .await?
                .await?
                .context("Swap failed")?;
            debug!("Swap TX: {:?}", receipt.transaction_hash);
        } else {
            // ROUTER SWAP (Approve -> swapExactTokensForETH)
            // Assuming standard Uniswap V2 Router
            debug!("Approving Router...");
            let approve_data = rise_contract.encode("approve", (target_address, rise_bal))?;
            let tx_approve = Eip1559TransactionRequest::new()
                .to(rise_address)
                .data(approve_data)
                .gas(100_000)
                .max_fee_per_gas(max_fee)
                .max_priority_fee_per_gas(priority_fee)
                .from(address);
            let _ = client.send_transaction(tx_approve, None).await?.await?;

            debug!("calling swapExactTokensForETH...");
            // swapExactTokensForETH(amountIn, amountOutMin, path, to, deadline)
            let router_abi = r#"[{"constant":false,"inputs":[{"name":"amountIn","type":"uint256"},{"name":"amountOutMin","type":"uint256"},{"name":"path","type":"address[]"},{"name":"to","type":"address"},{"name":"deadline","type":"uint256"}],"name":"swapExactTokensForETH","outputs":[{"name":"amounts","type":"uint256[]"}],"type":"function"}]"#;
            let router_abi_parsed: abi::Abi = serde_json::from_str(router_abi)?;
            let router_contract = Contract::new(target_address, router_abi_parsed, client.clone());

            let path = vec![rise_address, weth_address]; // RISE -> WETH (implicitly unwraps to ETH)
                                                         // Note: swapExactTokensForETH ends with ETH, so we don't need manual unwrap.
                                                         // But user said "RISE to WETH then unwrap". standard function does exactly that.

            let deadline = U256::from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs()
                    + 120,
            );

            let swap_data = router_contract.encode(
                "swapExactTokensForETH",
                (rise_bal, U256::zero(), path, address, deadline),
            )?;
            let tx_swap = Eip1559TransactionRequest::new()
                .to(target_address)
                .data(swap_data)
                .gas(300_000)
                .max_fee_per_gas(max_fee)
                .max_priority_fee_per_gas(priority_fee)
                .from(address);

            let receipt = client
                .send_transaction(tx_swap, None)
                .await?
                .await?
                .context("Router Swap failed")?;
            debug!("Swap TX: {:?}", receipt.transaction_hash);

            // If Router swap succeeds, we have ETH directly.
            // Manual unwrap logic only needed if we used swapExactTokensForTokens to WETH.
            // But let's assume standard router behavior.
            return Ok(TaskResult {
                success: true,
                message: "Swapped RISE -> ETH via Router".into(),
                tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
            });
        }

        // 4. UNWRAP WETH (Only needed if Pair Logic was used, or Router returned WETH)
        // Since Router (swapExactTokensForETH) handles it, this is mostly for Pair path.
        // But for safety, we check WETH balance and unwrap any we have.
        debug!("Checking for any WETH to unwrap...");

        // Add withdraw to WETH ABI
        let weth_unwrap_abi_str = r#"[
            {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
            {"constant":false,"inputs":[{"name":"wad","type":"uint256"}],"name":"withdraw","outputs":[],"type":"function"}
        ]"#;
        let weth_unwrap_abi: abi::Abi = serde_json::from_str(weth_unwrap_abi_str)?;
        let weth_contract_full = Contract::new(weth_address, weth_unwrap_abi, client.clone());

        let user_weth_bal: U256 = weth_contract_full
            .method("balanceOf", address)?
            .call()
            .await
            .unwrap_or_default();
        debug!("WETH Balance: {}", user_weth_bal);

        if user_weth_bal > U256::zero() {
            debug!("Unwrapping WETH...");
            let withdraw_data = weth_contract_full.encode("withdraw", user_weth_bal)?;
            let tx = Eip1559TransactionRequest::new()
                .to(weth_address)
                .data(withdraw_data)
                .gas(100_000)
                .max_fee_per_gas(max_fee)
                .max_priority_fee_per_gas(priority_fee)
                .from(address);
            let receipt = client
                .send_transaction(tx, None)
                .await?
                .await?
                .context("Unwrap failed")?;
            debug!("Unwrap TX: {:?}", receipt.transaction_hash);
        }

        Ok(TaskResult {
            success: true,
            message: "Swapped RISE -> WETH -> ETH".into(),
            tx_hash: None,
        })
    }
}
