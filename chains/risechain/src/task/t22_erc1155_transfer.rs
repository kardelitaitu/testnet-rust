use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

pub struct Erc1155TransferTask;

impl Erc1155TransferTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for Erc1155TransferTask {
    fn name(&self) -> &str {
        "22_erc1155Transfer"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // Get random recipient from address cache
        let recipient = AddressCache::get_random().context("Failed to get random address")?;

        let mut rng = OsRng;
        let token_id: u64 = rng.gen_range(1_000_000..9_999_999);
        let amount: u64 = rng.gen_range(1..50);

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
        let contract_address = deploy_receipt
            .contract_address
            .context("No contract address")?;
        debug!("Deployed TestERC1155 at {:?}", contract_address);

        let contract = Contract::new(contract_address, abi, client.clone());

        // Mint to self first
        // Mint to self first
        let mint_data = contract.encode(
            "mint",
            (
                address,
                U256::from(token_id),
                U256::from(amount),
                Bytes::from(vec![]),
            ),
        )?;
        let mint_tx = Eip1559TransactionRequest::new()
            .to(contract_address)
            .data(mint_data)
            .gas(500_000)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let pending_mint = client.send_transaction(mint_tx, None).await?;
        pending_mint.await?.context("Failed to mint tokens")?;
        debug!("Minted {} tokens of id {} to self", amount, token_id);

        // Transfer
        // Transfer
        let transfer_data = contract.encode(
            "safeTransferFrom",
            (
                address,
                recipient,
                U256::from(token_id),
                U256::from(amount),
                Bytes::from(vec![]),
            ),
        )?;
        let transfer_tx = Eip1559TransactionRequest::new()
            .to(contract_address)
            .data(transfer_data)
            .gas(500_000)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let pending_tx = client.send_transaction(transfer_tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transfer receipt")?;

        Ok(TaskResult {
            success: receipt.status == Some(U64::from(1)),
            message: format!(
                "Transferred {} of ERC1155 #{} to {:?}",
                amount, token_id, recipient
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
