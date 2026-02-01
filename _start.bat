@echo off
echo Building Spammer Orchestrator...
cargo build --workspace --release --quiet

echo.
echo ==========================================
echo   Spammer Orchestrator
echo ==========================================
echo 1. Run EVM Spammer
echo 2. Run Solana Spammer
echo 3. Run Rise Spammer
echo 4. Exit
echo.

set /p choice="Select chain (1-4): "

if "%choice%"=="1" (
    echo Running EVM Spammer...
    cd chains\evm-project
    ..\..\target\release\evm-project.exe --config config.toml
) else if "%choice%"=="2" (
    echo Running Solana Spammer...
    cd chains\solana-project
    ..\..\target\release\solana-project.exe --config config.toml
) else if "%choice%"=="3" (
    echo Running Rise Spammer...
    target\release\rise-project.exe --config chains\risechain\config.toml
) else (
    echo Exiting...
)
pause