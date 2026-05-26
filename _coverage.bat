@echo off
REM Coverage runner for testnet-framework
REM Usage: _coverage [--lib|--all|--html|--lcov] [package]

set PKG=core-logic
set FLAGS=--lib

if "%1"=="--all" set FLAGS=
if not "%2"=="" set PKG=%2

echo === Running coverage for %PKG% ===
cargo llvm-cov %FLAGS% -p %PKG% 2>nul

if "%1"=="--html" (
    echo === Generating HTML report ===
    cargo llvm-cov %FLAGS% -p %PKG% --html
    echo Report: target\llvm-cov\html\index.html
)

if "%1"=="--lcov" (
    echo === Generating LCOV report ===
    cargo llvm-cov %FLAGS% -p %PKG% --lcov --output-path coverage-%PKG%.lcov
    echo Report: coverage-%PKG%.lcov
)