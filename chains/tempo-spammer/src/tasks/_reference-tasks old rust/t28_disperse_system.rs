use crate::tasks::{TempoTask, TaskContext, TaskResult};
use crate::utils::multicall::BatchHelper;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use ethers::prelude::*;
use std::str::FromStr;
use std::sync::Arc;
use rand::seq::SliceRandom;
use rand::Rng; // Added Rng trait
use std::fs;

// Bindings
ethers::contract::abigen!(
    IERC20Disperse,
    r#"[
        function approve(address spender, uint256 amount) returns (bool)
        function allowance(address owner, address spender) view returns (uint256)
        function balanceOf(address owner) view returns (uint256)
        function transferFrom(address from, address to, uint256 amount) returns (bool)
        function symbol() view returns (string)
        function decimals() view returns (uint8)
    ]"#
);

pub struct DisperseSystemTask;

impl DisperseSystemTask {
    fn get_recipients(&self, min: usize, max: usize) -> Vec<Address> {
        let path = "chains/tempo/address.txt";
        let content = fs::read_to_string(path).unwrap_or_default();
        let addresses: Vec<Address> = content
            .lines()
            .filter_map(|line| Address::from_str(line.trim()).ok())
            .collect();

        if addresses.is_empty() {
            println!("[WARN] No addresses found in {}, generating random ones.", path);
            return (0..max).map(|_| LocalWallet::new(&mut rand::thread_rng()).address()).collect();
        }

        let mut rng = rand::thread_rng();
        let count = rng.gen_range(min..=max);
        addresses.choose_multiple(&mut rng, count).cloned().collect()
    }
}

#[async_trait]
impl TempoTask for DisperseSystemTask {
    fn name(&self) -> &str {
        "28_disperse_system"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        println!("\n=== [DEBUG] Starting DisperseSystemTask ===");
        let wallet_addr = ctx.wallet.address();
        let client = SignerMiddleware::new(ctx.provider.clone(), ctx.wallet.clone());
        let client = Arc::new(client);
        
        // 1. Select System Token with Balance
        let tokens = vec![
            (Address::from_str("0x20C0000000000000000000000000000000000000")?, "PathUSD"),
            (Address::from_str("0x20C0000000000000000000000000000000000001")?, "AlphaUSD"),
        ];

        let mut selected_token = None;
        let mut balance = U256::zero();
        let mut decimals = 18;

        for (addr, name) in &tokens {
            let token = IERC20Disperse::new(*addr, client.clone());
            if let Ok(bal) = token.balance_of(wallet_addr).call().await {
                if bal > U256::from(100000) {
                    println!("[DEBUG] Found {} Balance: {}", name, bal);
                    let dec = token.decimals().call().await.unwrap_or(18);
                    selected_token = Some((*addr, name.to_string(), token));
                    balance = bal;
                    decimals = dec;
                    break;
                }
            }
        }

        if selected_token.is_none() {
            return Err(anyhow!("No funded system token found"));
        }
        let (token_addr, token_name, token_contract) = selected_token.unwrap();

        // 2. Get Recipients
        let recipients = self.get_recipients(6, 15);
        println!("[DEBUG] Selected {} recipients", recipients.len());

        // 3. Prepare Amounts
        let mut total_amount = U256::zero();
        let mut amounts = Vec::new();
        // Removed `let mut rng` here to avoid holding across await (if any)
        // Although here we don't await inside the loop, but it's safer.

        for _ in &recipients {
            // Random amount between 100 and 500
            let amt_float: f64 = rand::thread_rng().gen_range(100.0..500.0);
            let amt = ethers::utils::parse_units(amt_float, decimals as u32)?.into();
            amounts.push(amt);
            total_amount += amt;
        }

        if balance < total_amount {
            println!("[WARN] Balance low. Capping amounts.");
            total_amount = balance;
            let share = balance / U256::from(recipients.len());
            amounts = vec![share; recipients.len()];
        }

        // 4. Approve Multicall
        let multicall_addr = Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11")?;
        let allowance = token_contract.allowance(wallet_addr, multicall_addr).call().await?;
        
        if allowance < total_amount {
            println!("[DEBUG] Approving Multicall...");
            let _ = token_contract.approve(multicall_addr, U256::MAX).send().await?.await?;
        }

        // 5. Batch Execute
        let batch_helper = BatchHelper::new(client.clone());
        let mut calls = Vec::new();
        
        for (recipient, amount) in recipients.iter().zip(amounts.iter()) {
            let data = token_contract.transfer_from(wallet_addr, *recipient, *amount).calldata().unwrap();
            calls.push((token_addr, data));
        }

        println!("[DEBUG] Executing Disperse via Multicall...");
        let receipt = batch_helper.execute_batch(calls).await?
            .ok_or(anyhow!("Batch execution failed or returned no receipt"))?;

        Ok(TaskResult {
            success: true,
            message: format!("Dispersed {} {} to {} recipients", total_amount, token_name, recipients.len()),
            tx_hash: Some(format!("{:?}", receipt.transaction_hash)),
        })
    }
}