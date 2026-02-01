use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;

pub struct UUPSProxyTask;

impl UUPSProxyTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for UUPSProxyTask {
    fn name(&self) -> &str {
        "37_uupsProxy"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let _factory_address: Address = "0x13b0D85CcB8bf860b6b79AF3029fCA081AE9beF2"
            .parse()
            .context("Invalid Create2Deployer address")?;

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let implementation_abi_json = r#"[
            {"type":"constructor","stateMutability":"nonpayable","inputs":[{"name":"_initialValue","type":"uint256"}]},
            {"type":"function","name":"getValue","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]},
            {"type":"function","name":"setValue","stateMutability":"nonpayable","inputs":[{"name":"_value","type":"uint256"}],"outputs":[]},
            {"type":"function","name":"version","stateMutability":"pure","inputs":[],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;

        let implementation_bytecode = "0x608060405260018055348015601357600080fd5b5060405161021e38038061021e8339810160408190526030916037565b600055604f565b600060208284031215604857600080fd5b5051919050565b6101c08061005e6000396000f3fe608060405234801561001057600080fd5b506004361061007d5760003560e01c806352d1902d1161005b57806352d1902d146100d457806354fd4d50146100fa57806355241077146101035780635c60da1b1461011657600080fd5b806320965255146100825780633659cfe6146100995780633fa4f245146100cb575b600080fd5b6000545b6040519081526020015b60405180910390f35b6100c96100a7366004610141565b600280546001600160a01b0319166001600160a01b0392909216919091179055565b005b61008660005481565b7fc5f16f0fcc639fa48a6947836d9850f504798523bf8c9a3a87d5876cf622bcf7610086565b61008660015481565b6100c9610111366004610171565b600055565b600254610129906001600160a01b031681565b6040516001600160a01b039091168152602001610090565b60006020828403121561015357600080fd5b81356001600160a01b038116811461016a57600080fd5b9392505050565b60006020828403121561018357600080fd5b503591905056fea2646970667358221220383dd5d8a7af200405a518702c65fd9901581130e05268926b2584a59136ffdc64736f6c63430008210033";

        // Remove factory usage to fix task execution
        /*
        let factory_abi_json = r#"[
            {"type":"function","name":"deploy(uint256,bytes32,bytes)","stateMutability":"nonpayable","inputs":[{"name":"salt","type":"uint256"},{"name":"bytecodeHash","type":"bytes32"},{"name":"data","type":"bytes"}],"outputs":[]}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(factory_abi_json)?;
        let factory = Contract::new(factory_address, abi, Arc::new(provider.clone()));
        */

        let salt = 12345u64;

        let clean_bytecode = implementation_bytecode.trim().trim_start_matches("0x");
        let mut impl_bytecode_vec =
            hex::decode(clean_bytecode).context("Failed to decode bytecode")?;

        // Append constructor args (uint256)
        let encoded_args = ethers::abi::encode(&[ethers::abi::Token::Uint(U256::from(salt))]);
        impl_bytecode_vec.extend(encoded_args);

        let client = std::sync::Arc::new(SignerMiddleware::new(
            std::sync::Arc::new(provider.clone()),
            wallet.clone(),
        ));

        // Direct deployment
        let impl_tx = Eip1559TransactionRequest::new()
            .data(impl_bytecode_vec)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let impl_pending = client.send_transaction(impl_tx, None).await?;
        let impl_receipt = impl_pending
            .await?
            .context("Failed to get implementation deployment receipt")?;

        let implementation_address = impl_receipt
            .contract_address
            .context("No implementation address")?;

        let impl_abi: abi::Abi = serde_json::from_str(implementation_abi_json)?;
        let proxy_contract =
            Contract::new(implementation_address, impl_abi, Arc::new(provider.clone()));

        let current_value: U256 = proxy_contract
            .method("getValue", ())?
            .call()
            .await
            .context("Failed to get proxy value")?;

        let version: U256 = proxy_contract
            .method("version", ())?
            .call()
            .await
            .context("Failed to get version")?;

        Ok(TaskResult {
            success: impl_receipt.status == Some(U64::from(1)),
            message: format!(
                "Implementation deployed: {:?}. Value: {}, Version: {}",
                implementation_address, current_value, version
            ),
            tx_hash: Some(format!("{:?}", impl_receipt.transaction_hash)),
        })
    }
}
