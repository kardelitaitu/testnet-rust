use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

#[derive(Default)]
pub struct PermitTokenTask;

impl PermitTokenTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for PermitTokenTask {
    fn name(&self) -> &str {
        "27_permitToken"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        let deadline = std::time::SystemTime::now()
            .checked_add(std::time::Duration::from_secs(3600))
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let amount: u128 = 1_000_000_000_000_000_000; // 1 ETH worth
        let amount_formatted = ethers::utils::format_units(amount, 18u32).unwrap_or_else(|_| amount.to_string());

        let gas_price = U256::from(1_100_000_000u64);
        let deploy_gas_limit = 3_000_000u64;
        let permit_gas_limit = 500_000u64;
        let estimated_gas = U256::from(deploy_gas_limit + permit_gas_limit) * gas_price;

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

        // 3. Deploy TestERC20Permit
        let bytecode_str = include_str!("../../contracts/TestERC20Permit_bytecode.txt").trim();
        let bytecode = hex::decode(bytecode_str).context("Failed to decode bytecode")?;
        let abi_str = include_str!("../../contracts/TestERC20Permit_abi.txt").trim();
        let abi: abi::Abi = serde_json::from_str(abi_str).context("Failed to parse ABI")?;

        let deploy_nonce = nonce_manager.next().await?;
        let deploy_tx = TransactionRequest::new()
            .data(Bytes::from(bytecode))
            .gas(deploy_gas_limit)
            .gas_price(gas_price)
            .nonce(deploy_nonce)
            .from(address);

        let pending_deploy = client.send_transaction(deploy_tx, None).await;
        let token_address = match pending_deploy {
            Ok(pending) => {
                let tx_hash = format!("{:?}", pending.tx_hash());
                match pending.await {
                    Ok(Some(receipt)) if receipt.status == Some(U64::from(1)) => {
                        receipt.contract_address.context("No contract address in receipt")?
                    },
                    _ => {
                        let _ = nonce_manager.resync().await;
                        return Ok(TaskResult {
                            success: false,
                            message: format!("Permit token deploy failed (tx: {})", tx_hash),
                            tx_hash: Some(tx_hash),
                        });
                    },
                }
            },
            Err(e) => {
                debug!("PermitToken deploy submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                return Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit permit token deploy tx: {}", e),
                    tx_hash: None,
                });
            },
        };

        debug!("Deployed TestERC20Permit at {:?}", token_address);
        let contract = Contract::new(token_address, abi, Arc::new(provider.clone()));

        let name: String = contract
            .method("name", ())?
            .call()
            .await
            .context("Failed to get name")?;
        let permit_nonce = nonce_manager.next().await?;
        let on_chain_nonce: U256 = contract
            .method("nonces", address)?
            .call()
            .await
            .context("Failed to get nonce")?;
        let domain_separator: H256 = contract
            .method("DOMAIN_SEPARATOR", ())?
            .call()
            .await
            .context("Failed to get domain separator")?;

        let permit_typehash: H256 = contract
            .method("getPermitTypeHash", ())?
            .call()
            .await
            .context("Failed to get permit typehash")?;

        let struct_hash = ethers::utils::keccak256(ethers::abi::encode(&[
            ethers::abi::Token::FixedBytes(permit_typehash.as_bytes().to_vec()),
            ethers::abi::Token::Address(address),
            ethers::abi::Token::Address(address),
            ethers::abi::Token::Uint(amount.into()),
            ethers::abi::Token::Uint(on_chain_nonce),
            ethers::abi::Token::Uint(U256::from(deadline)),
        ]));

        let contract_struct_hash: H256 = contract
            .method(
                "getStructHash",
                (
                    address,
                    address,
                    U256::from(amount),
                    on_chain_nonce,
                    U256::from(deadline),
                ),
            )?
            .call()
            .await
            .context("Failed to get struct hash from contract")?;

        debug!("Rust struct hash: {:?}", H256::from(struct_hash));
        debug!("Contract struct hash: {:?}", contract_struct_hash);

        if H256::from(struct_hash) != contract_struct_hash {
            return Err(anyhow::anyhow!("Struct hash mismatch"));
        }

        let digest_input = [domain_separator.as_bytes().to_vec(), struct_hash.to_vec()].concat();
        let digest = ethers::utils::keccak256(&digest_input);

        let message_hash = H256::from(digest);
        let signature = wallet.sign_hash(message_hash).context("Failed to sign permit")?;

        let (v, r, s) = {
            let sig = signature.to_vec();
            let mut v = sig[64] as u8;
            if v < 27 {
                v += 27;
            }
            let r = H256::from_slice(&sig[0..32]);
            let s = H256::from_slice(&sig[32..64]);
            (v, r, s)
        };

        // Debug recovery
        let recovered: Address = contract
            .method("testRecovery", (H256::from(digest), v, r, s))?
            .call()
            .await
            .context("Failed to recover signer")?;

        debug!("Recovered address: {:?}", recovered);
        debug!("Expected address: {:?}", address);

        if recovered != address {
            return Err(anyhow::anyhow!(
                "Signature recovery mismatch. Got {:?}, expected {:?}",
                recovered,
                address
            ));
        }

        // 4. Submit permit (fire-and-forget)
        let permit_data = contract.encode(
            "permit",
            (address, address, U256::from(amount), U256::from(deadline), v, r, s),
        )?;

        let permit_tx = TransactionRequest::new()
            .to(token_address)
            .data(permit_data)
            .gas(permit_gas_limit)
            .gas_price(gas_price)
            .nonce(permit_nonce)
            .from(address);

        let pending_permit = client.send_transaction(permit_tx, None).await;

        match pending_permit {
            Ok(pending) => Ok(TaskResult {
                success: true,
                message: format!(
                    "Permit submitted for {} {} tokens (nonce: {}, deadline: {})",
                    amount_formatted, name, on_chain_nonce, deadline
                ),
                tx_hash: Some(format!("{:?}", pending.tx_hash())),
            }),
            Err(e) => {
                debug!("Permit submit failed, resyncing nonce: {}", e);
                let _ = nonce_manager.resync().await;
                Ok(TaskResult {
                    success: false,
                    message: format!("Failed to submit permit tx: {}", e),
                    tx_hash: None,
                })
            },
        }
    }
}
