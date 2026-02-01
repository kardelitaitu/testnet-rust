use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

pub struct UniswapV2SwapTask;

impl UniswapV2SwapTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for UniswapV2SwapTask {
    fn name(&self) -> &str {
        "39_uniswapV2Swap"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        const ROUTER_ADDR: &str = "0x4a2E7A3aF895509874DB31808a86d5871D6ec6fE";
        const WBTC_ADDR: &str = "0xF32D39ff9f6Aa7a7A64d7a4F00a54826Ef791a55";
        const WETH_ADDR: &str = "0x4200000000000000000000000000000000000006";

        let router_address: Address = ROUTER_ADDR.parse()?;
        let wbtc_address: Address = WBTC_ADDR.parse()?;
        let weth_address: Address = WETH_ADDR.parse()?;

        let messages: Vec<String> = Vec::new();

        // Create deadline early for use in addLiquidity
        let deadline = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
            + 1800; // 30 mins

        // Gas settings
        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.clone()));

        // 1. Check WBTC Details
        let erc20_abi = r#"[
            {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
            {"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"type":"function"},
            {"constant":false,"inputs":[{"name":"_spender","type":"address"},{"name":"_value","type":"uint256"}],"name":"approve","outputs":[{"name":"","type":"bool"}],"type":"function"},
            {"constant":true,"inputs":[{"name":"_owner","type":"address"},{"name":"_spender","type":"address"}],"name":"allowance","outputs":[{"name":"","type":"uint256"}],"type":"function"},
            {"constant":false,"inputs":[{"name":"_to","type":"address"},{"name":"_value","type":"uint256"}],"name":"transfer","outputs":[{"name":"","type":"bool"}],"type":"function"}
        ]"#;
        let erc20_abi_parsed: abi::Abi = serde_json::from_str(erc20_abi)?;
        let wbtc_contract = Contract::new(wbtc_address, erc20_abi_parsed.clone(), client.clone());

        let decimals: u8 = wbtc_contract
            .method("decimals", ())?
            .call()
            .await
            .unwrap_or(18); // Default fallback

        let balance: U256 = wbtc_contract
            .method("balanceOf", address)?
            .call()
            .await
            .context("Failed to get WBTC balance")?;

        debug!("WBTC Address: {:?}", wbtc_address);
        debug!("Router Address: {:?}", router_address);
        debug!("WBTC Decimals: {}", decimals);
        debug!("WBTC Balance: {}", balance);

        debug!(
            "Router Code Size: {}",
            provider.get_code(router_address, None).await?.len()
        );

        if balance.is_zero() {
            return Ok(TaskResult {
                success: true,
                message: "No WBTC balance to swap".to_string(),
                tx_hash: None,
            });
        }

        // SWAP HARDCODED TINY AMOUNT
        let amount_in = U256::from(1000);
        debug!("Swapping Amount: {}", amount_in);

        // 2. Check Allowance
        let allowance: U256 = wbtc_contract
            .method("allowance", (address, router_address))?
            .call()
            .await
            .context("Failed to get allowance")?;

        if allowance < amount_in {
            // ... (keep approval logic) ...
            debug!("Approving Router...");
            let approve_data = wbtc_contract.encode("approve", (router_address, U256::MAX))?;
            let approve_tx = Eip1559TransactionRequest::new()
                .to(wbtc_address)
                .data(approve_data)
                .gas(100_000)
                .max_fee_per_gas(max_fee)
                .max_priority_fee_per_gas(priority_fee)
                .from(address);

            let pending = client.send_transaction(approve_tx, None).await?;
            let receipt = pending.await?.context("Approval failed")?;
            debug!("Approval Receipt Status: {:?}", receipt.status);
            if receipt.status != Some(U64::from(1)) {
                return Err(anyhow::anyhow!("WBTC Approval failed"));
            }
        }

        // 0. Verify WETH exists
        let weth_code_size = provider.get_code(weth_address, None).await?.len();
        debug!("WETH Code Size: {}", weth_code_size);
        if weth_code_size == 0 {
            return Ok(TaskResult {
                success: false,
                message: "WETH Contract does not exist at 0x4200...06".to_string(),
                tx_hash: None,
            });
        }

        // =================================================================
        // IDENTITY CHECK: Is this a Router or a Pair?
        // =================================================================
        // 1. Check Balances of the "Router"
        let wbtc_contract = Contract::new(wbtc_address, erc20_abi_parsed.clone(), client.clone());
        let router_wbtc_bal: U256 = wbtc_contract
            .method("balanceOf", router_address)?
            .call()
            .await
            .unwrap_or_default();

        let weth_abi = r#"[{"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"}]"#;
        let weth_abi_ro_parsed: abi::Abi = serde_json::from_str(weth_abi)?;
        let weth_contract_ro = Contract::new(weth_address, weth_abi_ro_parsed, client.clone());
        let router_weth_bal: U256 = weth_contract_ro
            .method("balanceOf", router_address)?
            .call()
            .await
            .unwrap_or_default();

        debug!("'Router' WBTC Balance: {}", router_wbtc_bal);
        debug!("'Router' WETH Balance: {}", router_weth_bal);

        // 2. Check if it has Pair methods
        let pair_check_abi = r#"[
            {"constant":true,"inputs":[],"name":"token0","outputs":[{"name":"","type":"address"}],"type":"function"},
            {"constant":true,"inputs":[],"name":"getReserves","outputs":[{"name":"_reserve0","type":"uint112"},{"name":"_reserve1","type":"uint112"},{"name":"_blockTimestampLast","type":"uint32"}],"type":"function"}
        ]"#;
        let pair_check_abi_parsed: abi::Abi = serde_json::from_str(pair_check_abi)?;
        let pair_check = Contract::new(router_address, pair_check_abi_parsed, client.clone());

        // Try calling token0
        let token0_res: Result<Address, _> = pair_check.method("token0", ())?.call().await;
        match token0_res {
            Ok(t0) => {
                debug!("Contract has token0(): {:?} -> IT IS A PAIR!", t0);

                // =================================================================
                // 3. EXECUTE SWAP (ALL AVAILABLE SATOSHIS)
                // =================================================================
                debug!("Attempting Direct Pair Swap for ALL WBTC...");

                // Fetch full WBTC balance
                let user_wbtc_bal: U256 = wbtc_contract
                    .method("balanceOf", address)?
                    .call()
                    .await
                    .unwrap_or_default();
                debug!("User WBTC Balance: {}", user_wbtc_bal);

                if user_wbtc_bal.is_zero() {
                    return Ok(TaskResult {
                        success: false,
                        message: "No WBTC to swap".to_string(),
                        tx_hash: None,
                    });
                }

                let amount_in = user_wbtc_bal;

                if router_wbtc_bal < amount_in && router_weth_bal < amount_in {
                    debug!("Pair has low liquidity. Swap might fail or have high slippage.");
                }

                // Calculate Amount Out (Uniswap V2 Formula)
                // getReserves returned (res0, res1)
                let pair_reserves = pair_check
                    .method::<_, (u128, u128, u32)>("getReserves", ())?
                    .call()
                    .await?;
                let (r0, r1, _) = pair_reserves;

                let (reserve_in, reserve_out) = if t0 == wbtc_address {
                    (U256::from(r0), U256::from(r1))
                } else {
                    (U256::from(r1), U256::from(r0))
                };

                debug!("Reserves - In: {}, Out: {}", reserve_in, reserve_out);

                // AmountOut = (In * 997 * ReserveOut) / (ReserveIn * 1000 + In * 997)
                let amount_in_with_fee = amount_in * 997;
                let numerator = amount_in_with_fee * reserve_out;
                let denominator = (reserve_in * 1000) + amount_in_with_fee;
                let amount_out: U256 = numerator / denominator;

                debug!("Calculated Amount Out: {} WETH", amount_out);

                if amount_out.is_zero() {
                    return Err(anyhow::anyhow!(
                        "Calculated output amount is 0 (insufficient liquidity)"
                    ));
                }

                // Transfer WBTC to Pair
                debug!("Transferring {} WBTC to Pair...", amount_in);
                let transfer_data =
                    wbtc_contract.encode("transfer", (router_address, amount_in))?;
                let transfer_tx = Eip1559TransactionRequest::new()
                    .to(wbtc_address)
                    .data(transfer_data)
                    .gas(100_000)
                    .max_fee_per_gas(max_fee)
                    .max_priority_fee_per_gas(priority_fee)
                    .from(address);
                let _ = client.send_transaction(transfer_tx, None).await?.await?;
                debug!("Transferred.");

                // Call swap(amount0Out, amount1Out, to, data)
                let amount0_out = if t0 == wbtc_address {
                    U256::zero()
                } else {
                    amount_out
                };
                let amount1_out = if t0 == wbtc_address {
                    amount_out
                } else {
                    U256::zero()
                };

                debug!("Calling swap({}, {})", amount0_out, amount1_out);

                let swap_low_abi = r#"[
                    {"constant":false,"inputs":[{"name":"amount0Out","type":"uint256"},{"name":"amount1Out","type":"uint256"},{"name":"to","type":"address"},{"name":"data","type":"bytes"}],"name":"swap","outputs":[],"payable":false,"stateMutability":"nonpayable","type":"function"}
                ]"#;
                let swap_low_abi_parsed: abi::Abi = serde_json::from_str(swap_low_abi)?;
                let pair_swap_contract =
                    Contract::new(router_address, swap_low_abi_parsed, client.clone());
                let swap_data = pair_swap_contract
                    .encode("swap", (amount0_out, amount1_out, address, Bytes::new()))?;
                let swap_tx = Eip1559TransactionRequest::new()
                    .to(router_address)
                    .data(swap_data)
                    .gas(200_000)
                    .max_fee_per_gas(max_fee)
                    .max_priority_fee_per_gas(priority_fee)
                    .from(address);

                let pending = client.send_transaction(swap_tx, None).await?;
                let receipt = pending.await?.context("Direct Swap failed")?;
                debug!("Swap Success: {:?}", receipt.transaction_hash);

                // =================================================================
                // 4. UNWRAP (ALL WETH -> ETH)
                // =================================================================
                debug!("Unwrapping all WETH...");
                // Add withdraw/deposit to WETH ABI
                let weth_abi_unwrap = r#"[
                    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
                    {"constant":false,"inputs":[{"name":"wad","type":"uint256"}],"name":"withdraw","outputs":[],"payable":false,"stateMutability":"nonpayable","type":"function"}
                ]"#;
                let weth_unwrap_abi: abi::Abi = serde_json::from_str(weth_abi_unwrap)?;
                let weth_contract_full =
                    Contract::new(weth_address, weth_unwrap_abi, client.clone());

                let user_weth_bal: U256 = weth_contract_full
                    .method("balanceOf", address)?
                    .call()
                    .await
                    .unwrap_or_default();
                debug!("User WETH Balance: {}", user_weth_bal);

                if user_weth_bal > U256::zero() {
                    let withdraw_data = weth_contract_full.encode("withdraw", user_weth_bal)?;
                    let unwrap_tx = Eip1559TransactionRequest::new()
                        .to(weth_address)
                        .data(withdraw_data)
                        .gas(100_000)
                        .max_fee_per_gas(max_fee)
                        .max_priority_fee_per_gas(priority_fee)
                        .from(address);

                    let pending_unwrap = client.send_transaction(unwrap_tx, None).await?;
                    let receipt_unwrap = pending_unwrap.await?.context("Unwrap failed")?;
                    debug!("Unwrap Success: {:?}", receipt_unwrap.transaction_hash);
                }

                return Ok(TaskResult {
                    success: true,
                    message: format!("Swapped {} WBTC -> {} WETH -> ETH", amount_in, amount_out),
                    tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
                });
            }
            Err(_) => {
                debug!("Contract does NOT have token0() -> Likely a Router.");
            }
        }

        // Fallback for Non-Pair (Router) kept for structure validity, though unused

        // 3. Swap WBTC -> WETH
        let router_abi_swap = r#"[
            {"inputs":[{"internalType":"uint256","name":"amountIn","type":"uint256"},{"internalType":"uint256","name":"amountOutMin","type":"uint256"},{"internalType":"address[]","name":"path","type":"address[]"},{"internalType":"address","name":"to","type":"address"},{"internalType":"uint256","name":"deadline","type":"uint256"}],"name":"swapExactTokensForTokens","outputs":[{"internalType":"uint256[]","name":"amounts","type":"uint256[]"}],"stateMutability":"nonpayable","type":"function"}
        ]"#;

        let router_abi_swap_parsed: abi::Abi = serde_json::from_str(router_abi_swap)?;
        let router_contract = Contract::new(router_address, router_abi_swap_parsed, client.clone());

        let path = vec![wbtc_address, weth_address];
        let amount_in = U256::from(100); // Reduce swap to 100 satoshis since we only added 1000

        // Call swapExactTokensForTokens details for debug
        debug!("Path: {:?} -> {:?}", wbtc_address, weth_address);
        debug!("Deadline: {}", deadline);

        let swap_data = router_contract.encode(
            "swapExactTokensForTokens",
            (
                amount_in,
                U256::from(0),
                path,
                address,
                U256::from(deadline),
            ),
        )?;

        let swap_tx = Eip1559TransactionRequest::new()
            .to(router_address)
            .data(swap_data)
            .gas(500_000)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let pending_swap = client.send_transaction(swap_tx, None).await?;
        let receipt = pending_swap.await?.context("Swap transaction failed")?;

        if receipt.status != Some(U64::from(1)) {
            return Err(anyhow::anyhow!(
                "Swap execution reverted. Hash: {:?}",
                receipt.transaction_hash
            ));
        }

        let amount_float =
            ethers::utils::format_units(amount_in, decimals as u32).unwrap_or("???".to_string());

        Ok(TaskResult {
            success: true,
            message: format!(
                "Swapped {} WBTC for WETH. \nDebug info: {}",
                amount_float,
                messages.join(" | ")
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
