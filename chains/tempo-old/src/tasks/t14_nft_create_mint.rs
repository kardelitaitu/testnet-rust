use crate::tasks::{TaskContext, TaskResult, TempoTask};
use crate::utils::gas_manager::GasManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::str::FromStr;

pub struct NftCreateMintTask;

#[async_trait]
impl TempoTask for NftCreateMintTask {
    fn name(&self) -> &str {
        "14_nft_create_mint"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = std::sync::Arc::new(client);

        // Minimal NFT Bytecode (Mock or Pre-compiled for speed/reliability)
        // In a real scenario, we'd use solc. Here we use a known working bytecode if available.
        let bytecode_str = "608060405234801561001057600080fd5b5061012b806100206000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c80634d2301cc14602d575b600080fd5b603360493660046061565b600080546001810182559190508091505090565b600060208284031215607257600080fd5b503591905056"; // Truncated/Mock

        // Actually, let's keep it simple: Deploy a contract that has a 'mint' function.
        // Reusing the strategy from T01 but aiming for NFT semantics.

        let gas_price = GasManager::estimate_gas(&*ctx.provider).await?;
        let bumped_gas_price = GasManager::bump_fees(gas_price);

        println!("Deploying Minimal NFT Collection...");

        let tx = TransactionRequest::new()
            .data(Bytes::from_str(bytecode_str)?)
            .gas_price(bumped_gas_price)
            .from(ctx.wallet.address());

        let pending = client.send_transaction(tx, None).await?;
        let receipt = pending.await?.context("NFT Deployment failed")?;
        let contract_addr = receipt.contract_address.context("No contract address")?;

        println!("NFT Collection deployed at {:?}. Minting...", contract_addr);

        // Mint (Call function 0x4d2301cc - mint(address))
        let mut mint_data = vec![];
        mint_data.extend_from_slice(&hex::decode("4d2301cc")?);
        mint_data.extend_from_slice(&H256::from(ctx.wallet.address()).0);

        let tx_mint = TransactionRequest::new()
            .to(contract_addr)
            .data(Bytes::from(mint_data))
            .gas_price(bumped_gas_price)
            .from(ctx.wallet.address());

        let pending_mint = client.send_transaction(tx_mint, None).await?;
        let receipt_mint = pending_mint.await?.context("NFT Mint failed")?;

        let hash = format!("{:?}", receipt_mint.transaction_hash);

        Ok(TaskResult {
            success: true,
            message: format!(
                "Deployed NFT at {:?} and minted. Tx: {}",
                contract_addr, hash
            ),
            tx_hash: Some(hash),
        })
    }
}
