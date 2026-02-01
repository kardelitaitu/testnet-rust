# Changelog

All notable changes to the tempo-spammer project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Comprehensive documentation for all 50 tasks in `docs/TASK_CATALOG.md`
- Module-level documentation for all core modules:
  - `src/lib.rs` - Crate-level documentation with examples
  - `src/client.rs` - TempoClient documentation
  - `src/client_pool.rs` - Client pool and leasing documentation
  - `src/nonce_manager.rs` - Nonce caching documentation
  - `src/proxy_health.rs` - Proxy health checking documentation
  - `src/tasks/mod.rs` - Task system documentation
- Task tracking document `TODO.md` with all 44 development TODOs
- Updated `README.md` with complete task table (all 50 tasks)

### Changed
- Enhanced existing documentation with comprehensive rustdoc comments
- Added inline examples to all public APIs
- Improved project structure documentation

## [0.1.0] - 2024-01-30

### Added
- Initial release with 50 task implementations
- Alloy 1.4.3 integration for blockchain interactions
- Multi-wallet support with automatic rotation
- Proxy support with health checking and automatic failover
- Nonce management with caching for high-throughput scenarios
- SQLite database persistence for metrics and asset tracking
- Client pool with RAII-based wallet leasing
- Proxy banlist with automatic recovery
- 50+ task implementations:
  - Core operations (deploy, faucet, transfers)
  - Token operations (create, mint, swap, burn)
  - Batch operations (batch swaps, batch transfers)
  - Multi-send operations (disperse, concurrent)
  - NFT operations (create, mint, domains)
  - Advanced features (viral faucets, time bombs, storms)
- Binary targets:
  - `tempo-spammer` - Main multi-worker spammer
  - `tempo-debug` - Single task testing
  - `tempo-runner` - Sequential execution
  - `tempo-sequence` - Sequence executor
  - `debug_proxy` - Proxy debugging

### Features
- **50+ Tasks**: Comprehensive testnet coverage
- **Multi-Wallet**: Support for multiple encrypted wallets
- **Proxy Rotation**: Automatic proxy health checking and rotation
- **Nonce Caching**: Thread-safe nonce management
- **Database Logging**: SQLite persistence for all operations
- **Weighted Distribution**: Favor high-volume tasks
- **Configurable**: TOML-based configuration
- **Async**: Built on tokio for high performance

### Technical
- Rust 2021 edition
- Alloy 1.4.3 for blockchain interactions
- Tokio async runtime
- SQLx for database operations
- Reqwest for HTTP with proxy support
- Tracing for structured logging

## Task History

### Phase 1: Core Tasks (01-10)
- Initial implementation of basic operations
- Contract deployment and interaction
- Token transfers and faucet claims

### Phase 2: Token Ecosystem (11-23)
- Stablecoin creation and management
- DEX integration (Fee AMM)
- Meme token support
- Liquidity operations

### Phase 3: Batch & Multi-Send (24-36)
- Batch transaction support
- Multi-send operations
- Concurrent execution patterns

### Phase 4: Advanced Features (37-50)
- Scheduled transfers
- Share distribution
- Viral mechanics
- NFT operations
- Storm deployment

## Migration Guide

### Upcoming Changes

#### Future Versions

##### Planned Features
- [ ] EIP-1559 fee estimation
- [ ] Multi-chain support
- [ ] WebSocket RPC support
- [ ] Prometheus metrics export
- [ ] Grafana dashboards
- [ ] REST API for remote control
- [ ] Task dependency graphs
- [ ] Dynamic task loading

##### Breaking Changes (Future)
- None planned currently

##### Deprecations
- None currently

## Contributors

Thanks to all contributors who have helped improve the tempo-spammer!

## Security

For security issues, please see [SECURITY.md](docs/SECURITY.md).
