use anyhow::{Context, Result};
use ethers::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let rpc_url = "https://testnet.riselabs.xyz";
    let provider = Provider::<Http>::try_from(rpc_url)?;

    // Use a wallet to sign transactions (reuse the one from debug_task usually, or hardcoded)
    // From config.toml: private_key_file = "../../wallets/evm.json"
    // I will use a random wallet or the one from debug_task if I can access it.
    // Let's use a random wallet for now and fund it? No, need testnet funds.
    // I'll try to load the wallet from the file used in debug_task.
    // Assuming "wallets/evm.json" exists and has keys.
    // Actually, I can just use a hardcoded private key if I have one, or use the one from the environment.
    // The `debug_task` uses wallet files.

    // Let's try to read one wallet from c:\My Script\testnet-framework\wallets\evm.json
    // But that file might be encrypted or just a list.
    // Let's look at `chains/risechain/src/bin/debug_task.rs` to see how it loads wallets.

    // To save time, I will use the private key of wallet 15 which was used in the tasks.
    // Address: 0x167bbc576052d4d2134a23bfa53ddacd107d1bb4
    // But I don't have the private key exposed in the logs.

    // I will modify `verify_create2_opcode.rs` to accept a private key or use the wallet loader.
    // Better yet, I can reuse the `rise-project` library code to load wallets.

    // For now, I will use a known private key if possible.
    // Or I can just write a script that runs inside the `rise-project` structure and uses its helpers.
    // `chains/risechain/src/bin/verify_create2.rs`

    let wallet_path = "C:\\My Script\\testnet-framework\\wallets\\evm.json";
    let wallet_content =
        std::fs::read_to_string(wallet_path).context("Failed to read wallet file")?;
    let wallets: Vec<String> = serde_json::from_str(&wallet_content)?;

    if wallets.is_empty() {
        return Err(anyhow::anyhow!("No wallets found"));
    }

    let priv_key = &wallets[0]; // Use the first wallet
    let wallet: LocalWallet = priv_key.parse()?;
    let chain_id = 11155931u64;
    let wallet = wallet.with_chain_id(chain_id);
    let address = wallet.address();
    println!("Using wallet: {:?}", address);

    let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.clone()));

    // Deploy SimpleFactory
    let factory_bytecode = "6080604052348015600f57600080fd5b5061020f8061001f6000396000f3fe608060405234801561001057600080fd5b506004361061002b5760003560e01c806361ff715f14610030575b600080fd5b61004361003e366004610116565b61005f565b6040516001600160a01b03909116815260200160405180910390f35b6000828251602084016000f590506001600160a01b0381166100b85760405162461bcd60e51b815260206004820152600e60248201526d10dc99585d194c8819985a5b195960921b604482015260640160405180910390fd5b604080516001600160a01b0383168152602081018590527fb03c53b28e78a88e31607a27e1fa48234dce28d5d9d9ec7b295aeb02e674a1e1910160405180910390a192915050565b634e487b7160e01b600052604160045260246000fd5b6000806040838503121561012957600080fd5b82359150602083013567ffffffffffffffff81111561014757600080fd5b8301601f8101851361015857600080fd5b803567ffffffffffffffff81111561017257610172610100565b604051601f8201601f19908116603f0116810167ffffffffffffffff811182821017156101a1576101a1610100565b6040528181528282016020018710156101b957600080fd5b81602084016020830137600060208383010152809350505050925092905056fea26469706673582212203d752ff8928077cad5caebcfb0833f2e92dd8a67636e26a88b59280bc1e801cc64736f6c63430008210033";
    let factory_bytes = hex::decode(factory_bytecode)?;
    let tx = Eip1559TransactionRequest::new()
        .data(factory_bytes)
        .max_fee_per_gas(U256::from(2000000000u64)) // 2 gwei
        .max_priority_fee_per_gas(U256::from(1000000000u64)); // 1 gwei

    println!("Deploying SimpleFactory...");
    let pending_tx = client.send_transaction(tx, None).await?;
    let receipt = pending_tx.await?.context("Failed to get receipt")?;
    let factory_address = receipt.contract_address.context("No contract address")?;
    println!("SimpleFactory deployed at: {:?}", factory_address);

    // Prepare child contract bytecode (simple return)
    // 0x600060205260206020f3 -> PUSH1 0 PUSH1 32 MSTORE PUSH1 32 PUSH1 32 RETURN (returns 32 bytes of zeros)
    // Or simpler: 00 (STOP)
    let child_bytecode = hex::decode("600060205260206020f3")?;

    let abi_json = r#"[{"inputs":[{"internalType":"uint256","name":"salt","type":"uint256"},{"internalType":"bytes","name":"bytecode","type":"bytes"}],"name":"deploy","outputs":[{"internalType":"address","name":"addr","type":"address"}],"stateMutability":"nonpayable","type":"function"}]"#;
    let abi: abi::Abi = serde_json::from_str(abi_json)?;
    let contract = Contract::new(factory_address, abi, client.clone());

    let salt = U256::from(12345);
    println!("Calling deploy with salt {}...", salt);

    let call_tx = contract.method::<_, Address>("deploy", (salt, child_bytecode.clone()))?;
    let pending_call = call_tx.send().await?;
    let call_receipt = pending_call.await?.context("Failed to get call receipt")?;

    println!("Deploy call status: {:?}", call_receipt.status);

    // Check event
    let logs = call_receipt.logs;
    if !logs.is_empty() {
        println!("Got {} logs", logs.len());
        for log in logs {
            println!("Log: {:?}", log);
        }
    } else {
        println!("No logs found - CREATE2 might have failed silently or reverted without message");
    }

    Ok(())
}
