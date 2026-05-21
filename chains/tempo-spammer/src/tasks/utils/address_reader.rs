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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_random_address_returns_valid() {
        let addr = get_random_address().unwrap();
        let s = addr.to_string();
        assert!(s.starts_with("0x1234"), "Address should start with 0x1234, got: {}", s);
        assert_eq!(s.len(), 42, "Address should be 42 hex chars, got: {}", s);
    }

    #[test]
    fn test_get_random_address_is_20_bytes() {
        let addr = get_random_address().unwrap();
        let bytes = addr.as_slice();
        assert_eq!(bytes.len(), 20, "Address should be 20 bytes");
        assert_eq!(bytes[0], 0x12);
        assert_eq!(bytes[1], 0x34);
    }
}
