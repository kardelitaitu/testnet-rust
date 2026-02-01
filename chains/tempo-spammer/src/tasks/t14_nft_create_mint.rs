//! NFT Create & Mint Task
//!
//! Deploys a new NFT collection and mints the first token.
//! Uses compiled MinimalNFT.sol from contracts folder.
//!
//! Workflow:
//! 1. Deploy ERC721 contract
//! 2. Grant Minter Role to the deployer
//! 3. Mint token #1 to wallet
//! 4. Log to database

use crate::TempoClient;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, TxKind, U256};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy::sol;
use alloy_sol_types::{SolCall, SolEvent};
use anyhow::{Context, Result};
use async_trait::async_trait;

// Generated from solc --bin --abi src/contracts/MinimalNFT.sol
const NFT_BYTECODE_HEX: &str = "60806040526040518060400160405280600881526020017f54454d504f4e46540000000000000000000000000000000000000000000000008152505f9081610047919061032c565b506040518060400160405280600381526020017f544d5000000000000000000000000000000000000000000000000000000000008152506001908161008c919061032c565b50348015610098575f5ffd5b503360065f6101000a81548173ffffffffffffffffffffffffffffffffffffffff021916908373ffffffffffffffffffffffffffffffffffffffff1602179055506103fb565b5f81519050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52604160045260245ffd5b7f4e487b71000000000000000000000000000000000000000000000000000000005f52602260045260245ffd5b5f600282049050600182168061015957607f821691505b60208210810361016c5761016b610115565b5b50919050565b5f819050815f5260205f209050919050565b5f6020601f8301049050919050565b5f82821b905092915050565b5f600883026101ce7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff82610193565b6101d88683610193565b95508019841693508086168417925050509392505050565b5f819050919050565b5f819050919050565b5f61021c610217610212846101f0565b6101f9565b6101f0565b9050919050565b5f819050919050565b61023583610202565b61024961024182610223565b84845461019f565b825550505050565b5f5f905090565b610260610251565b61026b81848461022c565b505050565b5f5b82811015610291576102865f828401610258565b600181019050610272565b505050565b601f8211156102e457828211156102e3576102b081610172565b6102b983610184565b6102c285610184565b60208610156102cf575f90505b8083016102de82840382610270565b505050505b5b505050565b5f82821c905092915050565b5f6103045f19846008026102e9565b1980831691505092915050565b5f61031c83836102f5565b9150826002028217905092915050565b610335826100de565b67ffffffffffffffff81111561034e5761034d6100e8565b5b6103588254610142565b610363828285610296565b5f60209050601f831160018114610394575f8415610382578287015190505b61038c8582610311565b8655506103f3565b601f1984166103a286610172565b5f5b828110156103c9578489015182556001820191506020850194506020810190506103a4565b868310156103e657848901516103e2601f8916826102f5565b8355505b6001600288020188555050505b505050505050565b610978806104085f395ff3fe608060405234801561000f575f5ffd5b5060043610610091575f3560e01c80636a627842116100645780636a6278421461012f57806375794a3c1461014b57806395d89b4114610169578063aa271e1a14610187578063f851a440146101b757610091565b8063025e7c271461009557806306fdde03146100c557806327e235e3146100e35780633897768614610113575b5f5ffd5b6100af60048036038101906100aa91906105de565b6101d5565b6040516100bc9190610648565b60405180910390f35b6100cd610205565b6040516100da91906106d1565b60405180910390f35b6100fd60048036038101906100f8919061071b565b610290565b60405161010a9190610755565b60405180910390f35b61012d6004803603810190610128919061071b565b6102a5565b005b6101496004803603810190610144919061071b565b61038c565b005b6101536104d3565b6040516101609190610755565b60405180910390f35b6101716104d9565b60405161017e91906106d1565b60405180910390f35b6101a1600480360381019061019c919061071b565b610565565b6040516101ae9190610788565b60405180910390f35b6101bf610582565b6040516101cc9190610648565b60405180910390f35b6003602052805f5260405f205f915054906101000a900473ffffffffffffffffffffffffffffffffffffffff1681565b5f8054610211906107ce565b80601f016020809104026020016040519081016040528092919081815260200182805461023d906107ce565b80156102885780601f1061025f57610100808354040283529160200191610288565b820191905f5260205f20905b81548152906001019060200180831161026b57829003601f168201915b505050505081565b6004602052805f5260405f205f915090505481565b60065f9054906101000a900473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff163373ffffffffffffffffffffffffffffffffffffffff1614610334576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040161032b90610848565b60405180910390fd5b600160055f8373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020015f205f6101000a81548160ff02191690831515021790555050565b60055f3373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020015f205f9054906101000a900460ff16610415576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040161040c906108b0565b60405180910390fd5b5f60025f815480929190610428906108fb565b9190505590508160035f8381526020019081526020015f205f6101000a81548173ffffffffffffffffffffffffffffffffffffffff021916908373ffffffffffffffffffffffffffffffffffffffff16021790555060045f8373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020015f205f8154809291906104ca906108fb565b91905055505050565b60025481565b600180546104e6906107ce565b80601f0160208091040260200160405190810160405280929190818152602001828054610512906107ce565b801561055d5780601f106105345761010080835404028352916020019161055d565b820191905f5260205f20905b81548152906001019060200180831161054057829003601f168201915b505050505081565b6005602052805f5260405f205f915054906101000a900460ff1681565b60065f9054906101000a900473ffffffffffffffffffffffffffffffffffffffff1681565b5f5ffd5b5f819050919050565b6105bd816105ab565b81146105c7575f5ffd5b50565b5f813590506105d8816105b4565b92915050565b5f602082840312156105f3576105f26105a7565b5b5f610600848285016105ca565b91505092915050565b5f73ffffffffffffffffffffffffffffffffffffffff82169050919050565b5f61063282610609565b9050919050565b61064281610628565b82525050565b5f60208201905061065b5f830184610639565b92915050565b5f81519050919050565b5f82825260208201905092915050565b8281835e5f83830152505050565b5f601f19601f8301169050919050565b5f6106a382610661565b6106ad818561066b565b93506106bd81856020860161067b565b6106c681610689565b840191505092915050565b5f6020820190508181035f8301526106e98184610699565b905092915050565b6106fa81610628565b8114610704575f5ffd5b50565b5f81359050610715816106f1565b92915050565b5f602082840312156107305761072f6105a7565b5b5f61073d84828501610707565b91505092915050565b61074f816105ab565b82525050565b5f6020820190506107685f830184610746565b92915050565b5f8115159050919050565b6107828161076e565b82525050565b5f60208201905061079b5f830184610779565b92915050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52602260045260245ffd5b5f60028204905060018216806107e557607f821691505b6020821081036107f8576107f76107a1565b5b50919050565b7f4e6f742061646d696e00000000000000000000000000000000000000000000005f82015250565b5f61083260098361066b565b915061083d826107fe565b602082019050919050565b5f6020820190508181035f83015261085f81610826565b9050919050565b7f4e6f74206d696e746572000000000000000000000000000000000000000000005f82015250565b5f61089a600a8361066b565b91506108a582610866565b602082019050919050565b5f6020820190508181035f8301526108c78161088e565b9050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52601160045260245ffd5b5f610905826105ab565b91507fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff8203610937576109366108ce565b5b60018201905091905056fea26469706673582212205c07cd1fb818f7bd79f097c1d85604d43fe738c8997f62e71b4e2e9b139aec6a64736f6c63430008210033";

sol! {
    interface IMinimalNFT {
        function grantRole(address minter) external;
        function mint(address to) external;
        function name() external view returns (string);
        function symbol() external view returns (string);
        event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);
    }
}

#[derive(Debug, Clone, Default)]
pub struct NftCreateMintTask;

impl NftCreateMintTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for NftCreateMintTask {
    fn name(&self) -> &'static str {
        "14_nft_create_mint"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        // println!("Deploying NFT Collection...");

        let bytecode = hex::decode(NFT_BYTECODE_HEX).context("Invalid bytecode hex")?;

        let mut deploy_tx = TransactionRequest::default()
            .input(TransactionInput::from(bytecode))
            .from(address)
            .max_fee_per_gas(150_000_000_000u128)
            .max_priority_fee_per_gas(1_500_000_000u128);

        deploy_tx.to = Some(TxKind::Create);

        // Send deploy with retry logic
        let pending = match client.provider.send_transaction(deploy_tx.clone()).await {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on NFT deploy, resetting cache and retrying...");
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client
                        .provider
                        .send_transaction(deploy_tx)
                        .await
                        .context("Failed to deploy NFT")?
                } else {
                    return Err(e).context("Failed to deploy NFT");
                }
            }
        };

        let tx_hash = *pending.tx_hash();
        let receipt = pending
            .get_receipt()
            .await
            .context("Failed to get receipt")?;

        if !receipt.inner.status() {
            return Ok(TaskResult {
                success: false,
                message: format!("NFT deployment transaction failed. Tx: {:?}", tx_hash),
                tx_hash: Some(format!("{:?}", tx_hash)),
            });
        }

        let contract_address = receipt
            .contract_address
            .ok_or(anyhow::anyhow!("No contract address in receipt"))?;

        // println!(
        //     "✅ Contract deployed at {:?}. Tx: {:?}",
        //     contract_address, tx_hash
        // );

        // Wait for node to index contract
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Fetch Name and Symbol
        // We use query calls (eth_call)
        let name_call = IMinimalNFT::nameCall {};
        let symbol_call = IMinimalNFT::symbolCall {};

        let name_result = client
            .provider
            .call(
                TransactionRequest::default()
                    .to(contract_address)
                    .input(TransactionInput::from(name_call.abi_encode())),
            )
            .await;

        let symbol_result = client
            .provider
            .call(
                TransactionRequest::default()
                    .to(contract_address)
                    .input(TransactionInput::from(symbol_call.abi_encode())),
            )
            .await;

        if let (Ok(name_bytes), Ok(symbol_bytes)) = (name_result, symbol_result) {
            // Removed boolean validation argument
            let decoded_name = IMinimalNFT::nameCall::abi_decode_returns(&name_bytes);
            let decoded_symbol = IMinimalNFT::symbolCall::abi_decode_returns(&symbol_bytes);

            // Correctly accessing the tuple field (or struct field if named)
            // The generated return type for `name` is `nameReturn {_0: String}`
            // Wait, the previous error `no field _0 on type String` implies `decoded_name` IS `String`??
            // If `abi_decode_returns` returns `Result<Return>`, and the return type is just a string?
            // Let's print using debug formatter to be safe and avoid assuming fields if we are unsure.
            if let (Ok(n), Ok(s)) = (decoded_name, decoded_symbol) {
                // println!("ℹ️  Collection: {:?} ({:?})", n, s);
            }
        }

        // Grant Minter Role
        // println!("Granting Minter Role...");
        // Define grant call...
        let grant_call = IMinimalNFT::grantRoleCall { minter: address };
        let grant_input = grant_call.abi_encode();

        let grant_tx = TransactionRequest::default()
            .to(contract_address)
            .input(TransactionInput::from(grant_input.clone()))
            .from(address)
            .gas_limit(200_000)
            .max_fee_per_gas(150_000_000_000u128)
            .max_priority_fee_per_gas(1_500_000_000u128);

        // Send grant with retry logic
        let grant_pending = match client.provider.send_transaction(grant_tx.clone()).await {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on NFT grant, resetting cache and retrying...");
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client
                        .provider
                        .send_transaction(grant_tx)
                        .await
                        .context("Failed to send grant role transaction")?
                } else {
                    return Err(e).context("Failed to send grant role transaction");
                }
            }
        };

        let grant_hash = *grant_pending.tx_hash();
        let grant_receipt = grant_pending
            .get_receipt()
            .await
            .context("Failed to get grant role receipt")?;

        if !grant_receipt.inner.status() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Grant role transaction failed. Deploy: {:?}, Grant: {:?}",
                    tx_hash, grant_hash
                ),
                tx_hash: Some(format!("{:?}", grant_hash)),
            });
        }
        // println!("✅ Minter role granted. Tx: {:?}", grant_hash);

        // Wait for node to index role
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // Mint NFT
        // println!("Minting NFT...");
        let mint_call = IMinimalNFT::mintCall { to: address };
        let mint_input = mint_call.abi_encode();

        let mint_tx = TransactionRequest::default()
            .to(contract_address)
            .input(TransactionInput::from(mint_input.clone()))
            .from(address)
            .gas_limit(5_000_000)
            .max_fee_per_gas(150_000_000_000u128)
            .max_priority_fee_per_gas(1_500_000_000u128);

        // Send mint with retry logic
        let mint_pending = match client.provider.send_transaction(mint_tx.clone()).await {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("nonce too low") || err_str.contains("already known") {
                    tracing::warn!("Nonce error on NFT mint, resetting cache and retrying...");
                    client.reset_nonce_cache().await;
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    client
                        .provider
                        .send_transaction(mint_tx)
                        .await
                        .context("Failed to send mint transaction")?
                } else {
                    return Err(e).context("Failed to send mint transaction");
                }
            }
        };

        let mint_hash = *mint_pending.tx_hash();
        let mint_receipt = mint_pending
            .get_receipt()
            .await
            .context("Failed to get mint receipt")?;

        if !mint_receipt.inner.status() {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "NFT mint transaction failed. Deploy: {:?}, Grant: {:?}, Mint: {:?}",
                    tx_hash, grant_hash, mint_hash
                ),
                tx_hash: Some(format!("{:?}", mint_hash)),
            });
        }

        // Parse logs to get Token ID
        let mut minted_id = U256::ZERO;
        for log in mint_receipt.inner.logs() {
            // Removed boolean validation argument
            if let Ok(decoded_log) =
                IMinimalNFT::Transfer::decode_raw_log(log.topics(), &log.data().data)
            {
                minted_id = decoded_log.tokenId;
                // println!("ℹ️  Minted Token ID: {}", minted_id);
                break;
            }
        }

        // println!(
        //     "✅ Minted NFT at {:?}. Tx: {:?}",
        //     contract_address, mint_hash
        // );

        if let Some(db) = &ctx.db {
            if let Err(e) = db
                .log_asset_creation(
                    &address.to_string(),
                    &contract_address.to_string(),
                    "nft",
                    "NFT",
                    "NFT",
                )
                .await
            {
                // println!("⚠️ Failed to log to database: {:?}", e);
            } else {
                // println!("📝 NFT logged to database");
            }
        }

        Ok(TaskResult {
            success: true,
            message: format!(
                "Deployed NFT contract at {:?} and minted Token ID {}. Tx: {:?}",
                contract_address, minted_id, tx_hash
            ),
            tx_hash: Some(format!("{:?}", tx_hash)),
        })
    }
}
