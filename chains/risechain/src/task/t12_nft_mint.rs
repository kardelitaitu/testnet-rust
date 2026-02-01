use crate::task::{Task, TaskContext, TaskResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers::abi::Token;
use ethers::prelude::*;
use rand::rngs::OsRng;
use rand::Rng;
use std::sync::Arc;
use tracing::debug;

pub struct NftMintTask;

impl NftMintTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for NftMintTask {
    fn name(&self) -> &str {
        "12_nftMint"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let _provider = &ctx.provider;
        let wallet = &ctx.wallet;
        let address = wallet.address();

        // Read Bytecode and ABI
        let bytecode_path = "chains/risechain/contracts/TestNFT_bytecode.txt";
        let abi_path = "chains/risechain/contracts/TestNFT_abi.txt";
        let mnemonic_path = "core-logic/src/utils/mnemonic.txt";

        let recipient = address;
        let mut rng = OsRng;
        let token_id: u64 = rng.gen_range(1000000..9999999);

        let bytecode_hex = std::fs::read_to_string(bytecode_path)
            .with_context(|| format!("Failed to read bytecode from {}", bytecode_path))?;
        let abi_json = std::fs::read_to_string(abi_path)
            .with_context(|| format!("Failed to read ABI from {}", abi_path))?;

        // Read mnemonic file and pick random words
        let mnemonic_content = std::fs::read_to_string(mnemonic_path)
            .with_context(|| format!("Failed to read mnemonic file from {}", mnemonic_path))?;
        let words: Vec<&str> = mnemonic_content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect();

        let word = words[rng.gen_range(0..words.len())];

        // Capitalize first letter
        let mut chars = word.chars();
        let capitalized_word = match chars.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        };

        let nft_name = format!("{} NFT", capitalized_word); // e.g., "Talent NFT"
        let nft_symbol = format!("{}NFT", capitalized_word.chars().next().unwrap_or('T')); // e.g. "TNFT"

        debug!("🎲 Random NFT Name: '{}' ({})", nft_name, nft_symbol);

        // Deploy Contract manually
        let bytecode_raw = ethers::utils::hex::decode(bytecode_hex.trim())?;
        let abi: abi::Abi = serde_json::from_str(&abi_json)?;

        // Encode constructor arguments
        let constructor = abi.constructor().context("ABI missing constructor")?;
        let encoded_args = constructor.encode_input(
            bytecode_raw.clone(),
            &[Token::String(nft_name.clone()), Token::String(nft_symbol)],
        )?;

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;

        let tx = Eip1559TransactionRequest::new()
            .from(address)
            .data(Bytes::from(encoded_args))
            .gas(crate::utils::gas::GasManager::LIMIT_DEPLOY)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee);

        // Use wallet for deployment
        use ethers::middleware::SignerMiddleware;
        let client = Arc::new(SignerMiddleware::new(
            ctx.provider.clone(),
            ctx.wallet.clone(),
        ));
        let pending_tx = client.send_transaction(tx, None).await?;

        let receipt = pending_tx
            .await?
            .context("Failed to get deployment receipt")?;

        let nft_address = receipt
            .contract_address
            .context("Deployment receipt has no contract address")?;

        debug!("✅ Deployed TestNFT at {:?}", nft_address);

        let contract = Contract::new(nft_address, abi, client.clone());

        // Generate Random Color and SVG
        let r: u8 = rng.gen();
        let g: u8 = rng.gen();
        let b: u8 = rng.gen();
        let color_hex = format!("#{:02X}{:02X}{:02X}", r, g, b);
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="500" height="500" viewBox="0 0 500 500"><rect width="500" height="500" fill="{}"/></svg>"#,
            color_hex
        );
        debug!("🖼️ Generated SVG: {}", svg);

        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        let svg_base64 = BASE64.encode(&svg);
        // Pure image Data URI without charset
        let image_uri = format!("data:image/svg+xml;base64,{}", svg_base64);

        // Construct Metadata JSON with standard fields
        let metadata_json = serde_json::json!({
            "name": nft_name,
            "description": format!("NFT of {}", capitalized_word),
            "image": image_uri,
            "external_url": "https://testnet.riselabs.xyz",
            "attributes": [
                {
                    "trait_type": "Color",
                    "value": color_hex
                },
                {
                    "trait_type": "Word",
                    "value": capitalized_word
                }
            ]
        });

        // Encode Token URI
        let json_str = serde_json::to_string(&metadata_json)?;
        debug!("📄 Raw JSON: {}", json_str);

        // Standard data URI for JSON
        let token_uri = format!("data:application/json;base64,{}", BASE64.encode(&json_str));

        debug!("🎨 Generated Metadata: {} (Color: {})", nft_name, color_hex);
        debug!("🔗 Full TokenURI: {}", token_uri);

        let mint_data = contract.encode("mint", (recipient, token_uri.clone()))?;

        let tx = Eip1559TransactionRequest::new()
            .to(nft_address)
            .data(mint_data)
            .gas(U256::from(600_000))
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(address);

        let pending_tx = client.send_transaction(tx, None).await?;
        let receipt = pending_tx.await?.context("Failed to get mint receipt")?;

        // Find Transfer event to get actual Token ID
        // Event signature: Transfer(address indexed from, address indexed to, uint256 indexed tokenId)
        let transfer_event_sig =
            ethers::utils::keccak256("Transfer(address,address,uint256)".as_bytes());

        let mut actual_token_id = U256::zero();
        let mut found_event = false;

        for log in &receipt.logs {
            if log.topics.len() == 4 && log.topics[0] == H256::from(transfer_event_sig) {
                // Topic 1: from (address), Topic 2: to (address), Topic 3: tokenId (uint256)
                actual_token_id = U256::from_big_endian(log.topics[3].as_bytes());
                found_event = true;
                break;
            }
        }

        if !found_event {
            debug!("⚠️ Warning: Could not find Transfer event in receipt.");
        } else {
            debug!("🔍 Found Transfer Event: Token ID {}", actual_token_id);

            // Verify Owner on-chain
            let owner: Address = contract
                .method("ownerOf", actual_token_id)?
                .call()
                .await
                .context("Failed to call ownerOf")?;

            if owner == recipient {
                debug!(
                    "✅ Verified on-chain: Owner of #{} is {:?}",
                    actual_token_id, owner
                );

                // Deep verify TokenURI from contract
                let retrieved_uri: String = contract
                    .method("tokenURI", actual_token_id)?
                    .call()
                    .await
                    .context("Failed to call tokenURI")?;

                if retrieved_uri == token_uri {
                    debug!("✅ Verified on-chain: tokenURI matches perfectly");
                } else {
                    debug!("❌ Mismatch: Contract returned different URI length (Retrieved: {}, Expected: {})", retrieved_uri.len(), token_uri.len());
                    if retrieved_uri.is_empty() {
                        debug!("⚠️  CRITICAL: Contract returned EMPTY tokenURI!");
                    }
                }
            } else {
                debug!(
                    "❌ Mismatch: Owner of #{} is {:?}, expected {:?}",
                    actual_token_id, owner, recipient
                );
            }
        }

        Ok(TaskResult {
            success: receipt.status == Some(U64::from(1)),
            message: format!(
                "Deployed {:?} & Minted #{} (URI ID: {})",
                nft_address, actual_token_id, token_id
            ),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}
