use super::{SepoliaTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::sync::Arc;
use std::time::Duration;

/// USDC+ (C+) on Sepolia
const USDC_PLUS: &str = "0xe815718d44694ec4637cb775c468d87f6e15b538";

/// Minimal ERC-20 ABI: balanceOf + transfer
const CPLUS_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":false,"inputs":[{"name":"to","type":"address"},{"name":"amount","type":"uint256"}],"name":"transfer","outputs":[{"name":"","type":"bool"}],"type":"function"}
]"#;

/// Gas budget for proxy top-up (return transfer only, with a small buffer).
const PROXY_TOPUP_GAS_LIMIT: u64 = 80_000;
/// Gas buffer for main wallet's two transactions (token transfer 60k + ETH send 21k) + margin
const MAIN_GAS_LIMIT: u64 = 125_000;
/// Gas limit for the token return transfer itself.
const RETURN_TRANSFER_GAS_LIMIT: u64 = 60_000;
/// Maximum time to wait for the proxy to confirm receipt.
const PROXY_POLL_TIMEOUT_SECS: u64 = 60;

pub struct ReceiveCplusTask;

#[async_trait]
impl SepoliaTask for ReceiveCplusTask {
    fn name(&self) -> &str {
        "19_receiveCplus"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let main_wallet = ctx.wallet;
        let main_addr = main_wallet.address();
        let provider = &ctx.provider;
        let chain_id = ctx.config.chain_id;
        let cplus_addr: Address = USDC_PLUS.parse()?;

        // ------------------------------------------------------------------
        // 1. Generate ephemeral proxy wallet
        // ------------------------------------------------------------------
        let mut rng = StdRng::from_entropy();
        let proxy_wallet = LocalWallet::new(&mut rng);
        let proxy_addr = proxy_wallet.address();

        // ------------------------------------------------------------------
        // 2. Get C+ balance on main wallet
        // ------------------------------------------------------------------
        let contract = Contract::new(
            cplus_addr,
            serde_json::from_str::<ethers::abi::Abi>(CPLUS_ABI)?,
            Arc::new(provider.clone()),
        );

        let balance: U256 = contract
            .method::<_, U256>("balanceOf", main_addr)?
            .call()
            .await
            .context("Failed to query C+ balance")?;

        // 5% of balance
        let amount = balance * U256::from(5) / U256::from(100);
        if amount.is_zero() {
            return Ok(TaskResult {
                success: false,
                message: "C+ balance too low — 5% rounds to 0".into(),
            });
        }

        // ------------------------------------------------------------------
        // 3. Get gas fees
        // ------------------------------------------------------------------
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // ------------------------------------------------------------------
        // 4. Check main wallet has enough ETH for gas + proxy funding
        // ------------------------------------------------------------------
        let main_eth: U256 = provider
            .get_balance(main_addr, None)
            .await
            .context("Failed to check main ETH balance")?;

        let proxy_gas_u256 = calculate_proxy_topup_amount(max_fee);
        let main_gas_u256 = U256::from(MAIN_GAS_LIMIT) * max_fee;
        let min_eth = proxy_gas_u256 + main_gas_u256;
        if main_eth < min_eth {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Not enough ETH — have {} wei, need {} wei",
                    main_eth, min_eth
                ),
            });
        }

        // ------------------------------------------------------------------
        // 5. Build main-wallet signer
        // ------------------------------------------------------------------
        let main_signer = Arc::new(SignerMiddleware::new(
            provider.clone(),
            main_wallet.clone().with_chain_id(chain_id),
        ));

        // ------------------------------------------------------------------
        // 6. Send C+ from main → proxy
        // ------------------------------------------------------------------
        let cplus_contract = Contract::new(
            cplus_addr,
            serde_json::from_str::<ethers::abi::Abi>(CPLUS_ABI)?,
            main_signer.clone(),
        );

        let transfer_call = cplus_contract
            .method::<_, H256>("transfer", (proxy_addr, amount))?
            .gas(RETURN_TRANSFER_GAS_LIMIT)
            .gas_price(max_fee);

        let tx = transfer_call
            .send()
            .await
            .context("Failed to send C+ to proxy")?;

        let receipt = tx
            .confirmations(1)
            .interval(Duration::from_millis(500))
            .await?;

        if receipt.is_none_or(|r| r.status != Some(1.into())) {
            return Ok(TaskResult {
                success: false,
                message: format!("C+ transfer to proxy {} failed on-chain", proxy_addr),
            });
        }

        // ------------------------------------------------------------------
        // 7. Send ETH from main → proxy (for proxy's return gas)
        // ------------------------------------------------------------------
        let eth_tx = main_signer
            .send_transaction(TransactionRequest::pay(proxy_addr, proxy_gas_u256), None)
            .await
            .context("Failed to send ETH to proxy")?;

        let eth_receipt = eth_tx
            .confirmations(1)
            .interval(Duration::from_millis(500))
            .await?;

        if eth_receipt.is_none_or(|r| r.status != Some(1.into())) {
            return Ok(TaskResult {
                success: false,
                message: format!("ETH transfer to proxy {} failed on-chain", proxy_addr),
            });
        }

        // ------------------------------------------------------------------
        // 8. Wait for proxy to confirm receipt of both
        // ------------------------------------------------------------------
        let proxy_amount = amount;
        let deadline = std::time::Instant::now() + Duration::from_secs(PROXY_POLL_TIMEOUT_SECS);

        let (proxy_t_balance, proxy_eth_balance) = loop {
            let tb: U256 = contract
                .method::<_, U256>("balanceOf", proxy_addr)?
                .call()
                .await
                .unwrap_or(U256::zero());
            let eb: U256 = provider
                .get_balance(proxy_addr, None)
                .await
                .unwrap_or(U256::zero());

            if tb >= proxy_amount && eb >= proxy_gas_u256 {
                break (tb, eb);
            }
            if std::time::Instant::now() > deadline {
                return Ok(TaskResult {
                    success: false,
                    message: format!("Timeout waiting for proxy {} to receive funds", proxy_addr),
                });
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        };

        // ------------------------------------------------------------------
        // 9. Check proxy has enough ETH for return gas
        // ------------------------------------------------------------------
        let (proxy_max_fee, _) = ctx.gas_manager.get_fees().await?;
        let estimated_cost = U256::from(RETURN_TRANSFER_GAS_LIMIT) * proxy_max_fee;
        if proxy_eth_balance < estimated_cost {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Proxy {} has {} wei but needs ~{} wei for return gas (max_fee={} gwei, 60k gas)",
                    proxy_addr,
                    proxy_eth_balance,
                    estimated_cost,
                    proxy_max_fee / U256::from(1_000_000_000u64),
                ),
            });
        }

        // ------------------------------------------------------------------
        // 10. Proxy sends all C+ back to main wallet
        // ------------------------------------------------------------------
        let proxy_signer = Arc::new(SignerMiddleware::new(
            provider.clone(),
            proxy_wallet.with_chain_id(chain_id),
        ));

        let proxy_cplus = Contract::new(
            cplus_addr,
            serde_json::from_str::<ethers::abi::Abi>(CPLUS_ABI)?,
            proxy_signer.clone(),
        );

        let proxy_return_call = proxy_cplus
            .method::<_, H256>("transfer", (main_addr, proxy_t_balance))?
            .gas(RETURN_TRANSFER_GAS_LIMIT)
            .gas_price(proxy_max_fee);
        let proxy_return = proxy_return_call
            .send()
            .await
            .context("Proxy failed to send C+ back")?;

        let return_tx_hash = proxy_return.tx_hash();
        let return_receipt = proxy_return
            .confirmations(1)
            .interval(Duration::from_millis(500))
            .await?;

        let return_ok = return_receipt
            .as_ref()
            .is_some_and(|r| r.status == Some(1.into()));

        let result_msg = format!(
            "Received {} USDC Plus (tx: {:?})",
            format_whole_with_commas(proxy_t_balance),
            return_tx_hash,
        );

        Ok(TaskResult {
            success: return_ok,
            message: result_msg,
        })
    }
}

fn calculate_proxy_topup_amount(max_fee: U256) -> U256 {
    U256::from(PROXY_TOPUP_GAS_LIMIT) * max_fee
}

/// Divide by 10^18 and format the whole part with comma separators.
/// e.g. 27597078135524549372407 → "27,597"
fn format_whole_with_commas(amount: U256) -> String {
    let divisor = U256::from(10u128.pow(18));
    let whole = amount / divisor;
    let s = whole.to_string();
    // Insert commas every 3 chars from the right
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_is_correct() {
        let task = ReceiveCplusTask;
        assert_eq!(task.name(), "19_receiveCplus");
    }

    #[test]
    fn test_format_whole_with_commas_27k() {
        let amount = U256::from(27_597u128) * U256::from(10u128.pow(18));
        assert_eq!(format_whole_with_commas(amount), "27,597");
    }

    #[test]
    fn test_format_whole_with_commas_small() {
        let amount = U256::from(100u128) * U256::from(10u128.pow(18));
        assert_eq!(format_whole_with_commas(amount), "100");
    }

    #[test]
    fn test_format_whole_with_commas_million() {
        let amount = U256::from(1_234_567u128) * U256::from(10u128.pow(18));
        assert_eq!(format_whole_with_commas(amount), "1,234,567");
    }

    #[test]
    fn test_format_whole_with_commas_zero() {
        assert_eq!(format_whole_with_commas(U256::zero()), "0");
    }

    #[test]
    fn test_format_whole_with_commas_billion() {
        let amount = U256::from(3_456_789_012u128) * U256::from(10u128.pow(18));
        assert_eq!(format_whole_with_commas(amount), "3,456,789,012");
    }

    #[test]
    fn test_return_transfer_gas_limit_is_60k() {
        assert_eq!(RETURN_TRANSFER_GAS_LIMIT, 60_000);
    }

    #[test]
    fn test_calculate_proxy_topup_amount_1gwei() {
        let one_gwei = U256::from(1_000_000_000u64);
        let expected = U256::from(PROXY_TOPUP_GAS_LIMIT) * one_gwei;
        assert_eq!(calculate_proxy_topup_amount(one_gwei), expected);
    }
}
