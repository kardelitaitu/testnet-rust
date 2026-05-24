pub mod calc;
pub mod gas;

#[cfg(test)]
mod tests {
    #[test]
    fn test_gas_module_resolves() {
        let _ = crate::utils::gas::GasManager::MAX_FEE_GWEI_DEFAULT;
        let _ = crate::utils::gas::GasManager::LIMIT_DEPLOY;
    }

    #[test]
    fn test_calc_module_accessible() {
        // calc module is accessible
        let _result = crate::utils::calc::calc_pct_rounded(0, 10, 100, 6);
    }
}
