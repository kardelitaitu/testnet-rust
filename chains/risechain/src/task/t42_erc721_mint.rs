use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

pub struct ERC721MintTask;

impl ERC721MintTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for ERC721MintTask {
    fn name(&self) -> &str {
        "42_erc721Mint"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.clone()));

        // Deploy TestNFT
        let bytecode_str = include_str!("../../contracts/TestNFT_bytecode.txt").trim();
        let mut bytecode = hex::decode(bytecode_str).context("Failed to decode bytecode")?;

        let encoded_args = ethers::abi::encode(&[
            ethers::abi::Token::String("TestNFT".to_string()),
            ethers::abi::Token::String("TNFT".to_string()),
        ]);
        bytecode.extend(encoded_args);

        let abi_str = include_str!("../../contracts/TestNFT_abi.txt").trim();
        let abi: abi::Abi = serde_json::from_str(abi_str).context("Failed to parse ABI")?;

        let tx = Eip1559TransactionRequest::new()
            .data(Bytes::from(bytecode))
            .gas(ctx.gas_manager.limit_deploy())
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let pending_deploy = client.send_transaction(tx, None).await?;
        let deploy_receipt = pending_deploy
            .await?
            .context("Failed to get deploy receipt")?;
        if deploy_receipt.status != Some(U64::from(1)) {
            return Err(anyhow::anyhow!(
                "Deployment failed. Receipt: {:?}",
                deploy_receipt
            ));
        }
        let nft_address = deploy_receipt
            .contract_address
            .context("No contract address")?;
        let contract = Contract::new(nft_address, abi, client.clone());
        debug!("Deployed TestNFT at {:?}", nft_address);

        let total_before: U256 = contract
            .method("totalSupply", ())?
            .call()
            .await
            .context("Failed to get total supply")?;

        let mut rng = OsRng;
        let token_id: u64 = rng.gen();
        let token_uri = format!("https://api.rise-testnet.io/metadata/{}", token_id);

        let mint_data = contract.encode("mint", (address, token_uri.clone()))?;

        let mint_tx = Eip1559TransactionRequest::new()
            .to(nft_address)
            .data(mint_data)
            .gas(1_000_000) // Explicit generous limit for minting
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let pending_tx = client.send_transaction(mint_tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        let total_after: U256 = contract
            .method("totalSupply", ())?
            .call()
            .await
            .context("Failed to get total supply after")?;

        Ok(TaskResult {
            success: receipt.status == Some(U64::from(1)),
            message: format!(
                "ERC721 Mint: Token minted with URI: {}. Total supply: {} -> {}",
                token_uri, total_before, total_after
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
