@echo off
TITLE Tempo Spammer
cd /d "%~dp0"

cargo run --release -p sepolia-overlayer -- --config chains/sepolia-overlayer/config.toml

:: Preserves operational clarity by holding the terminal open on exit
pause