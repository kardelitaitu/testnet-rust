# Contributing to Tempo-Spammer

Thank you for your interest in contributing to the tempo-spammer project!

## Table of Contents
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Code Standards](#code-standards)
- [Making Changes](#making-changes)
- [Testing](#testing)
- [Documentation](#documentation)
- [Pull Request Process](#pull-request-process)
- [Code Review](#code-review)
- [Release Process](#release-process)

---

## Getting Started

### Prerequisites

- **Rust**: Latest stable version (1.75+)
- **Git**: For version control
- **Cargo**: Rust's package manager

### Fork and Clone

```bash
# Fork the repository on GitHub
# Then clone your fork:
git clone https://github.com/YOUR_USERNAME/tempo-spammer.git
cd tempo-spammer

# Add upstream remote
git remote add upstream https://github.com/original/tempo-spammer.git
```

---

## Development Setup

### Install Dependencies

```bash
# Build the project
cargo build --workspace

# Run tests
cargo test --workspace

# Install development tools
cargo install cargo-watch cargo-clippy cargo-audit
```

### Configure Environment

```bash
# Copy example config
cp chains/tempo-spammer/config/config.example.toml chains/tempo-spammer/config/config.toml

# Set wallet password (for testing)
export WALLET_PASSWORD="test_password"

# Enable debug logging
export RUST_LOG=debug
```

### IDE Setup

**Recommended VS Code Extensions:**
- rust-analyzer
- Even Better TOML
- CodeLLDB (for debugging)

**Recommended IntelliJ Plugins:**
- Rust Plugin
- TOML Plugin

---

## Code Standards

### Rust Style Guide

We follow the official [Rust Style Guide](https://doc.rust-lang.org/style-guide/):

```bash
# Format code before committing
cargo fmt

# Check formatting
cargo fmt -- --check
```

### Linting

```bash
# Run clippy
cargo clippy --workspace -- -D warnings

# Fix auto-fixable issues
cargo clippy --workspace --fix
```

### Code Quality

**Must Pass:**
- `cargo build` - No compilation errors
- `cargo test` - All tests pass
- `cargo fmt -- --check` - Code is formatted
- `cargo clippy -- -D warnings` - No warnings
- `cargo audit` - No security vulnerabilities

### Naming Conventions

| Item | Convention | Example |
|------|------------|---------|
| Structs | PascalCase | `TempoClient` |
| Functions | snake_case | `get_nonce` |
| Variables | snake_case | `gas_price` |
| Constants | SCREAMING_SNAKE_CASE | `MAX_RETRIES` |
| Modules | snake_case | `client_pool` |
| Traits | PascalCase | `TempoTask` |
| Enums | PascalCase | `TaskStatus` |

### Documentation Standards

**Required for all public APIs:**

```rust
/// Brief description of what this does
///
/// # Arguments
///
/// * `arg1` - Description of arg1
/// * `arg2` - Description of arg2
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// Description of possible errors
///
/// # Example
///
/// ```rust,no_run
/// let result = function_call(arg1, arg2).await?;
/// ```
pub async fn function_call(arg1: Type1, arg2: Type2) -> Result<ReturnType> {
    // Implementation
}
```

### Error Handling

**Use `anyhow` for application errors:**

```rust
use anyhow::{Context, Result};

pub async fn do_something() -> Result<()> {
    let data = fetch_data()
        .await
        .context("Failed to fetch data")?;
    
    process_data(data)
        .context("Failed to process data")?;
    
    Ok(())
}
```

**Use `thiserror` for library errors:**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MyError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("Network error")]
    Network(#[from] reqwest::Error),
}
```

---

## Making Changes

### Branch Naming

```
feature/description    - New features
fix/description        - Bug fixes
docs/description       - Documentation updates
refactor/description   - Code refactoring
test/description       - Test additions/changes
chore/description      - Maintenance tasks
```

**Examples:**
```bash
git checkout -b feature/add-nft-marketplace-task
git checkout -b fix/nonce-cache-race-condition
git checkout -b docs/update-task-catalog
```

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Formatting (no code change)
- `refactor`: Code restructuring
- `test`: Test additions/changes
- `chore`: Maintenance

**Examples:**
```bash
git commit -m "feat(tasks): add batch mint NFT task"
git commit -m "fix(client): handle nonce too low error"
git commit -m "docs(readme): update task table"
git commit -m "refactor(pool): simplify wallet leasing logic"
```

### Scope Guidelines

| Scope | Description |
|-------|-------------|
| `tasks` | Task implementations |
| `client` | Client/Provider code |
| `pool` | Client pool management |
| `config` | Configuration handling |
| `docs` | Documentation |
| `tests` | Test code |

---

## Testing

### Test Requirements

**All changes must include:**
- Unit tests for new functions
- Integration tests for new features
- Documentation tests for examples

### Running Tests

```bash
# Run all tests
cargo test --workspace

# Run specific package
cargo test -p tempo-spammer

# Run with output
cargo test -- --nocapture

# Run ignored tests
cargo test -- --ignored

# Run specific test
cargo test test_name -- --exact
```

### Writing Tests

**Unit Test Example:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_price_bumping() {
        let gas_manager = GasManager;
        let price = U256::from(1000000000u64);
        
        let bumped = gas_manager.bump_fees(price, 20);
        
        assert_eq!(bumped, U256::from(1200000000u64));
    }

    #[tokio::test]
    async fn test_client_creation() {
        let client = create_test_client().await;
        assert!(client.is_ok());
    }
}
```

**Integration Test Example:**

```rust
#[tokio::test]
async fn test_task_execution() {
    let ctx = create_test_context().await;
    let task = DeployContractTask::new();
    
    let result = task.run(&ctx).await;
    
    assert!(result.is_ok());
    assert!(result.unwrap().success);
}
```

### Test Coverage

Aim for >80% coverage on new code:

```bash
# Generate coverage report
cargo install cargo-tarpaulin
cargo tarpaulin --out Html

# View report
open tarpaulin-report.html
```

---

## Documentation

### Code Documentation

**Required for:**
- All public structs and functions
- Complex algorithms
- Non-obvious behavior
- Error conditions

**Format:**
```rust
/// Brief one-line description
///
/// Detailed description if needed. Explain what this does,
/// why it exists, and any important details.
///
/// # Arguments
///
/// * `param` - Description
///
/// # Returns
///
/// What the function returns
///
/// # Errors
///
/// When and why errors occur
///
/// # Examples
///
/// ```rust,no_run
/// let result = example().await?;
/// ```
```

### README Updates

Update README.md if you:
- Add new tasks
- Change CLI interface
- Add new features
- Change configuration

### Documentation Files

Update relevant docs in `docs/`:
- `TASK_CATALOG.md` - New tasks
- `CONFIG_REFERENCE.md` - New config options
- `TROUBLESHOOTING.md` - New issues/solutions
- `ARCHITECTURE.md` - Architectural changes

---

## Pull Request Process

### Before Creating PR

1. **Update your branch:**
```bash
git fetch upstream
git rebase upstream/main
```

2. **Run quality checks:**
```bash
cargo build --workspace
cargo test --workspace
cargo fmt -- --check
cargo clippy --workspace -- -D warnings
cargo audit
```

3. **Update documentation:**
   - Code comments
   - README.md (if needed)
   - docs/ files (if needed)

4. **Write good commit messages:**
   - Follow conventional commits
   - Explain what and why

### Creating the PR

1. **Push to your fork:**
```bash
git push origin feature/your-feature
```

2. **Create PR on GitHub:**
   - Use clear title
   - Fill out PR template
   - Link related issues

### PR Template

```markdown
## Description
Brief description of changes

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Documentation
- [ ] Refactoring
- [ ] Performance improvement

## Testing
- [ ] Unit tests added/updated
- [ ] Integration tests pass
- [ ] Manual testing performed

## Checklist
- [ ] Code follows style guidelines
- [ ] Self-review completed
- [ ] Documentation updated
- [ ] No new warnings
- [ ] Tests pass

## Related Issues
Fixes #123
```

---

## Code Review

### Review Criteria

**Reviewers check:**
- Code correctness
- Test coverage
- Documentation quality
- Performance implications
- Security considerations

### Responding to Reviews

1. **Be responsive:**
   - Address comments promptly
   - Ask questions if unclear
   - Explain reasoning

2. **Make requested changes:**
```bash
# Make changes
git add .
git commit -m "refactor: address review feedback"
git push origin feature/your-feature
```

3. **Resolve conversations:**
   - Mark as resolved when fixed
   - Comment if you disagree

### Review Timeline

- Initial review: Within 2 business days
- Follow-up reviews: Within 1 business day
- Merge after approval + CI pass

---

## Release Process

### Version Numbering

We follow [Semantic Versioning](https://semver.org/):

```
MAJOR.MINOR.PATCH

MAJOR - Breaking changes
MINOR - New features (backward compatible)
PATCH - Bug fixes
```

### Release Checklist

1. **Update version:**
```bash
# Update Cargo.toml
version = "0.2.0"
```

2. **Update CHANGELOG.md:**
```markdown
## [0.2.0] - 2024-02-01
### Added
- New feature X

### Fixed
- Bug Y
```

3. **Create git tag:**
```bash
git tag -a v0.2.0 -m "Release version 0.2.0"
git push origin v0.2.0
```

4. **Create GitHub Release:**
   - Draft release notes
   - Attach binaries
   - Publish

---

## Development Tips

### Debugging

```bash
# Run with debug logging
RUST_LOG=debug cargo run -p tempo-spammer --bin tempo-debug

# Run specific test with output
cargo test test_name -- --nocapture

# Use debugger
rust-gdb target/debug/tempo-spammer
```

### Performance Profiling

```bash
# Build with profiling
cargo build --release --features profiling

# Run profiler
perf record ./target/release/tempo-spammer
perf report
```

### Common Issues

**Build fails:**
```bash
# Clean and rebuild
cargo clean
cargo build
```

**Tests fail:**
```bash
# Check if tests need database
# Ensure WALLET_PASSWORD is set
export WALLET_PASSWORD="test"
```

**Clippy warnings:**
```bash
# Auto-fix
cargo clippy --fix
```

---

## Community

### Communication Channels

- **GitHub Issues**: Bug reports, feature requests
- **GitHub Discussions**: General questions
- **Discord**: Real-time chat (link TBD)

### Code of Conduct

- Be respectful and inclusive
- Welcome newcomers
- Focus on constructive feedback
- Respect differing viewpoints

### Recognition

Contributors will be:
- Listed in CONTRIBUTORS.md
- Mentioned in release notes
- Credited in relevant documentation

---

## Questions?

- Check [docs/](../docs/) directory
- Read [TROUBLESHOOTING.md](../docs/TROUBLESHOOTING.md)
- Open a GitHub Discussion
- Ask in Discord

---

Thank you for contributing! 🎉

**Last Updated:** 2024-01-30  
**Version:** 0.1.0
