use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use crate::utils::nonce_manager_2d::TempoNonceManager;
use crate::utils::tempo_tx::TempoCall;
use crate::utils::tempo_tx_sender::TempoBatchSender;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::Rng;
use std::str::FromStr;
use std::sync::Arc;

ethers::contract::abigen!(
    ITIP20Factory,
    r#"[
        function createToken(string name, string symbol, string currency, address quoteToken, address admin, bytes32 salt) returns (address)
        event TokenCreated(address indexed token, string name, string symbol, string currency, address quoteToken, address admin, bytes32 salt)
    ]"#
);

ethers::contract::abigen!(
    ITIP20Mintable,
    r#"[
        function mint(address to, uint256 amount)
        function grantRole(bytes32 role, address account)
    ]"#
);

pub struct CreateStableTask;

#[async_trait]
impl TempoTask for CreateStableTask {
    fn name(&self) -> &str {
        "04_create_stable"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let factory_address = Address::from_str("0x20FC000000000000000000000000000000000000")?;
        let quote_token = Address::from_str("0x20C0000000000000000000000000000000000000")?;

        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = Arc::new(client);

        let factory = ITIP20Factory::new(factory_address, client.clone());
        let address = ctx.wallet.address();

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        let nonce_manager = TempoNonceManager::new(ctx.provider.clone());

        let name = generate_random_name();
        let symbol = generate_random_symbol();
        let currency = "USD".to_string();

        let salt = {
            let mut rng = rand::thread_rng();
            let mut salt = [0u8; 32];
            rng.fill(&mut salt);
            salt
        };

        println!("Creating {} ({})...", name, symbol);

        let nonce_create = nonce_manager.get_next_protocol_nonce(address).await?;
        let _nonce_grant = nonce_manager.get_next_protocol_nonce(address).await?;
        let _nonce_mint = nonce_manager.get_next_protocol_nonce(address).await?;

        let tx_create = factory
            .create_token(
                name.clone(),
                symbol.clone(),
                currency.clone(),
                quote_token,
                address,
                salt,
            )
            .gas(5_000_000)
            .gas_price(bumped_gas_price)
            .nonce(U256::from(nonce_create));

        let pending_create = tx_create.send().await?;
        println!("Create tx sent: {:?}", pending_create.tx_hash());

        let receipt = pending_create.await?.context("Create transaction failed")?;
        println!("Create receipt: {:?}", receipt.transaction_hash);

        let mut token_address = Address::zero();
        for log in &receipt.logs {
            if log.address == factory_address && log.topics.len() > 1 {
                token_address = Address::from_slice(&log.topics[1].as_bytes()[12..32]);
                break;
            }
        }

        if token_address == Address::zero() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Token created but address not found in logs. Tx: {:?}",
                    receipt.transaction_hash
                ),
                tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
            });
        }

        println!("Token deployed at: {:?}", token_address);

        let issuer_role = ethers::utils::keccak256("ISSUER_ROLE".as_bytes());
        let token_contract = ITIP20Mintable::new(token_address, client.clone());

        let grant_calldata = token_contract
            .grant_role(issuer_role, address)
            .calldata()
            .context("Failed to encode grant")?;

        let mint_amount = U256::from(100_000u64) * U256::exp10(18);
        let mint_calldata = token_contract
            .mint(address, mint_amount)
            .calldata()
            .context("Failed to encode mint")?;

        let batch_sender = TempoBatchSender::new(ctx.provider.clone(), ctx.wallet.clone());

        let nonce_batch = nonce_manager.get_next_protocol_nonce(address).await?;

        let calls = vec![
            TempoCall::new(token_address, grant_calldata),
            TempoCall::new(token_address, mint_calldata),
        ];

        println!(
            "Sending grant + mint as native batch (nonce: {})...",
            nonce_batch
        );

        let batch_receipt = batch_sender
            .send_batch(calls, nonce_batch)
            .await
            .context("Batch transaction failed")?;

        let batch_hash = format!("{:?}", batch_receipt.transaction_hash);
        println!("Batch receipt: {:?}", batch_hash);

        if let Some(db) = ctx.db.as_ref() {
            let _ = db
                .log_asset_creation(
                    &format!("{:?}", address),
                    &format!("{:?}", token_address),
                    "stablecoin",
                    &name,
                    &symbol,
                )
                .await;
        }

        Ok(TaskResult {
            success: true,
            message: format!(
                "Created {} ({}) at {:?}. Create: {:?}, Batch: {}",
                name, symbol, token_address, receipt.transaction_hash, batch_hash
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}

fn generate_random_name() -> String {
    let prefixes = [
        "Alpha", "Beta", "Gamma", "Delta", "Omega", "Nova", "Stellar", "Crypto", "Digital", "Meta",
    ];
    let suffixes = [
        "Dollar", "Coin", "Cash", "Pay", "Money", "Finance", "Capital", "Fund",
    ];
    let mut rng = rand::thread_rng();
    format!(
        "{} {}",
        prefixes[rng.gen_range(0..prefixes.len())],
        suffixes[rng.gen_range(0..suffixes.len())]
    )
}

fn generate_random_symbol() -> String {
    let letters = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut rng = rand::thread_rng();
    let mut s = String::new();
    for _ in 0..3 {
        s.push(letters[rng.gen_range(0..letters.len())] as char);
    }
    s + "USD"
}
