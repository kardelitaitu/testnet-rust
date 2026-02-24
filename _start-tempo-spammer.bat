@echo off
TITLE Tempo Spammer
cd /d "%~dp0"

cargo run -p tempo-spammer --bin tempo-spammer --release -- spammer

:: Preserves operational clarity by holding the terminal open on exit
pause