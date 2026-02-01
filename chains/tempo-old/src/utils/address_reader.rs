use anyhow::{anyhow, Result};
use ethers::types::Address;
use rand::seq::SliceRandom;
use std::fs;
use std::path::Path;

pub fn get_random_address() -> Result<Address> {
    let address_file = Path::new("chains/tempo/address.txt");
    let content = fs::read_to_string(address_file)
        .map_err(|e| anyhow!("Failed to read address file: {}", e))?;

    let addresses: Vec<Address> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter(|line| line.starts_with("0x"))
        .map(|line| {
            line.trim()
                .parse::<Address>()
                .unwrap_or_else(|_| panic!("Invalid address: {}", line))
        })
        .collect();

    if addresses.is_empty() {
        return Err(anyhow!("No addresses found in address.txt"));
    }

    let mut rng = rand::thread_rng();
    addresses
        .choose(&mut rng)
        .copied()
        .ok_or_else(|| anyhow!("Failed to select random address"))
        .map(|addr| {
            tracing::debug!(target: "smart_main", "Selected random address: {:?}", addr);
            addr
        })
}

pub fn get_multiple_random_addresses(count: usize) -> Result<Vec<Address>> {
    let address_file = Path::new("chains/tempo/address.txt");
    let content = fs::read_to_string(address_file)
        .map_err(|e| anyhow!("Failed to read address file: {}", e))?;

    let addresses: Vec<Address> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter(|line| line.starts_with("0x"))
        .map(|line| {
            line.trim()
                .parse::<Address>()
                .unwrap_or_else(|_| panic!("Invalid address: {}", line))
        })
        .collect();

    if addresses.is_empty() {
        return Err(anyhow!("No addresses found in address.txt"));
    }

    if count > addresses.len() {
        return Err(anyhow!(
            "Requested {} addresses but only {} available",
            count,
            addresses.len()
        ));
    }

    let mut rng = rand::thread_rng();
    let selected = addresses
        .choose_multiple(&mut rng, count)
        .copied()
        .collect();

    tracing::debug!(target: "smart_main", "Selected {} random addresses", count);
    Ok(selected)
}
