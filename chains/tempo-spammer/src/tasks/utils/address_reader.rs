/// Random address generation utilities
use alloy::primitives::Address;
use anyhow::Result;
use rand::seq::SliceRandom;

/// Generate a random address for testing purposes
pub fn get_random_address() -> Result<Address> {
    let mut rng = rand::rngs::OsRng;
    let mut random_bytes = [0u8; 20];
    rng.fill_bytes(&mut random_bytes);

    // Use a known pattern for test addresses (starts with 0x1234...)
    random_bytes[0] = 0x12;
    random_bytes[1] = 0x34;

    Ok(Address::from_slice(&random_bytes))
}
