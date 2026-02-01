use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

pub struct TransparentProxyTask;

impl TransparentProxyTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for TransparentProxyTask {
    fn name(&self) -> &str {
        "38_transparentProxy"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let factory_address: Address = "0x4e59b44847b379578588920ca78fbf26c0b4956c"
            .parse()
            .context("Invalid Create2Deployer address")?;

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let impl_abi_json = r#"[
            {"type":"constructor","stateMutability":"nonpayable","inputs":[{"name":"_admin","type":"address"}]},
            {"type":"function","name":"getAdmin","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"address"}]},
            {"type":"function","name":"getImplementation","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"address"}]},
            {"type":"function","name":"setValue","stateMutability":"nonpayable","inputs":[{"name":"_value","type":"uint256"}],"outputs":[]},
            {"type":"function","name":"getValue","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"uint256"}]}
        ]"#;

        let impl_bytecode = "0x6080604052348015600f57600080fd5b506040516101f33803806101f3833981016040819052602c916050565b600080546001600160a01b0319166001600160a01b0392909216919091179055607e565b600060208284031215606157600080fd5b81516001600160a01b0381168114607757600080fd5b9392505050565b6101668061008d6000396000f3fe608060405234801561001057600080fd5b506004361061007d5760003560e01c80635c60da1b1161005b5780635c60da1b146100b75780636e9960c3146100e2578063aaf10f42146100f3578063f851a4401461010457600080fd5b806320965255146100825780633fa4f2451461009957806355241077146100a2575b600080fd5b6002545b6040519081526020015b60405180910390f35b61008660025481565b6100b56100b0366004610117565b600255565b005b6001546100ca906001600160a01b031681565b6040516001600160a01b039091168152602001610090565b6000546001600160a01b03166100ca565b6001546001600160a01b03166100ca565b6000546100ca906001600160a01b031681565b60006020828403121561012957600080fd5b503591905056fea26469706673582212209da41bf25da902b77696685abec48be665e2268d4c1ae1729442bf60960f400864736f6c63430008210033";

        let factory_abi_json = r#"[
            {"type":"function","name":"deploy","stateMutability":"nonpayable","inputs":[{"name":"salt","type":"uint256"},{"name":"bytecodeHash","type":"bytes32"},{"name":"data","type":"bytes"}],"outputs":[]}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(factory_abi_json)?;
        let _factory = Contract::new(factory_address, abi, Arc::new(provider.clone()));

        let _salt = 54321u64;

        let mut impl_data = hex::decode(&impl_bytecode.trim_start_matches("0x")).unwrap();
        let encoded_admin = ethers::abi::encode(&[ethers::abi::Token::Address(address)]);
        impl_data.extend(encoded_admin);

        let _impl_bytecode_hash = H256::from_slice(&ethers::utils::keccak256(&impl_data));

        let client = std::sync::Arc::new(SignerMiddleware::new(
            std::sync::Arc::new(provider.clone()),
            wallet.clone(),
        ));

        // Direct deployment (bypass factory to avoid ABI issues)
        debug!("Deploying implementation directly...");
        let impl_tx = Eip1559TransactionRequest::new()
            .data(impl_data.clone())
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let impl_pending = client.send_transaction(impl_tx, None).await?;
        debug!("Tx sent, waiting for receipt...");
        let impl_receipt = impl_pending
            .await?
            .context("Failed to get implementation receipt")?;

        debug!(
            "Receipt obtained. Status: {:?}, Contract Address: {:?}",
            impl_receipt.status, impl_receipt.contract_address
        );

        let implementation_address = impl_receipt
            .contract_address
            .context("No implementation address")?;

        let impl_abi: abi::Abi = serde_json::from_str(impl_abi_json)?;

        let impl_contract =
            Contract::new(implementation_address, impl_abi, Arc::new(provider.clone()));

        let current_value: U256 = impl_contract
            .method("getValue", ())?
            .call()
            .await
            .context("Failed to get value")?;

        let admin: Address = impl_contract
            .method("getAdmin", ())?
            .call()
            .await
            .context("Failed to get admin")?;

        Ok(TaskResult {
            success: impl_receipt.status == Some(U64::from(1)),
            message: format!(
                "Transparent Impl deployed: {:?}. Admin: {:?}. Storage value: {}",
                implementation_address, admin, current_value
            ),
            tx_hash: Some(format!("{:?}", impl_receipt.transaction_hash)),
        })
    }
}
