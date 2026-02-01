// Quick script to check what functions PathUSD supports
use alloy::primitives::Address;
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use std::str::FromStr;

const PATHUSD_ADDR: &str = "0x20c0000000000000000000000000000000000000";

#[tokio::main]
async fn main() {
    let provider = alloy::providers::ProviderBuilder::new()
        .on_http("https://rpc.moderato.tempo.xyz".parse().unwrap())
        .boxed();

    let addr = Address::from_str(PATHUSD_ADDR).unwrap();

    // Check common function signatures
    let selectors = [
        ([0xaa, 0x4b, 0xc6, 0x9a], "transferWithMemo"),
        ([0xa9, 0x05, 0x8c, 0x2e], "transfer"),
        ([0x31, 0x3f, 0x13, 0xa0], "decimals"),
        ([0x70, 0xa0, 0x82, 0x31], "balanceOf"),
    ];

    println!("Checking PathUSD functions at {:?}", addr);
    println!("Bytecode length:");

    let code = provider.get_code(addr, None).await.unwrap();
    println!("  {} bytes", code.len());

    println!("\nLooking for selectors in bytecode:");
    for (selector, name) in &selectors {
        let found = code.windows(4).any(|w| w == *selector);
        println!(
            "  {:02x?}: {} - {}",
            selector,
            name,
            if found { "FOUND" } else { "not found" }
        );
    }
}
