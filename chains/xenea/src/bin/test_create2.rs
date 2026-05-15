use ethers::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rpc_url = "https://testnet.riselabs.xyz";
    let provider = Provider::<Http>::try_from(rpc_url)?;

    let factories = vec![
        "0x13b0D85CcB8bf860b6b79AF3029fCA081AE9beF2",
        "0x4e59b44847b379578588920ca78fbf26c0b4956c",
    ];

    for factory_addr_str in factories {
        let create2_address: Address = factory_addr_str.parse()?;

        println!("--------------------------------------------------");
        println!("Checking Factory: {:?}", create2_address);

        let code = provider.get_code(create2_address, None).await?;
        println!("Code length: {} bytes", code.len());
        if code.len() > 0 {
            println!(
                "Code (first 100 bytes): 0x{}",
                hex::encode(&code[..code.len().min(100)])
            );
        } else {
            println!("NO CODE AT THIS ADDRESS");
            continue;
        }

        println!("\nTrying different method names...");

        // Try common CREATE2 function names
        let test_abis = vec![
            (
                "deploy(uint256,bytes32,bytes)",
                r#"[{"type":"function","name":"deploy","stateMutability":"nonpayable","inputs":[{"name":"salt","type":"uint256"},{"name":"bytecodeHash","type":"bytes32"},{"name":"data","type":"bytes"}],"outputs":[]}]"#,
            ),
            (
                "deploy2(uint256,bytes)",
                r#"[{"type":"function","name":"deploy2","stateMutability":"nonpayable","inputs":[{"name":"salt","type":"uint256"},{"name":"bytecode","type":"bytes"}],"outputs":[]}]"#,
            ),
            (
                "create2(bytes,bytes32)",
                r#"[{"type":"function","name":"create2","stateMutability":"nonpayable","inputs":[{"name":"bytecode","type":"bytes"},{"name":"salt","type":"bytes32"}],"outputs":[]}]"#,
            ),
            (
                "create2(bytes)",
                r#"[{"type":"function","name":"create2","stateMutability":"nonpayable","inputs":[{"name":"bytecode","type":"bytes"}],"outputs":[]}]"#,
            ),
        ];

        for (name, abi_json) in &test_abis {
            let abi: abi::Abi = serde_json::from_str(abi_json)?;
            let contract = Contract::new(create2_address, abi, Arc::new(provider.clone()));

            // Try to call the function with dummy data (using eth_call to simulate)
            let salt: u64 = 12345;
            let dummy_hash = H256::repeat_byte(0xab);
            let dummy_data = vec![0u8; 32];

            // Note: We use call() which is a read-only simulation.
            // If the method doesn't exist or reverts, we'll get an error.

            let call_future = if name.contains("deploy(uint256,bytes32,bytes)") {
                contract
                    .method::<_, ()>("deploy", (U256::from(salt), dummy_hash, dummy_data.clone()))
            } else if name.contains("deploy2") {
                contract.method::<_, ()>("deploy2", (U256::from(salt), dummy_data.clone()))
            } else if name.contains("create2(bytes,bytes32)") {
                contract
                    .method::<_, ()>("create2", (dummy_data.clone(), H256::from_low_u64_be(salt)))
            // salt might be bytes32
            } else {
                contract.method::<_, ()>("create2", (dummy_data.clone(),))
            };

            match call_future {
                Ok(method) => {
                    // We just want to check if encoding works and if we can call it.
                    // It will likely revert because of invalid data/salt, but we check for "revert" vs "does not exist".
                    match method.call().await {
                        Ok(_) => println!("✓ {} - call succeeded (unexpected!)", name),
                        Err(e) => {
                            let msg = e.to_string();
                            if msg.contains("execution reverted") {
                                println!("✓ {} - method exists (reverted as expected)", name);
                            } else {
                                println!(
                                    "? {} - error: {}",
                                    name,
                                    msg.lines().next().unwrap_or("unknown")
                                );
                            }
                        }
                    }
                }
                Err(e) => println!("✗ {} - method encoding failed: {}", name, e),
            }
        }
    }

    Ok(())
}
