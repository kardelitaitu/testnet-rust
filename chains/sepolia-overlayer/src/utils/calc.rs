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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_returns_zero() {
        assert_eq!(calc_pct_rounded(0, 50, 100, 6), 0);
    }

    #[test]
    fn test_calc_pct_rounded_five_pct_eighteen_dec() {
        // 1 T+ (10^18 raw) → 5% = 0.05 T+ → rounds to 0
        let result = calc_pct_rounded(1_000_000_000_000_000_000u128, 5, 100, 18);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_calc_pct_rounded_zero_balance_returns_zero() {
        let result = calc_pct_rounded(0, 50, 100, 6);
        assert_eq!(result, 0);
    }
}
