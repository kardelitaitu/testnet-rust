use crate::task::{Task, TaskContext, TaskResult};
use crate::utils::address_cache::AddressCache;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::{seq::SliceRandom, Rng};
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tracing::debug;

#[derive(Default)]
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
        let amount: u64 = rng.gen_range(1..50);

        let gas_price = U256::from(1_100_000_000u64);
        let deploy_gas_limit = 3_000_000u64;
        let mint_gas_limit = 500_000u64;
        let transfer_gas_limit = 500_000u64;

        // If using existing contract, no deploy needed
        let estimated_gas = if ctx.db.is_some() {
            U256::from(mint_gas_limit + transfer_gas_limit) * gas_price
        } else {
            U256::from(deploy_gas_limit + mint_gas_limit + transfer_gas_limit) * gas_price
        };

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
        let nonce_manager = crate::utils::nonce_manager::SimpleNonceManager::new(Arc::new(provider.clone()), address);

        let client = SignerMiddleware::new(provider.clone(), wallet.clone());

        // 3. Get contract from DB or deploy new one
        let contract_address = if let Some(db) = &ctx.db {
            match db.get_all_assets_by_type("ERC1155").await {
                Ok(contracts) if !contracts.is_empty() => {
                    // Use existing contract
                    let addr_str = contracts.choose(&mut rng).context("Failed to pick contract")?;
                    debug!("Using existing ERC1155: {}", addr_str);
                    Some(addr_str.parse::<Address>().context("Invalid address in DB")?)
                },
                _ => None,
            }
        } else {
            None
        };

        // Generate token_id for minting
        let token_id: u64 = rng.gen_range(1_000_000..9_999_999);

        // Load ABI
        let abi_str = include_str!("../../contracts/TestERC1155_abi.txt").trim();
        let abi: abi::Abi = serde_json::from_str(abi_str).context("Failed to parse ABI")?;

        let contract_address = match contract_address {
            Some(addr) => addr,
            None => {
                // Deploy new ERC1155
                let bytecode_str = include_str!("../../contracts/TestERC1155_bytecode.txt").trim();
                let bytecode = hex::decode(bytecode_str).context("Failed to decode bytecode")?;

                let deploy_nonce = nonce_manager.next().await?;
                let deploy_tx = TransactionRequest::new()
                    .data(Bytes::from(bytecode))
                    .gas(deploy_gas_limit)
                    .gas_price(gas_price)
                    .nonce(deploy_nonce)
                    .from(address);

                let pending_deploy = client.send_transaction(deploy_tx, None).await;
                match pending_deploy {
                    Ok(pending) => {
                        let tx_hash = format!("{:?}", pending.tx_hash());
                        match timeout(Duration::from_secs(90), pending).await {
                            Ok(Ok(Some(receipt))) if receipt.status == Some(U64::from(1)) => {
                                let addr = receipt.contract_address.context("No contract address in receipt")?;
                                // Save to DB
                                if let Some(db) = &ctx.db {
                                    let _ = db
                                        .log_asset_creation(
                                            &format!("{:?}", address),
                                            &format!("{:?}", addr),
                                            "ERC1155",
                                            "TestERC1155",
                                            "T1155",
                                        )
                                        .await;
                                }
                                addr
                            },
                            Ok(Ok(Some(_))) => {
                                let _ = nonce_manager.resync().await;
                                return Ok(TaskResult {
                                    success: false,
                                    message: format!("ERC1155 deploy reverted (tx: {})", tx_hash),
                                    tx_hash: Some(tx_hash),
                                });
                            },
                            Ok(Ok(None)) | Err(_) => {
                                let _ = nonce_manager.resync().await;
                                return Ok(TaskResult {
                                    success: false,
                                    message: format!("ERC1155 deploy timed out (tx: {})", tx_hash),
                                    tx_hash: Some(tx_hash),
                                });
                            },
                            Ok(Err(e)) => {
                                let _ = nonce_manager.resync().await;
                                return Ok(TaskResult {
                                    success: false,
                                    message: format!("ERC1155 deploy receipt failed (tx: {}): {}", tx_hash, e),
                                    tx_hash: Some(tx_hash),
                                });
                            },
                        }
                    },
                    Err(e) => {
                        debug!("ERC1155 deploy submit failed, resyncing nonce: {}", e);
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("Failed to submit ERC1155 deploy tx: {}", e),
                            tx_hash: None,
                        });
                    },
                }
            },
        };

        debug!("Using ERC1155 at {:?}", contract_address);
        let contract = Contract::new(contract_address, abi, Arc::new(provider.clone()));

        // 4. Mint to self (with timeout, must wait to ensure tokens exist before transfer)
        let mint_nonce = nonce_manager.next().await?;
        let mint_data = contract.encode(
            "mint",
            (address, U256::from(token_id), U256::from(amount), Bytes::from(vec![])),
        )?;

        let mint_tx = TransactionRequest::new()
            .to(contract_address)
            .data(mint_data)
            .gas(mint_gas_limit)
            .gas_price(gas_price)
            .nonce(mint_nonce)
            .from(address);

        let pending_mint = client.send_transaction(mint_tx, None).await;
        match pending_mint {
            Ok(pending) => {
                let mint_tx_hash = format!("{:?}", pending.tx_hash());
                match timeout(Duration::from_secs(90), pending).await {
                    Ok(Ok(Some(receipt))) if receipt.status == Some(U64::from(1)) => {},
                    Ok(Ok(Some(_))) => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("ERC1155 mint reverted (tx: {})", mint_tx_hash),
                            tx_hash: Some(mint_tx_hash),
                        });
                    },
                    Ok(Ok(None)) | Err(_) => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("ERC1155 mint timed out or unavailable (tx: {})", mint_tx_hash),
                            tx_hash: Some(mint_tx_hash),
                        });
                    },
                    Ok(Err(e)) => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("ERC1155 mint receipt failed (tx: {}): {}", mint_tx_hash, e),
                            tx_hash: Some(mint_tx_hash),
                        });
                    },
                }
            },
            Err(e) => {
                debug!("ERC1155 mint submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit ERC1155 mint tx: {}", e),
                    tx_hash: None,
                });
            },
        }

        debug!("Minted {} tokens of id {} to self", amount, token_id);

        // 5. Transfer (fire-and-forget)
        let transfer_nonce = nonce_manager.next().await?;
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

        let transfer_tx = TransactionRequest::new()
            .to(contract_address)
            .data(transfer_data)
            .gas(transfer_gas_limit)
            .gas_price(gas_price)
            .nonce(transfer_nonce)
            .from(address);

        let pending_transfer = client.send_transaction(transfer_tx, None).await;

        match pending_transfer {
            Ok(pending) => Ok(TaskResult {
                success: true,
                message: format!(
                    "Transferred {} of ERC1155 #{} to {:?} at {:?}",
                    amount, token_id, recipient, contract_address
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("ERC1155 transfer submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit ERC1155 transfer tx: {}", e),
                    tx_hash: None,
                })
            },
        }
    }
}
