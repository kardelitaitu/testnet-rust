use super::{SepoliaTask, TaskContext, TaskResult};
use crate::utils::calc::calc_pct_rounded;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::abi::{encode, Token};
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use ethers::types::Bytes;
use std::sync::Arc;
use std::time::Duration;

/// USDT+ (T+) on Sepolia — the OFT token we bridge
const USDT_PLUS: &str = "0xe20534a32f9162488a90026f268a74fbe28d272d";
/// Base Sepolia LayerZero endpoint ID (from working tx: 0x9d35 = 40245)
const DST_EID: u32 = 40245;

/// ABI for balanceOf + approve + allowance
const BRIDGE_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":false,"inputs":[{"name":"spender","type":"address"},{"name":"value","type":"uint256"}],"name":"approve","outputs":[{"name":"","type":"bool"}],"type":"function"},
    {"constant":true,"inputs":[{"name":"_owner","type":"address"},{"name":"_spender","type":"address"}],"name":"allowance","outputs":[{"name":"","type":"uint256"}],"type":"function"}
]"#;

/// Verified send selector from working tx
const SEND_SELECTOR: [u8; 4] = [0xc7, 0xc7, 0xf5, 0xb3];

/// Build the SendParam token array for LayerZero OFT send
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
        Token::Uint(U256::from(dst_eid)),        // dstEid (uint32 padded)
        Token::FixedBytes(to_bytes32.to_vec()),   // to (bytes32)
        Token::Uint(amount),                      // amountLD
        Token::Uint(amount),                      // minAmountLD
        Token::Bytes(extra_options),              // extraOptions
        Token::Bytes(Vec::new()),                 // composeMsg
        Token::Bytes(Vec::new()),                 // oftCmd
    ])
}

/// Build the MessagingFee token pair
fn build_fee(native: U256, lz: U256) -> Token {
    Token::Tuple(vec![Token::Uint(native), Token::Uint(lz)])
}

/// Encode calldata for send(sendParam, fee, refundAddress)
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
                assert_eq!(items.len(), 7, "SendParam must have 7 components");
                // dstEid
                assert_eq!(items[0], Token::Uint(U256::from(40245)));
                // to is bytes32
                assert!(matches!(items[1], Token::FixedBytes(_)));
                // amountLD and minAmountLD
                assert_eq!(items[2], Token::Uint(U256::from(100)));
                assert_eq!(items[3], Token::Uint(U256::from(100)));
                // extraOptions
                assert_eq!(items[4], Token::Bytes(vec![0x00, 0x03]));
                // composeMsg and oftCmd (empty)
                assert_eq!(items[5], Token::Bytes(vec![]));
                assert_eq!(items[6], Token::Bytes(vec![]));
            }
            _ => panic!("SendParam must be a Tuple token"),
        }
    }

    #[test]
    fn test_build_send_param_addr_to_bytes32() {
        let addr: Address = "0xd7d2e492e6dda0013e9062f00327a06fdb722488".parse().unwrap();
        let param = build_send_param(1, addr, U256::zero(), vec![]);
        match param {
            Token::Tuple(items) => {
                // The `to` field is at index 1
                if let Token::FixedBytes(ref bytes) = items[1] {
                    assert_eq!(bytes.len(), 32, "to must be bytes32 (32 bytes)");
                    // Last 20 bytes should match the address
                    assert_eq!(&bytes[12..32], addr.as_bytes());
                } else {
                    panic!("Expected FixedBytes for `to`");
                }
            }
            _ => panic!("Expected Tuple"),
        }
    }

    #[test]
    fn test_build_fee_creates_2_element_tuple() {
        let fee = build_fee(U256::from(1000), U256::from(0));
        match fee {
            Token::Tuple(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], Token::Uint(U256::from(1000)));
                assert_eq!(items[1], Token::Uint(U256::from(0)));
            }
            _ => panic!("Fee must be a Tuple token"),
        }
    }

    #[test]
    fn test_encode_send_starts_with_correct_selector() {
        let addr: Address = "0xd7d2e492e6dda0013e9062f00327a06fdb722488".parse().unwrap();
        let param = build_send_param(1, addr, U256::zero(), vec![]);
        let fee = build_fee(U256::zero(), U256::zero());
        let encoded = encode_send(&param, &fee, addr);

        // First 4 bytes should be the send selector 0xc7c7f5b3
        assert_eq!(encoded[0], 0xc7);
        assert_eq!(encoded[1], 0xc7);
        assert_eq!(encoded[2], 0xf5);
        assert_eq!(encoded[3], 0xb3);
    }

    #[test]
    fn test_addr_to_bytes32_padding() {
        let addr: Address = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".parse().unwrap();
        let param = build_send_param(0, addr, U256::zero(), vec![]);
        match param {
            Token::Tuple(items) => {
                if let Token::FixedBytes(ref bytes) = items[1] {
                    // First 12 bytes should be zero (padding)
                    for i in 0..12 {
                        assert_eq!(bytes[i], 0, "byte {} of bytes32 padding should be 0", i);
                    }
                } else {
                    panic!("Expected FixedBytes");
                }
            }
            _ => panic!("Expected Tuple"),
        }
    }
}

async fn get_tplus_balance(provider: &Provider<Http>, wallet: Address) -> Result<U256> {
    let addr: Address = USDT_PLUS.parse()?;
    let contract = Contract::new(
        addr,
        serde_json::from_str::<ethers::abi::Abi>(BRIDGE_ABI)?,
        Arc::new(provider.clone()),
    );
    Ok(contract.method::<_, U256>("balanceOf", wallet)?.call().await?)
}

pub struct BridgeTplusTask;

#[async_trait]
impl SepoliaTask for BridgeTplusTask {
    fn name(&self) -> &str {
        "12_bridgeTplus"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let tplus_addr: Address = USDT_PLUS.parse()?;

        // --- 1. Check T+ balance ---
        let tplus_balance = get_tplus_balance(provider, address).await?;

        // --- 2. Calculate 5% of T+, round to nearest whole T+ ---
        const DEC18: u128 = 1_000_000_000_000_000_000;
        let bridge_raw = calc_pct_rounded(tplus_balance.as_u128(), 5, 100, 18);
        let whole_tplus = bridge_raw / DEC18;
        let bridge_amount = U256::from(bridge_raw);

        if whole_tplus == 0 {
            return Ok(TaskResult {
                success: false,
                message: "5% of T+ balance rounds to 0, nothing to bridge".to_string(),
            });
        }

        // --- 3. Build SendParam ---
        let extra_options = hex::decode("0003").unwrap();
        let send_param = build_send_param(DST_EID, address, bridge_amount, extra_options);

        // --- 4. Get gas fees ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // --- 5. Execute send ---
        // Use a small native fee (bridge fee on testnet is usually minimal)
        // The actual fee should be queried via quoteSend, but we use a fixed estimate
        let native_fee = U256::from(200_000_000_000_000u128); // 0.0002 ETH
        let fee = build_fee(native_fee, U256::zero());
        let send_calldata = encode_send(&send_param, &fee, address);

        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        let tx = ethers::types::TransactionRequest::default()
            .to(tplus_addr)
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
                "Bridged {} T+ → Base Sepolia (tx: {:?}) | fee: {:.6} ETH",
                whole_tplus, tx_hash, native_fee.as_u128() as f64 / 1e18
            ),
        })
    }
}
