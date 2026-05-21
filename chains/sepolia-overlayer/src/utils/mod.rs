pub mod gas;

#[cfg(test)]
mod tests {
    #[test]
    fn test_gas_module_resolves() {
        let _ = crate::utils::gas::GasManager::MAX_FEE_GWEI_DEFAULT;
        let _ = crate::utils::gas::GasManager::LIMIT_DEPLOY;
    }
}
