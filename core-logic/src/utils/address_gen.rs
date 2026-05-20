//! # Random Address Generation
//!
//! Utility for generating random EVM-compatible addresses.
//! Uses cryptographically secure random bytes from `rand`.

use rand::Rng;

/// Generate a random EVM-compatible address (20 bytes) as a hex string with `0x` prefix.
///
/// Uses `OsRng` for cryptographically secure randomness.
///
/// # Returns
///
/// A hex-encoded address string, e.g. `"0x742d35Cc6634C0532925a3b844Bc454e4438f44e"`.
///
/// # Example
///
/// ```
/// use core_logic::generate_random_address;
///
/// let addr = generate_random_address();
/// assert_eq!(addr.len(), 42);          // 0x + 40 hex chars
/// assert!(addr.starts_with("0x"));
/// assert!(addr[2..].chars().all(|c| c.is_ascii_hexdigit()));
/// ```
pub fn generate_random_address() -> String {
    let mut bytes = [0u8; 20];
    rand::rngs::OsRng.fill(&mut bytes);
    format!("0x{}", hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_random_address_format() {
        let addr = generate_random_address();
        assert_eq!(addr.len(), 42, "Expected 0x + 40 hex chars");
        assert!(addr.starts_with("0x"), "Must start with 0x");
        assert!(
            addr[2..].chars().all(|c| c.is_ascii_hexdigit()),
            "All chars after 0x must be hex digits"
        );
    }

    #[test]
    fn test_generate_random_address_length() {
        let addr = generate_random_address();
        // 20 bytes = 40 hex chars + 0x prefix = 42
        assert_eq!(addr.len(), 42);
    }

    #[test]
    fn test_generate_random_address_unique() {
        let a = generate_random_address();
        let b = generate_random_address();
        assert_ne!(a, b, "Two calls should produce different addresses");
    }

    #[test]
    fn test_generate_random_address_lowercase_hex() {
        let addr = generate_random_address();
        let hex_part = &addr[2..];
        // hex::encode always produces lowercase
        assert_eq!(hex_part, hex_part.to_lowercase());
    }

    #[test]
    fn test_generate_random_address_checksum_candidate() {
        let addr = generate_random_address();
        // Should be valid hex that could be checksummed
        let hex_part = &addr[2..];
        assert_eq!(hex_part.len(), 40);
        // Verify it decodes to 20 bytes
        let decoded = hex::decode(hex_part).unwrap();
        assert_eq!(decoded.len(), 20);
    }
}
