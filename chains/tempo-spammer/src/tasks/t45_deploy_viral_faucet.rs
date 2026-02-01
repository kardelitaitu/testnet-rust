//! Deploy Viral Faucet Task
//!
//! Deploys a new ViralFaucet contract and funds it with a stablecoin.
//! Uses embedded bytecode extracted from build artifacts.

use crate::TempoClient;
use crate::tasks::tempo_tokens::TempoTokens;
use crate::tasks::{TaskContext, TaskResult, TempoTask};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::TransactionRequest;
use alloy::sol;
use alloy::sol_types::SolCall;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::Rng;

// Define the contract interfaces using alloy's sol! macro
sol!(
    #[sol(rpc)]
    contract ViralFaucet {
        function fund(address token, uint256 amount) external;
        function claim(address token, uint256 amount) external;
        function getBalance(address token) external view returns (uint256);
    }
);

sol!(
    interface IERC20Local {
        function approve(address spender, uint256 amount) external returns (bool);
    }
);

// Bytecode from data/ViralFaucet_build.json
const VIRAL_FAUCET_BYTECODE: &str = "608060405234801561000f575f80fd5b506105ed8061001d5f395ff3fe608060405234801561000f575f80fd5b5060043610610055575f3560e01c806331ff6327146100595780637b1837de14610092578063a2724a4d146100a7578063aad3ec96146100af578063f8b2cb4f146100c2575b5f80fd5b6100806100673660046104e9565b5f60208181529281526040808220909352908152205481565b60405190815260200160405180910390f35b6100a56100a036600461051a565b6100d5565b005b610080603c81565b6100a56100bd36600461051a565b61021a565b6100806100d0366004610542565b610460565b5f811161011e5760405162461bcd60e51b81526020600482015260126024820152710416d6f756e74206d757374206265203e20360741b60448201526064015b60405180910390fd5b6040516323b872dd60e01b8152336004820152306024820152604481018290525f906001600160a01b038416906323b872dd906064016020604051808303815f875af1158015610170573d5f803e3d5ffd5b505050506040513d601f19601f820116820180604052508101906101949190610562565b9050806101d55760405162461bcd60e51b815260206004820152600f60248201526e151c985b9cd9995c8819985a5b1959608a1b6044820152606401610115565b6040518281526001600160a01b0384169033907f3b5083eec1a1116c56de5d6841cff8efc6a0aec9850e836ec509d6ce024ea5619060200160405180910390a3505050565b335f908152602081815260408083206001600160a01b038616845290915290205461024790603c90610581565b4210156102965760405162461bcd60e51b815260206004820152601e60248201527f436f6f6c646f776e2061637469766520666f72207468697320746f6b656e00006044820152606401610115565b6040516370a0823160e01b81523060048201525f906001600160a01b038416906370a0823190602401602060405180830381865afa1580156102da573d5f803e3d5ffd5b505050506040513d601f19601f820116820180604052508101906102fe91906105a0565b9050818110156103505760405162461bcd60e51b815260206004820152601b60248201527f46617563657420656d70747920666f72207468697320746f6b656e00000000006044820152606401610115565b335f818152602081815260408083206001600160a01b03881680855292528083204290555163a9059cbb60e01b8152600481019390935260248301859052909163a9059cbb906044016020604051808303815f875af11580156103b5573d5f803e3d5ffd5b505050506040513d601f19601f820116820180604052508101906103d99190610562565b90508061041a5760405162461bcd60e51b815260206004820152600f60248201526e151c985b9cd9995c8819985a5b1959608a1b6044820152606401610115565b6040518381526001600160a01b0385169033907ff7a40077ff7a04c7e61f6f26fb13774259ddf1b6bce9ecf26a8276cdd39926839060200160405180910390a350505050565b6040516370a0823160e01b81523060048201525f906001600160a01b038316906370a0823190602401602060405180830381865afa1580156104a4573d5f803e3d5ffd5b505050506040513d601f19601f820116820180604052508101906104c891906105a0565b92915050565b80356001600160a01b03811681146104e4575f80fd5b919050565b5f80604083850312156104fa575f80fd5b610503836104ce565b9150610511602084016104ce565b90509250929050565b5f806040838503121561052b575f80fd5b610534836104ce565b946020939093013593505050565b5f60208284031215610552575f80fd5b61055b826104ce565b9392505050565b5f60208284031215610572575f80fd5b8151801515811461055b575f80fd5b808201808211156104c857634e487b7160e01b5f52601160045260245ffd5b5f602082840312156105b0575f80fd5b505191905056fea264697066735822122074478da855b01add75c2b8665e400b0464fe3020266a2b16ab7bfa3a789ff81264736f6c63430008140033";

#[derive(Debug, Clone, Default)]
pub struct DeployViralFaucetTask;

impl DeployViralFaucetTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TempoTask for DeployViralFaucetTask {
    fn name(&self) -> &'static str {
        "45_deploy_viral_faucet"
    }

    async fn run(&self, ctx: &TaskContext) -> Result<TaskResult> {
        let client = &ctx.client;
        let address = ctx.address();

        // 1. Find a Token to Fund
        tracing::debug!("Scanning for stablecoins to fund faucet...");
        let mut funded_token = None;
        let mut funded_balance = U256::ZERO;
        let mut funded_decimals = 18;

        // Shuffle tokens to random selection
        let mut tokens = TempoTokens::get_system_tokens();
        use rand::seq::SliceRandom;
        let mut rng = rand::rngs::OsRng;
        tokens.shuffle(&mut rng);

        for token in tokens {
            let decimals = TempoTokens::get_token_decimals(client, token.address)
                .await
                .unwrap_or(18);
            let balance = TempoTokens::get_token_balance(client, token.address, address).await?;

            // Check for > 50 units (50 * 10^decimals)
            let min_bal = U256::from(50) * U256::from(10_u64.pow(decimals as u32));

            if balance >= min_bal {
                funded_token = Some(token);
                funded_balance = balance;
                funded_decimals = decimals;
                break;
            }
        }

        if funded_token.is_none() {
            return Ok(TaskResult {
                success: false,
                message: "No stablecoin with > 50 balance found to fund faucet.".to_string(),
                tx_hash: None,
            });
        }

        let token = funded_token.unwrap();
        // println!("Selected {} for funding (Bal: {})", token.symbol, TempoTokens::format_amount(funded_balance, funded_decimals));

        // 2. Deploy ViralFaucet
        let bytecode_bytes = hex::decode(VIRAL_FAUCET_BYTECODE).context("Invalid hex bytecode")?;
        let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let mut deploy_tx = TransactionRequest::default()
            .input(bytecode_bytes.into())
            .from(address)
            .nonce(nonce)
            .max_fee_per_gas(150_000_000_000u128)
            .max_priority_fee_per_gas(1_500_000_000u128)
            .gas_limit(5_000_000);
        deploy_tx.to = Some(alloy::primitives::TxKind::Create);

        let pending_deploy = client
            .provider
            .send_transaction(deploy_tx)
            .await
            .context("Failed to send deploy tx")?;
        let deploy_hash = *pending_deploy.tx_hash();
        let deploy_receipt = pending_deploy
            .get_receipt()
            .await
            .context("Failed to get deploy receipt")?;

        let contract_addr = if let Some(addr) = deploy_receipt.contract_address {
            addr
        } else {
            return Ok(TaskResult {
                success: false,
                message: format!(
                    "Deployment failed (no contract address). Tx: {:?}",
                    deploy_hash
                ),
                tx_hash: Some(format!("{:?}", deploy_hash)),
            });
        };

        // println!("ViralFaucet deployed at {:?}", contract_addr);

        // 3. Calculate Fund Amount (20-50% of balance, capped at 100)
        let pct = rng.gen_range(20..=50);
        let amount_base = funded_balance * U256::from(pct) / U256::from(100);
        let max_cap = U256::from(100) * U256::from(10_u64.pow(funded_decimals as u32));
        let min_cap = U256::from(10) * U256::from(10_u64.pow(funded_decimals as u32));

        let mut fund_amount = amount_base;
        if fund_amount > max_cap {
            fund_amount = max_cap;
        }
        if fund_amount < min_cap {
            fund_amount = min_cap;
        }

        // println!("Funding with {} {}...", TempoTokens::format_amount(fund_amount, funded_decimals), token.symbol);

        // 4. Approve Faucet (2x for safety buffer)
        let approve_amount = fund_amount * U256::from(2);
        let approve_call = IERC20Local::approveCall {
            spender: contract_addr,
            amount: approve_amount,
        };
        let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let approve_tx = TransactionRequest::default()
            .to(token.address)
            .input(approve_call.abi_encode().into())
            .from(address)
            .nonce(nonce);

        let pending_app = client.provider.send_transaction(approve_tx).await?;
        let approve_receipt = pending_app.get_receipt().await?;

        // Ensure approval propagated
        if !approve_receipt.inner.status() {
            anyhow::bail!("Approval transaction failed");
        }

        // 5. Fund Faucet
        let fund_call = ViralFaucet::fundCall {
            token: token.address,
            amount: fund_amount,
        };
        let nonce = client.get_pending_nonce(&ctx.config.rpc_url).await?;
        let fund_tx = TransactionRequest::default()
            .to(contract_addr)
            .input(fund_call.abi_encode().into())
            .from(address)
            .nonce(nonce);

        let pending_fund = client.provider.send_transaction(fund_tx).await?;
        let _ = pending_fund.get_receipt().await?;

        // 6. Log to DB
        if let Some(db) = &ctx.db {
            db.log_asset_creation(
                &format!("{:?}", address),
                &format!("{:?}", contract_addr),
                "viral_faucet",
                "Viral Faucet",
                "VIRAL",
            )
            .await?;
        }

        Ok(TaskResult {
            success: true,
            message: format!(
                "Deployed & Funded ViralFaucet with {} {}",
                TempoTokens::format_amount(fund_amount, funded_decimals),
                token.symbol
            ),
            tx_hash: Some(format!("{:?}", deploy_hash)),
        })
    }
}
