use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use tracing::debug;

pub struct DeployFactoryTask;

impl DeployFactoryTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for DeployFactoryTask {
    fn name(&self) -> &str {
        "59_deployFactory"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();
        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;

        // Bytecode of SimpleFactory (from previous verification)
        let factory_bytecode = "6080604052348015600f57600080fd5b5061020f8061001f6000396000f3fe608060405234801561001057600080fd5b506004361061002b5760003560e01c806361ff715f14610030575b600080fd5b61004361003e366004610116565b61005f565b6040516001600160a01b03909116815260200160405180910390f35b6000828251602084016000f590506001600160a01b0381166100b85760405162461bcd60e51b815260206004820152600e60248201526d10dc99585d194c8819985a5b195960921b604482015260640160405180910390fd5b604080516001600160a01b0383168152602081018590527fb03c53b28e78a88e31607a27e1fa48234dce28d5d9d9ec7b295aeb02e674a1e1910160405180910390a192915050565b634e487b7160e01b600052604160045260246000fd5b6000806040838503121561012957600080fd5b82359150602083013567ffffffffffffffff81111561014757600080fd5b8301601f8101851361015857600080fd5b803567ffffffffffffffff81111561017257610172610100565b604051601f8201601f19908116603f0116810167ffffffffffffffff811182821017156101a1576101a1610100565b6040528181528282016020018710156101b957600080fd5b81602084016020830137600060208383010152809350505050925092905056fea26469706673582212203d752ff8928077cad5caebcfb0833f2e92dd8a67636e26a88b59280bc1e801cc64736f6c63430008210033";
        let factory_bytes = hex::decode(factory_bytecode)?;

        let client = std::sync::Arc::new(SignerMiddleware::new(
            std::sync::Arc::new(provider.clone()),
            wallet.clone(),
        ));

        let tx = Eip1559TransactionRequest::new()
            .data(factory_bytes)
            .gas(2000000)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        debug!("Deploying Universal Create2 Factory...");
        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx.await?.context("Failed to get receipt")?;
        let factory_address = receipt.contract_address.context("No contract address")?;

        debug!("--------------------------------------------------");
        debug!("Universal Factory Deployed At: {:?}", factory_address);
        debug!("--------------------------------------------------");

        Ok(TaskResult {
            success: true,
            message: format!("Factory Deployed: {:?}", factory_address),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
