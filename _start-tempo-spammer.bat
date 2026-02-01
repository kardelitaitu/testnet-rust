@echo off
TITLE Tempo Spammer
cd /d "%~dp0"

:: Optimize for local machine
set RUSTFLAGS=-C target-cpu=native

echo ========================================================
echo               STARTING TEMPO SPAMMER
echo ========================================================
echo.

:: Check if wallet password is set
if "%WALLET_PASSWORD%"=="" (
    set /p WALLET_PASSWORD="Enter Wallet Password: "
)

:: Ask for worker count (optional)
set /p WORKERS="Enter Worker Count (Press Enter for config default): "

if "%WORKERS%"=="" (
    cargo run --release --manifest-path chains/tempo-spammer/Cargo.toml --bin tempo-spammer -- spammer --quiet
) else (
    cargo run --release --manifest-path chains/tempo-spammer/Cargo.toml --bin tempo-spammer -- spammer --workers %WORKERS% --quiet
)

if errorlevel 1 (
    echo.
    echo [ERROR] Spammer crashed or failed to compile.
    pause
)
