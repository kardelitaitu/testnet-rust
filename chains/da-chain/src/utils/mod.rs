pub mod address_cache;
pub mod gas;

#[cfg(test)]
mod tests {
    #[test]
    fn test_address_cache_module_resolves() {
        // Can't construct AddressCache directly (private fields),
        // but can call static methods
        let _ = crate::utils::address_cache::AddressCache::len();
    }

    #[test]
    fn test_gas_module_resolves() {
        let _ = crate::utils::gas::GasManager::MAX_FEE_GWEI_DEFAULT;
        let _ = crate::utils::gas::GasManager::LIMIT_DEPLOY;
    }
}
