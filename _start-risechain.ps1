Write-Host "Building Risechain Spammer..."
cargo build --bin rise-project --release

Write-Host ""
Write-Host "=========================================="
Write-Host "  Risechain Spammer"
Write-Host "=========================================="
Write-Host ""
Write-Host "Running Risechain Spammer..."

# Execute the binary directly. When Ctrl+C is pressed, PS handles it better than CMD batch.
& ".\target\release\rise-project.exe" --config chains/risechain/config.toml
