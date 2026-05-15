use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

pub struct Create2FactoryTask;

impl Create2FactoryTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for Create2FactoryTask {
    fn name(&self) -> &str {
        "36_create2Factory"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // Load factory from config or use fallback
        let factory_address: Address = if let Some(addr) = &ctx.config.create2_factory {
            addr.parse()
                .context("Invalid create2_factory address in config")?
        } else {
            "0x8628208543e2b16be283e30abec6fec7b91e5721".parse()?
        };

        let gas_price = U256::from(1_100_000_000u64);
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;
        let estimated_gas = gas_limit * gas_price;

        // 1. Balance check
        let balance = provider.get_balance(address, None).await?;
        if balance <= estimated_gas {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Insufficient TXENE for gas: need {} Wei, have {} Wei",
                    estimated_gas, balance
                ),
                tx_hash: None,
            });
        }

        // 2. Initialize Nonce Manager
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(
            Arc::new(provider.clone()),
            address,
        );
        let nonce = nonce_manager.next().await?;

        let mut rng = OsRng;
        let salt: u64 = rng.gen();
        let salt_hex = format!("0x{:x}", salt);

        // Runtime code (Minimal Proxy logic)
        let runtime_code_hex = "363d3d373d3d3d363d73bebebebebebebebebebebebebebebebebebebebebebebebebebebe5af43d82803e903d91602b57fd5bf3";
        let runtime_code = hex::decode(runtime_code_hex)?;

        // Wrap in init code loader: 3d60<len>80600a3d3981f3
        // len = 0x37 (55 bytes)
        let loader_hex = "3d603780600a3d3981f3";
        let loader = hex::decode(loader_hex)?;

        let mut init_code = loader;
        init_code.extend(runtime_code);
        let init_code_bytes = Bytes::from(init_code);

        // SimpleFactory ABI (deploy(uint256,bytes))
        let factory_abi_json = r#"[
            {"inputs":[{"internalType":"uint256","name":"salt","type":"uint256"},{"internalType":"bytes","name":"bytecode","type":"bytes"}],"name":"deploy","outputs":[{"internalType":"address","name":"addr","type":"address"}],"stateMutability":"nonpayable","type":"function"}
        ]"#;

        let abi: abi::Abi = serde_json::from_str(factory_abi_json)?;
        let factory = Contract::new(factory_address, abi, Arc::new(provider.clone()));

        // Encode call: deploy(uint256 salt, bytes bytecode)
        let deploy_data = factory.encode("deploy", (U256::from(salt), init_code_bytes))?;

        let tx = TransactionRequest::new()
            .to(factory_address)
            .data(deploy_data)
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce)
            .from(address);

        // 3. Send and wait for receipt to verify
        let client = SignerMiddleware::new(provider.clone(), wallet.clone());
        let pending_tx = client.send_transaction(tx, None).await;

        match pending_tx {
            Ok(pending) => {
                let tx_hash = format!("{:?}", pending.tx_hash());
                match pending.await {
                    Ok(Some(receipt)) if receipt.status == Some(U64::from(1)) => {
                        let deployed_info = receipt
                            .logs
                            .first()
                            .map(|l| format!("logs: {}", l.topics.len()))
                            .unwrap_or_else(|| "deployed".to_string());
                        Ok(TaskResult {
                            success: true,
                            message: format!(
                                "CREATE2 factory deploy succeeded with salt {} ({}, tx: {})",
                                salt_hex, deployed_info, tx_hash
                            ),
                            tx_hash: Some(tx_hash),
                        })
                    }
                    Ok(Some(_)) => {
                        let _ = nonce_manager.resync().await;
                        Ok(TaskResult {
                            success: false,
                            message: format!("CREATE2 factory deploy reverted (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        })
                    }
                    Ok(None) => {
                        let _ = nonce_manager.resync().await;
                        Ok(TaskResult {
                            success: false,
                            message: format!("CREATE2 factory deploy receipt unavailable (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        })
                    }
                    Err(e) => {
                        debug!("CREATE2 factory deploy receipt failed: {}", e);
                        let _ = nonce_manager.resync().await;
                        Ok(TaskResult {
                            success: false,
                            message: format!("CREATE2 factory deploy receipt error: {}", e),
                            tx_hash: Some(tx_hash),
                        })
                    }
                }
            }
            Err(e) => {
                debug!("Create2Factory tx submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit CREATE2 factory deploy tx: {}", e),
                    tx_hash: None,
                })
            }
        }
    }
}
