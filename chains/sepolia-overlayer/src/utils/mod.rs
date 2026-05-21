pub mod gas;
pub mod calc;

#[cfg(test)]
mod tests {
    #[test]
    fn test_gas_module_resolves() {
        let _ = crate::utils::gas::GasManager::MAX_FEE_GWEI_DEFAULT;
        let _ = crate::utils::gas::GasManager::LIMIT_DEPLOY;
    }

    #[test]
    fn test_calc_eighty_pct_6dec_via_utils_module() {
        // This function should exist in the calc module
        let _result = crate::utils::calc::calc_eighty_pct_6dec(0);
    }
}
