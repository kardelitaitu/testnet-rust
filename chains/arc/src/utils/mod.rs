pub mod address_cache;
pub mod faucet;
pub mod gas;

#[cfg(test)]
mod tests {
    #[test]
    fn test_address_cache_module_resolves() {
        let _ = crate::utils::address_cache::AddressCache::len();
    }

    #[test]
    fn test_faucet_module_resolves() {
        let _ = crate::utils::faucet::FaucetResult {
            success: true,
            message: "test".into(),
        };
    }

    #[test]
    fn test_gas_module_resolves() {
        let _ = crate::utils::gas::GasManager::MAX_FEE_GWEI_DEFAULT;
        let _ = crate::utils::gas::GasManager::LIMIT_DEPLOY;
    }
}
