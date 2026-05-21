/// Calculate a percentage of a token balance, rounded to the nearest whole unit
/// at the given decimal precision. Uses saturating multiplication to avoid overflow.
/// Returns 0 if the result rounds to 0.
pub fn calc_pct_rounded(balance: u128, pct_numer: u128, pct_denom: u128, decimals: u32) -> u128 {
    let pct_raw = balance.saturating_mul(pct_numer) / pct_denom;
    let unit = 10u128.pow(decimals);
    let half = unit / 2;
    let whole = (pct_raw + half) / unit;
    whole * unit
}

/// Calculate 80% of a 6-decimal token balance, rounded to nearest whole unit.
/// Returns 0 if the result rounds to 0.
pub fn calc_eighty_pct_6dec(balance_raw: u128) -> u128 {
    calc_pct_rounded(balance_raw, 80, 100, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_returns_zero() {
        assert_eq!(calc_eighty_pct_6dec(0), 0);
    }

    #[test]
    fn test_one_usdt_rounds_up() {
        assert_eq!(calc_eighty_pct_6dec(1_000_000), 1_000_000);
    }

    #[test]
    fn test_hundred_usdt_exact() {
        assert_eq!(calc_eighty_pct_6dec(100_000_000), 80_000_000);
    }

    #[test]
    fn test_small_balance_rounds_to_zero() {
        assert_eq!(calc_eighty_pct_6dec(500_000), 0);
    }

    #[test]
    fn test_very_large_balance_does_not_overflow() {
        let large = u128::MAX / 10;
        let result = calc_eighty_pct_6dec(large);
        assert_ne!(result, 0, "Result should be non-zero for large input");
        assert!(
            result < large,
            "Result {} should be less than input {}",
            result,
            large
        );
    }

    #[test]
    fn test_calc_pct_rounded_matches_existing_eighty_pct() {
        let result = calc_pct_rounded(1_000_000, 80, 100, 6);
        assert_eq!(result, 1_000_000);
    }

    #[test]
    fn test_calc_pct_rounded_five_pct_eighteen_dec() {
        // 1 T+ (10^18 raw) → 5% = 0.05 T+ → rounds to 0
        let result = calc_pct_rounded(1_000_000_000_000_000_000u128, 5, 100, 18);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_calc_pct_rounded_two_pct_sixteen_dec() {
        // Used by t08/t09 unstake tasks — 2% at 16 decimals (0.01 shares rounding)
        let balance = 100 * 10u128.pow(18);
        let result = calc_pct_rounded(balance, 2, 100, 16);
        assert!(result > 0, "2% of 100 tokens should be non-zero");
        assert_eq!(
            result % 10u128.pow(16),
            0,
            "Should be rounded to 2 dp (multiple of 10^16)"
        );
    }

    #[test]
    fn test_calc_pct_rounded_zero_balance_returns_zero() {
        let result = calc_pct_rounded(0, 50, 100, 6);
        assert_eq!(result, 0);
    }
}
