use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use std::sync::Arc;
use tracing::debug;

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
        let amount_formatted =
            ethers::utils::format_units(amount, 18u32).unwrap_or_else(|_| amount.to_string());

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.clone()));

        // Deploy TestERC20Permit
        let bytecode_str = include_str!("../../contracts/TestERC20Permit_bytecode.txt").trim();
        let bytecode = hex::decode(bytecode_str).context("Failed to decode bytecode")?;
        let abi_str = include_str!("../../contracts/TestERC20Permit_abi.txt").trim();
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
        let token_address = deploy_receipt
            .contract_address
            .context("No contract address")?;
        let contract = Contract::new(token_address, abi, client.clone());
        debug!("Deployed TestERC20Permit at {:?}", token_address);

        let name: String = contract
            .method("name", ())?
            .call()
            .await
            .context("Failed to get name")?;
        let token_nonce: U256 = contract
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

        let struct_hash = ethers::utils::keccak256(&ethers::abi::encode(&[
            ethers::abi::Token::FixedBytes(permit_typehash.as_bytes().to_vec()),
            ethers::abi::Token::Address(address),
            ethers::abi::Token::Address(address),
            ethers::abi::Token::Uint(amount.into()),
            ethers::abi::Token::Uint(token_nonce),
            ethers::abi::Token::Uint(U256::from(deadline)),
        ]));

        let contract_struct_hash: H256 = contract
            .method(
                "getStructHash",
                (
                    address,
                    address,
                    U256::from(amount),
                    token_nonce,
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
        let signature = wallet
            .sign_hash(message_hash)
            .context("Failed to sign permit")?;

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

        // Note: Contract interface already wrapped by `contract` variable using `abi`.
        // We can call `permit` directly.
        // We can call `permit` directly.
        let permit_data = contract.encode(
            "permit",
            (
                address,
                address,
                U256::from(amount),
                U256::from(deadline),
                v,
                r,
                s,
            ),
        )?;
        let permit_tx = Eip1559TransactionRequest::new()
            .to(token_address)
            .data(permit_data)
            .gas(500_000)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let pending_tx = client.send_transaction(permit_tx, None).await?;
        let receipt = pending_tx
            .await?
            .context("Failed to get transaction receipt")?;

        Ok(TaskResult {
            success: receipt.status == Some(U64::from(1)),
            message: format!(
                "Permit sent for {} {} tokens (nonce: {}, deadline: {})",
                amount_formatted, name, token_nonce, deadline
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
