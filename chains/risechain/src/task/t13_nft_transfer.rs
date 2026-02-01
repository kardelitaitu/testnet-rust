use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use rand::Rng;
use std::fs;
use std::sync::Arc;
use tracing::debug;

pub struct NftTransferTask;

impl NftTransferTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for NftTransferTask {
    fn name(&self) -> &str {
        "13_nftTransfer"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // 1. Setup Data Paths and Recipient
        let bytecode_path = "chains/risechain/contracts/TestNFT_bytecode.txt";
        let abi_path = "chains/risechain/contracts/TestNFT_abi.txt";
        let mnemonic_path = "core-logic/src/utils/mnemonic.txt";

        let recipients = fs::read_to_string("address.txt").context("Failed to read address.txt")?;
        let recipient_list: Vec<&str> = recipients
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect();

        let recipient_str = recipient_list
            .choose(&mut OsRng)
            .context("address.txt is empty")?;

        let recipient: Address = recipient_str
            .trim()
            .parse()
            .context(format!("Invalid address in address.txt: {}", recipient_str))?;

        let mut rng = OsRng;

        // 2. Prepare NFT Details (Random Name/Symbol)
        let bytecode_hex = std::fs::read_to_string(bytecode_path)
            .with_context(|| format!("Failed to read bytecode from {}", bytecode_path))?;
        let abi_json = std::fs::read_to_string(abi_path)
            .with_context(|| format!("Failed to read ABI from {}", abi_path))?;
        let abi: abi::Abi = serde_json::from_str(&abi_json)?;

        let mnemonic_content = std::fs::read_to_string(mnemonic_path)
            .with_context(|| format!("Failed to read mnemonic file from {}", mnemonic_path))?;
        let words: Vec<&str> = mnemonic_content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect();
        let word = words[rng.gen_range(0..words.len())];

        let mut chars = word.chars();
        let capitalized_word = match chars.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        };

        let nft_name = format!("{} Transfer NFT", capitalized_word);
        let nft_symbol = format!("{}TNFT", capitalized_word.chars().next().unwrap_or('T'));

        // 3. Deploy Contract Manually
        let bytecode_raw = ethers::utils::hex::decode(bytecode_hex.trim())?;
        let constructor = abi.constructor().context("ABI missing constructor")?;
        let encoded_args = constructor.encode_input(
            bytecode_raw,
            &[
                ethers::abi::Token::String(nft_name),
                ethers::abi::Token::String(nft_symbol),
            ],
        )?;

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.clone()));

        let deploy_tx = Eip1559TransactionRequest::new()
            .from(address)
            .data(Bytes::from(encoded_args))
            .gas(crate::utils::gas::GasManager::LIMIT_DEPLOY)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee);

        let receipt = client
            .send_transaction(deploy_tx, None)
            .await?
            .await?
            .context("Failed to get deployment receipt")?;
        let nft_address = receipt
            .contract_address
            .context("No contract address in receipt")?;
        debug!("✅ Deployed NFT at {:?}", nft_address);

        let contract = Contract::new(nft_address, abi, client.clone());

        // 4. Mint Token to Sender
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        let metadata_json = format!(
            r#"{{"name":"{}","description":"Transfer Test"}}"#,
            capitalized_word
        );
        let metadata_uri = format!(
            "data:application/json;base64,{}",
            BASE64.encode(metadata_json)
        );

        let mint_data = contract.encode("mint", (address, metadata_uri))?;
        let mint_tx = Eip1559TransactionRequest::new()
            .to(nft_address)
            .data(mint_data)
            .gas(600_000)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let mint_receipt = client
            .send_transaction(mint_tx, None)
            .await?
            .await?
            .context("Failed to get mint receipt")?;

        let transfer_event_sig =
            ethers::utils::keccak256("Transfer(address,address,uint256)".as_bytes());
        let mut token_id = U256::zero();
        for log in &mint_receipt.logs {
            if log.topics.len() == 4 && log.topics[0] == H256::from(transfer_event_sig) {
                token_id = U256::from_big_endian(log.topics[3].as_bytes());
                break;
            }
        }
        debug!("✅ Minted Token #{}", token_id);

        // 5. Transfer Token to Recipient
        let transfer_data = contract.encode("transferFrom", (address, recipient, token_id))?;
        let transfer_tx = Eip1559TransactionRequest::new()
            .to(nft_address)
            .data(transfer_data)
            .gas(crate::utils::gas::GasManager::LIMIT_SEND_MEME)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let transfer_receipt = client
            .send_transaction(transfer_tx, None)
            .await?
            .await?
            .context("Failed to get transfer receipt")?;
        let success = transfer_receipt.status == Some(U64::from(1));

        // 6. Verify On-Chain
        if success {
            let owner: Address = contract.method("ownerOf", token_id)?.call().await?;
            if owner == recipient {
                debug!(
                    "✅ Verified on-chain: New owner of #{} is {:?}",
                    token_id, owner
                );
            } else {
                debug!(
                    "❌ Mismatch: New owner of #{} is {:?}, expected {:?}",
                    token_id, owner, recipient
                );
            }
        }

        Ok(TaskResult {
            success,
            message: format!(
                "Transferred NFT #{} from {:?} to {:?}",
                token_id, nft_address, recipient
            ),
            tx_hash: Some(format!("{:?}", transfer_receipt.transaction_hash)),
        })
    }
}
