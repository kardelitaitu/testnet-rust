use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use tracing::debug;

pub struct VerifyCreate2Task;

impl VerifyCreate2Task {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for VerifyCreate2Task {
    fn name(&self) -> &str {
        "58_verifyCreate2"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();
        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;

        // 1. Deploy SimpleFactory
        // Bytecode of SimpleFactory
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

        debug!("Deploying SimpleFactory...");
        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx.await?.context("Failed to get receipt")?;
        let factory_address = receipt.contract_address.context("No contract address")?;
        debug!("SimpleFactory deployed at: {:?}", factory_address);

        // 2. Call deploy() on SimpleFactory
        let child_bytecode = hex::decode("600060205260206020f3")?; // Returns 32 bytes of zeros
        let abi_json = r#"[{"inputs":[{"internalType":"uint256","name":"salt","type":"uint256"},{"internalType":"bytes","name":"bytecode","type":"bytes"}],"name":"deploy","outputs":[{"internalType":"address","name":"addr","type":"address"}],"stateMutability":"nonpayable","type":"function"}]"#;
        let abi: abi::Abi = serde_json::from_str(abi_json)?;
        let contract = Contract::new(factory_address, abi, client.clone());

        let salt = U256::from(12345);
        debug!("Calling deploy with salt {}...", salt);

        let child_bytecode_bytes = Bytes::from(child_bytecode);

        let val_data = contract.encode("deploy", (salt, child_bytecode_bytes))?;

        let call_tx = Eip1559TransactionRequest::new()
            .to(factory_address)
            .data(val_data)
            .gas(500_000)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let pending_call = client.send_transaction(call_tx, None).await?;
        let call_receipt = pending_call.await?.context("Failed to get call receipt")?;

        debug!("Deploy call status: {:?}", call_receipt.status);

        let mut success = false;
        let mut deployed_addr = Address::zero();

        if call_receipt.status == Some(U64::from(1)) {
            // Check logs
            for log in call_receipt.logs {
                // Event Deployed(address addr, uint256 salt)
                // topic0 is hash of event signature
                if log.topics.len() > 0 {
                    // We assume it's our event
                    let addr_bytes = &log.data[12..32]; // First 32 bytes, address is last 20
                    deployed_addr = Address::from_slice(addr_bytes);
                    success = true;
                    debug!("Deployed address from logs: {:?}", deployed_addr);
                }
            }
        }

        if success {
            // Verify code at deployed address
            let code = provider.get_code(deployed_addr, None).await?;
            debug!("Code at deployed address: {} bytes", code.len());
            if code.len() > 0 {
                return Ok(TaskResult {
                    success: true,
                    message: format!("CREATE2 Opcode WORKS! Deployed at {:?}", deployed_addr),
                    tx_hash: Some(format!("{:?}", call_receipt.transaction_hash)),
                });
            } else {
                return Ok(TaskResult {
                    success: false,
                    message: "CREATE2 succeeded but no code found at address".to_string(),
                    tx_hash: Some(format!("{:?}", call_receipt.transaction_hash)),
                });
            }
        } else {
            return Ok(TaskResult {
                success: false,
                message: "CREATE2 transaction failed or reverted".to_string(),
                tx_hash: Some(format!("{:?}", call_receipt.transaction_hash)),
            });
        }
    }
}
