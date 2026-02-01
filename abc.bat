@echo off
setlocal enabledelayedexpansion

:: --- CONFIGURATION ---
:: Pointing directly to the .exe bypasses Cargo overhead for 2,500+ calls.
set "BIN=target\debug\tempo-debug.exe"
set "TASKS=2 4 7 21 22"
set "START_WALLET=1"
set "END_WALLET=500"

echo [ARCHITECT] Starting session for %END_WALLET% wallets...
echo [ARCHITECT] Binary: %BIN%
echo ---------------------------------------------------

:: --- WALLET LOOP (1 to 500) ---
for /L %%W in (%START_WALLET%, 1, %END_WALLET%) do (
    echo [WALLET %%W/%END_WALLET%] Processing...
    
    :: --- TASK SEQUENCE ---
    for %%T in (%TASKS%) do (
        echo   ^> Executing Task %%T...
        
        :: Execute the binary
        "%BIN%" --task %%T --wallet %%W
        
        :: Error Handling: Halt the entire sequence if a task fails
        if errorlevel 1 (
            echo.
            echo [!] FATAL ERROR: Wallet %%W, Task %%T failed.
            echo [!] Aborting to maintain session hygiene.
            pause
            exit /b 1
        )
    )
    echo [WALLET %%W] Completed.
    echo ---------------------------------------------------
)

echo [DONE] Mission complete. All 500 wallets processed.
pause