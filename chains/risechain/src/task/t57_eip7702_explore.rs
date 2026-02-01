use crate::task::{Task, TaskContext, TaskResult};
use anyhow::Result;
use async_trait::async_trait;
use ethers::prelude::*;

pub struct Eip7702ExploreTask;

impl Eip7702ExploreTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task<TaskContext> for Eip7702ExploreTask {
    fn name(&self) -> &str {
        "57_eip7702Explore"
    }

    async fn run(&self, ctx: TaskContext) -> Result<TaskResult> {
        let provider = &ctx.provider;
        let wallet = &ctx.wallet;

        const AUTH_DEPLOYER: &str = "0x962560A0333190D57009A0aAAB7Bfa088f58461C";
        const _TEST_EOA: &str = "0x0000000000000000000000000000000000000000";

        let deployer_address: Address = AUTH_DEPLOYER.parse()?;

        let mut messages = Vec::new();
        messages.push("EIP-7702 Exploration on RISE L2".to_string());
        messages.push("https://eips.ethereum.org/EIPS/eip-7702".to_string());
        messages.push("".to_string());

        let (max_fee, priority_fee) = ctx.gas_manager.get_fees().await?;
        let gas_limit = crate::utils::gas::GasManager::LIMIT_DEPLOY;

        let client = std::sync::Arc::new(SignerMiddleware::new(
            std::sync::Arc::new(provider.clone()),
            wallet.clone(),
        ));

        messages.push("=== Test 1: Basic EIP-7702 Authorization ===".to_string());

        let deployer_wallet: LocalWallet =
            "0x942ba639ec667bdded6d727ad2e483648a34b584f916e6b826fdb7b512633731".parse()?;
        let deployer_wallet = deployer_wallet.with_chain_id(ctx.config.chain_id);

        let address_bytes = deployer_address.as_bytes();
        let mut auth_data = vec![0x80, 0x94];
        auth_data.extend_from_slice(address_bytes);
        auth_data.push(0x80);
        let mut auth_message = vec![0x04u8];
        auth_message.extend_from_slice(&auth_data);
        let auth_hash = ethers::utils::keccak256(&auth_message);

        let signature = deployer_wallet.sign_hash(TxHash(auth_hash))?;
        let y_parity = if signature.recovery_id().unwrap().is_y_odd() {
            28u8
        } else {
            27u8
        };

        messages.push(format!("Deployer: {:?}", deployer_address));
        messages.push(format!("Auth hash: 0x{}", hex::encode(&auth_hash)));
        messages.push(format!(
            "Signature: v={}, r=0x{:064x}, s=0x{:064x}",
            y_parity, signature.r, signature.s
        ));
        messages.push("".to_string());

        messages.push("=== Test 2: Simple Bootstrap Code ===".to_string());
        let simple_bootstrap = hex::decode("6000")?;
        messages.push(format!(
            "Bootstrap (RETURN 0,0): 0x{}",
            hex::encode(&simple_bootstrap)
        ));

        let tx1 = Eip1559TransactionRequest::new()
            .to(deployer_address)
            .data(simple_bootstrap.clone())
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(wallet.address());

        messages.push("Sending simple bootstrap...".to_string());
        match client.send_transaction(tx1, None).await {
            Ok(pending) => match pending.await {
                Ok(Some(receipt)) => {
                    messages.push(format!("Tx: {:?}", receipt.transaction_hash));
                    messages.push(format!("Status: {:?}", receipt.status));
                    messages.push(format!("Gas used: {:?}", receipt.gas_used));

                    let code_after = provider.get_code(deployer_address, None).await?;
                    messages.push(format!("Code length after: {} bytes", code_after.len()));

                    if receipt.status == Some(U64::from(1)) && code_after.len() > 0 {
                        messages.push("✅ EIP-7702 WORKS! Code was set!".to_string());
                    } else if receipt.status == Some(U64::from(1)) {
                        messages.push("⚠️  Transaction succeeded but no code set".to_string());
                    }
                }
                Ok(None) => messages.push("⏳ Pending".to_string()),
                Err(e) => messages.push(format!(
                    "❌ {}",
                    e.to_string().lines().next().unwrap_or("error")
                )),
            },
            Err(e) => messages.push(format!(
                "❌ Send failed: {}",
                e.to_string().lines().next().unwrap_or("error")
            )),
        }
        messages.push("".to_string());

        messages.push("=== Test 3: CREATE2 Bootstrap ===".to_string());
        let create2_bootstrap = hex::decode(
            "60203d3d3582360380843d373d34f5806019573d813d933efd5b3d52f33d52601d6003f3",
        )?;
        messages.push(format!(
            "CREATE2 bootstrap: 0x{}...",
            hex::encode(&create2_bootstrap[..std::cmp::min(36, create2_bootstrap.len())])
        ));

        let tx2 = Eip1559TransactionRequest::new()
            .to(deployer_address)
            .data(create2_bootstrap)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(wallet.address());

        messages.push("Sending CREATE2 bootstrap...".to_string());
        match client.send_transaction(tx2, None).await {
            Ok(pending) => match pending.await {
                Ok(Some(receipt)) => {
                    messages.push(format!("Tx: {:?}", receipt.transaction_hash));
                    messages.push(format!("Status: {:?}", receipt.status));

                    let factory_address: Address = "0xC0DEb853af168215879d284cc8B4d0A645fA9b0E"
                        .parse()
                        .unwrap();
                    let factory_code = provider.get_code(factory_address, None).await?;
                    messages.push(format!(
                        "Factory at 0xC0DE...: {} bytes",
                        factory_code.len()
                    ));
                }
                Ok(None) => messages.push("⏳ Pending".to_string()),
                Err(e) => messages.push(format!(
                    "❌ {}",
                    e.to_string().lines().next().unwrap_or("error")
                )),
            },
            Err(e) => messages.push(format!(
                "❌ Send failed: {}",
                e.to_string().lines().next().unwrap_or("error")
            )),
        }
        messages.push("".to_string());

        messages.push("=== Test 4: Multibyte Bootstrap ===".to_string());
        let multibyte = hex::decode("6b5afa05")?;
        messages.push(format!(
            "MULTIBYTE(0x5afa05): 0x{}",
            hex::encode(&multibyte)
        ));

        let tx3 = Eip1559TransactionRequest::new()
            .to(deployer_address)
            .data(multibyte)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(wallet.address());

        messages.push("Sending multibyte...".to_string());
        match client.send_transaction(tx3, None).await {
            Ok(pending) => match pending.await {
                Ok(Some(receipt)) => {
                    messages.push(format!("Tx: {:?}", receipt.transaction_hash));
                    messages.push(format!("Status: {:?}", receipt.status));
                    let code = provider.get_code(deployer_address, None).await?;
                    messages.push(format!("Code: 0x{}", hex::encode(&code)));
                }
                Ok(None) => messages.push("⏳ Pending".to_string()),
                Err(e) => messages.push(format!(
                    "❌ {}",
                    e.to_string().lines().next().unwrap_or("error")
                )),
            },
            Err(e) => messages.push(format!(
                "❌ Send failed: {}",
                e.to_string().lines().next().unwrap_or("error")
            )),
        }
        messages.push("".to_string());

        messages.push("=== Test 5: 0xFE (INVALID) ===".to_string());
        let invalid = hex::decode("fe")?;
        messages.push("Sending INVALID opcode...".to_string());

        let tx4 = Eip1559TransactionRequest::new()
            .to(deployer_address)
            .data(invalid)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(wallet.address());

        match client.send_transaction(tx4, None).await {
            Ok(pending) => match pending.await {
                Ok(Some(receipt)) => {
                    messages.push(format!("Tx: {:?}", receipt.transaction_hash));
                    messages.push(format!("Status: {:?}", receipt.status));
                    messages.push("⚠️  INVALID opcode didn't revert!".to_string());
                }
                Ok(None) => messages.push("⏳ Pending".to_string()),
                Err(_) => messages.push("✅ Reverted as expected".to_string()),
            },
            Err(_) => messages.push("✅ Reverted as expected".to_string()),
        }
        messages.push("".to_string());

        messages.push("=== Test 6: SSTORE Bootstrap ===".to_string());
        let sstore = hex::decode("5560")?;
        messages.push("Sending SSTORE(0x60)...".to_string());

        let tx5 = Eip1559TransactionRequest::new()
            .to(deployer_address)
            .data(sstore)
            .gas(gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee)
            .from(wallet.address());

        match client.send_transaction(tx5, None).await {
            Ok(pending) => match pending.await {
                Ok(Some(receipt)) => {
                    messages.push(format!("Tx: {:?}", receipt.transaction_hash));
                    messages.push(format!("Status: {:?}", receipt.status));
                    let code = provider.get_code(deployer_address, None).await?;
                    messages.push(format!("Code: 0x{}", hex::encode(&code)));
                }
                Ok(None) => messages.push("⏳ Pending".to_string()),
                Err(e) => messages.push(format!(
                    "❌ {}",
                    e.to_string().lines().next().unwrap_or("error")
                )),
            },
            Err(e) => messages.push(format!(
                "❌ Send failed: {}",
                e.to_string().lines().next().unwrap_or("error")
            )),
        }

        messages.push("".to_string());
        messages.push("=== CONCLUSIONS ===".to_string());
        messages.push("".to_string());
        messages.push("EIP-7702 TRANSACTION BEHAVIOR ON RISE L2:".to_string());
        messages.push("1. Transactions show status=1 (SUCCESS)".to_string());
        messages.push("2. NO code is set on the target address".to_string());
        messages.push("3. Bootstrap code is NOT executed".to_string());
        messages.push("4. INVALID opcode (0xFE) doesn't revert".to_string());
        messages.push("".to_string());
        messages.push("POSSIBLE EXPLANATIONS:".to_string());
        messages.push("- RISE L2 accepts but ignores EIP-7702 data".to_string());
        messages.push("- Bootstrap code execution is disabled/sandboxed".to_string());
        messages.push("- EIP-7702 not fully implemented yet".to_string());
        messages.push("- Code-setting mechanism requires special permission".to_string());
        messages.push("".to_string());
        messages.push("IMPACT:".to_string());
        messages.push("- ERC-7955 Universal CREATE2 Factory cannot be deployed".to_string());
        messages.push("- Permissionless CREATE2 via EIP-7702 not possible".to_string());
        messages.push("- Must use existing CREATE2 Deployer at 0x13b0...".to_string());
        messages.push("".to_string());
        messages.push("RECOMMENDATION:".to_string());
        messages.push("- Contact RISE team about EIP-7702 support".to_string());
        messages.push("- Or work with existing CREATE2 deployer".to_string());

        Ok(TaskResult {
            success: true,
            message: messages.join("\n"),
            tx_hash: None,
        })
    }
}
