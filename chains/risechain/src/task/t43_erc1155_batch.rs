use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

pub struct ERC1155BatchTask;

impl ERC1155BatchTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for ERC1155BatchTask {
    fn name(&self) -> &str {
        "43_erc1155Batch"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.clone()));

        // Deploy TestERC1155
        let bytecode_str = include_str!("../../contracts/TestERC1155_bytecode.txt").trim();
        let bytecode = hex::decode(bytecode_str).context("Failed to decode bytecode")?;
        let abi_str = include_str!("../../contracts/TestERC1155_abi.txt").trim();
        let abi: abi::Abi = serde_json::from_str(abi_str).context("Failed to parse ABI")?;

        let tx = Eip1559TransactionRequest::new()
            .data(Bytes::from(bytecode))
            .gas(3000000)
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
        debug!("Deployed TestERC1155 at {:?}", nft_address);

        let mut rng = OsRng;
        let ids: Vec<U256> = (0..5)
            .map(|_| U256::from(rng.gen_range(1..10000)))
            .collect();
        let amounts: Vec<U256> = (0..5).map(|_| U256::from(rng.gen_range(1..100))).collect();
        let data = format!("Batch mint for {:?}", address);

        // mintBatch(address to, uint256[] ids, uint256[] amounts, bytes data)
        let mint_data = contract.encode(
            "mintBatch",
            (
                address,
                ids.clone(),
                amounts.clone(),
                Bytes::from(data.as_bytes().to_vec()),
            ),
        )?;

        let mint_tx = Eip1559TransactionRequest::new()
            .to(nft_address)
            .data(mint_data)
            .gas(1_000_000)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let pending_tx = client.send_transaction(mint_tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        let total_minted: U256 = amounts.iter().fold(U256::zero(), |acc, &x| acc + x);

        Ok(TaskResult {
            success: receipt.status == Some(U64::from(1)),
            message: format!(
                "ERC1155 Batch Mint: {} tokens minted across {} IDs. Total: {} units",
                ids.len(),
                ids.len(),
                total_minted
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
