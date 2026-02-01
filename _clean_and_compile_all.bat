@echo off
setlocal enabledelayedexpansion
echo ===================================================
echo   TESTNET FRAMEWORK - DIRECT BUILD (NO ELEVATION)
echo ===================================================

REM 1. Configure High-Performance Build Environment
REM Use a specific target directory to avoid lock conflicts from other processes
set CARGO_TARGET_DIR=target\final
REM Disable incremental compilation to prevent "os error 32" file locks
set CARGO_BUILD_INCREMENTAL=false
REM Optimize linking speed
set CARGO_PROFILE_DEV_SPLIT_DEBUGINFO=unpacked
REM Use native CPU instructions
set RUSTFLAGS=-C target-cpu=native

echo [INFO] Target Directory: %CARGO_TARGET_DIR%
echo [INFO] Incremental Build: Disabled (Safe Mode)

REM 2. Kill Lingering Processes (Best Effort)
echo [1/4] Attempting to kill lingering processes...
taskkill /F /IM cargo.exe >nul 2>&1
taskkill /F /IM rustc.exe >nul 2>&1
taskkill /F /IM rust-analyzer.exe >nul 2>&1
taskkill /F /IM evm-project.exe >nul 2>&1
taskkill /F /IM solana-project.exe >nul 2>&1

REM 3. Clean (Fast)
REM Since we are non-incremental, we can optionally clean specific crates or just build.
REM "target_final" should remain relatively clean.
REM We skip "cargo clean" here to allow some caching if possible, 
REM but since INCREMENTAL=false, it will rebuild crates mostly anyway.

REM 4. Build Workspace
echo [3/4] Building Workspace (Parallel)...
cargo build --workspace
if %errorlevel% neq 0 (
    echo [ERROR] Build Failed!
    ::pause
    exit /b %errorlevel%
)

echo [4/4] Build Complete.
echo ===================================================
echo   SUCCESS!
echo   Executable: %CARGO_TARGET_DIR%\debug\evm-project.exe
echo ===================================================
timeout 1
exit
