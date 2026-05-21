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

/// USDC+ (C+) on Base Sepolia — the OFT token we bridge back
const USDC_PLUS_BASE: &str = "0x92f36e427a9579fe1356f19c74eb5d64beae8930";
/// Ethereum Sepolia LayerZero endpoint ID
const DST_EID: u32 = 40161;

/// Hardcoded native fee for the bridge (~0.0001 ETH)
const NATIVE_FEE: u128 = 103_699_056_274_284;

/// ABI for balanceOf + approve + allowance
const BRIDGE_ABI: &str = r#"[
    {"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"balance","type":"uint256"}],"type":"function"},
    {"constant":false,"inputs":[{"name":"spender","type":"address"},{"name":"value","type":"uint256"}],"name":"approve","outputs":[{"name":"","type":"bool"}],"type":"function"},
    {"constant":true,"inputs":[{"name":"_owner","type":"address"},{"name":"_spender","type":"address"}],"name":"allowance","outputs":[{"name":"","type":"uint256"}],"type":"function"}
]"#;

/// Verified send selector from working tx
const SEND_SELECTOR: [u8; 4] = [0xc7, 0xc7, 0xf5, 0xb3];

/// Build the SendParam tuple for LayerZero OFT send
fn build_send_param(dst_eid: u32, to: Address, amount: U256, extra_options: Vec<u8>) -> Token {
    let to_bytes32 = {
        let mut b = [0u8; 32];
        let addr: [u8; 20] = to.into();
        b[12..32].copy_from_slice(&addr);
        b
    };

    Token::Tuple(vec![
        Token::Uint(U256::from(dst_eid)),       // dstEid (uint32 padded)
        Token::FixedBytes(to_bytes32.to_vec()), // to (bytes32)
        Token::Uint(amount),                    // amountLD
        Token::Uint(amount),                    // minAmountLD
        Token::Bytes(extra_options),            // extraOptions
        Token::Bytes(Vec::new()),               // composeMsg
        Token::Bytes(Vec::new()),               // oftCmd
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

async fn get_cplus_balance(provider: &Provider<Http>, wallet: Address) -> Result<U256> {
    let addr: Address = USDC_PLUS_BASE.parse()?;
    let contract = Contract::new(
        addr,
        serde_json::from_str::<ethers::abi::Abi>(BRIDGE_ABI)?,
        Arc::new(provider.clone()),
    );
    Ok(contract
        .method::<_, U256>("balanceOf", wallet)?
        .call()
        .await?)
}

pub struct BridgeBackCplusTask;

#[async_trait]
impl SepoliaTask for BridgeBackCplusTask {
    fn name(&self) -> &str {
        "17_bridgeBackCplus"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let wallet = ctx.wallet;
        let address = wallet.address();
        let provider = &ctx.provider;

        let cplus_addr: Address = USDC_PLUS_BASE.parse()?;

        // --- 1. Check C+ balance on Base Sepolia ---
        let cplus_balance = get_cplus_balance(provider, address).await?;

        // --- 2. Calculate 5% of C+ balance, round to nearest whole C+ ---
        const DEC18: u128 = 1_000_000_000_000_000_000;
        let bridge_raw = calc_pct_rounded(cplus_balance.as_u128(), 5, 100, 18);
        let whole_cplus = bridge_raw / DEC18;
        let bridge_amount = U256::from(bridge_raw);

        if whole_cplus == 0 {
            return Ok(TaskResult {
                success: false,
                message: "5% of C+ balance on Base Sepolia rounds to 0, nothing to bridge"
                    .to_string(),
            });
        }

        // --- 3. Build SendParam ---
        let extra_options = hex::decode("0003").unwrap();
        let send_param = build_send_param(DST_EID, address, bridge_amount, extra_options);

        // --- 4. Get gas fees ---
        let (max_fee, _priority_fee) = ctx.gas_manager.get_fees().await?;

        // --- 5. Execute send ---
        let native_fee = U256::from(NATIVE_FEE);
        let fee = build_fee(native_fee, U256::zero());
        let send_calldata = encode_send(&send_param, &fee, address);

        let middleware = SignerMiddleware::new(provider.clone(), wallet.clone());

        let tx = ethers::types::TransactionRequest::default()
            .to(cplus_addr)
            .data(send_calldata)
            .gas(350_000)
            .gas_price(max_fee)
            .value(native_fee);

        let pending_tx = middleware
            .send_transaction(tx, None)
            .await
            .context("Failed to send bridge-back tx")?;
        let tx_hash = pending_tx.tx_hash();

        let receipt = pending_tx
            .confirmations(1)
            .interval(Duration::from_millis(500))
            .await?;

        let success = receipt.is_some_and(|r| r.status == Some(1.into()));
        Ok(TaskResult {
            success,
            message: format!(
                "Bridged {} C+ from Base Sepolia → Eth Sepolia (tx: {:?}) | fee: {:.6} ETH",
                whole_cplus,
                tx_hash,
                NATIVE_FEE as f64 / 1e18
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_send_param_back_cplus() {
        let addr: Address = "0x11731e95c1423cd570194f07eeef606bf2d4c0ba"
            .parse()
            .unwrap();
        let param = build_send_param(40161, addr, U256::from(1000), vec![0x00, 0x03]);
        match param {
            Token::Tuple(items) => {
                assert_eq!(items.len(), 7);
                assert_eq!(items[0], Token::Uint(U256::from(40161)));
                assert!(matches!(items[1], Token::FixedBytes(_)));
                assert_eq!(items[2], Token::Uint(U256::from(1000)));
                assert_eq!(items[3], Token::Uint(U256::from(1000)));
                assert_eq!(items[4], Token::Bytes(vec![0x00, 0x03]));
                assert_eq!(items[5], Token::Bytes(vec![]));
                assert_eq!(items[6], Token::Bytes(vec![]));
            }
            _ => panic!("Expected Tuple"),
        }
    }
}
