use super::{SepoliaTask, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::abi::{encode, Token};
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use ethers::types::Bytes;
use std::sync::Arc;
use std::time::Duration;

/// USDC+ (C+) on Sepolia — the OFT token we bridge
const USDC_PLUS: &str = "0xe815718d44694ec4637cb775c468d87f6e15b538";
/// Base Sepolia LayerZero endpoint ID
const DST_EID: u32 = 40245;

/// ABI for balanceOf
const BRIDGE_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"}
]"#;

/// Verified send selector from working tx
const SEND_SELECTOR: [u8; 4] = [0xc7, 0xc7, 0xf5, 0xb3];

fn build_send_param(
    dst_eid: u32,
    to: Address,
    amount: U256,
    extra_options: Vec<u8>,
) -> Token {
    let to_bytes32 = {
        let mut b = [0u8; 32];
        let addr: [u8; 20] = to.into();
        b[12..32].copy_from_slice(&addr);
        b
    };

    Token::Tuple(vec![
        Token::Uint(U256::from(dst_eid)),
        Token::FixedBytes(to_bytes32.to_vec()),
        Token::Uint(amount),
        Token::Uint(amount),
        Token::Bytes(extra_options),
        Token::Bytes(Vec::new()),
        Token::Bytes(Vec::new()),
    ])
}

fn build_fee(native: U256, lz: U256) -> Token {
    Token::Tuple(vec![Token::Uint(native), Token::Uint(lz)])
}

fn encode_send(send_param: &Token, fee: &Token, refund: Address) -> Bytes {
    let mut data = SEND_SELECTOR.to_vec();
    data.extend(encode(&[
        send_param.clone(),
        fee.clone(),
        Token::Address(refund),
    ]));
    Bytes::from(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_send_param_creates_7_element_tuple() {
        let addr: Address = "0xd7d2e492e6dda0013e9062f00327a06fdb722488".parse().unwrap();
        let param = build_send_param(40245, addr, U256::from(100), vec![0x00, 0x03]);
        match param {
            Token::Tuple(items) => {
                assert_eq!(items.len(), 7);
                assert_eq!(items[0], Token::Uint(U256::from(40245)));
                assert_eq!(items[2], Token::Uint(U256::from(100)));
            }
            _ => panic!("SendParam must be a Tuple"),
        }
    }

    #[test]
    fn test_build_fee_creates_2_element_tuple() {
        let fee = build_fee(U256::from(500), U256::from(1));
        match fee {
            Token::Tuple(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], Token::Uint(U256::from(500)));
            }
            _ => panic!("Fee must be a Tuple"),
        }
    }

    #[test]
    fn test_encode_send_starts_with_correct_selector() {
        let addr: Address = "0xd7d2e492e6dda0013e9062f00327a06fdb722488".parse().unwrap();
        let param = build_send_param(1, addr, U256::zero(), vec![]);
        let fee = build_fee(U256::zero(), U256::zero());
        let encoded = encode_send(&param, &fee, addr);
        assert_eq!(encoded[0], 0xc7);
        assert_eq!(encoded[1], 0xc7);
        assert_eq!(encoded[2], 0xf5);
        assert_eq!(encoded[3], 0xb3);
    }
}

async fn get_cplus_balance(provider: &Provider<Http>, wallet: Address) -> Result<U256> {
    let addr: Address = USDC_PLUS.parse()?;
    let contract = Contract::new(
        addr,
        serde_json::from_str::<ethers::abi::Abi>(BRIDGE_ABI)?,
        Arc::new(provider.clone()),
    );
    Ok(contract.method::<_, U256>("balanceOf", wallet)?.call().await?)
}

pub struct BridgeCplusTask;

#[async_trait]
impl SepoliaTask for BridgeCplusTask {
    fn name(&self) -> &str {
        "13_bridgeCplus"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let cplus_addr: Address = USDC_PLUS.parse()?;

        // --- 1. Check C+ balance ---
        let cplus_balance = get_cplus_balance(provider, address).await?;

        // --- 2. Calculate 5% of C+, round to nearest whole C+ ---
        let pct_raw = cplus_balance.as_u128() * 5 / 100;
        let rounding = 500_000_000_000_000_000u128;
        let whole_cplus = (pct_raw + rounding) / 1_000_000_000_000_000_000u128;
        let bridge_amount = U256::from(whole_cplus) * U256::exp10(18);

        if whole_cplus == 0 {
            return Ok(TaskResult {
                success: false,
                message: "5% of C+ balance rounds to 0, nothing to bridge".to_string(),
            });
        }

        // --- 3. Build SendParam ---
        let extra_options = hex::decode("0003").unwrap();
        let send_param = build_send_param(DST_EID, address, bridge_amount, extra_options);

        // --- 4. Get gas fees ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // --- 5. Execute send ---
        let native_fee = U256::from(200_000_000_000_000u128); // 0.0002 ETH
        let fee = build_fee(native_fee, U256::zero());
        let send_calldata = encode_send(&send_param, &fee, address);

        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        let tx = ethers::types::TransactionRequest::default()
            .to(cplus_addr)
            .data(send_calldata)
            .gas(350_000)
            .gas_price(max_fee)
            .value(native_fee);

        let pending_tx = middleware.send_transaction(tx, None).await.context("Failed to send bridge tx")?;
        let tx_hash = pending_tx.tx_hash();

        let receipt = pending_tx
            .confirmations(1)
            .interval(Duration::from_millis(500))
            .await?;

        let success = receipt.is_some_and(|r| r.status == Some(1.into()));
        Ok(TaskResult {
            success,
            message: format!(
                "Bridged {} C+ → Base Sepolia (tx: {:?}) | fee: {:.6} ETH",
                whole_cplus, tx_hash, native_fee.as_u128() as f64 / 1e18
            ),
        })
    }
}
