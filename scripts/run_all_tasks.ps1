$ErrorActionPreference = "Stop"

Write-Host "Building project..."
cargo build --bin debug_task
if ($LASTEXITCODE -ne 0) {
    Write-Error "Build failed"
    exit 1
}

$exePath = "target/debug/debug_task.exe"
$logFile = "task_execution_report.log"
$summaryFile = "task_summary.md"

"Task Execution Report - $(Get-Date)" | Out-File -FilePath $logFile -Encoding utf8
"# Task Execution Summary" | Out-File -FilePath $summaryFile -Encoding utf8
"| Task ID | Status | Message |" | Out-File -FilePath $summaryFile -Append -Encoding utf8
"|---|---|---|" | Out-File -FilePath $summaryFile -Append -Encoding utf8

# Task IDs roughly 0 to 55 (based on file list)
# We can detect the max later, but for now loop 0..55
for ($i = 0; $i -le 55; $i++) {
    Write-Host "Running Task $i..."
    
    $output = & $exePath --task $i --wallet 15 2>&1
    
    $status = "Unknown"
    $message = ""
    
    # Check for panic
    if ($output -match "thread 'main' panicked") {
        $status = "CRASH"
        $message = "Panicked"
        Write-Host "Task $i CRASHED" -ForegroundColor Red
    } elseif ($output -match "Task Error") {
        # This is a handled error (e.g. logic failure)
        # But for our 'debug' purpose, if it's "Insufficient funds", it's a PASS (logic works, env fails)
        if ($output -match "Insufficient funds") {
            $status = "PASS (No Funds)"
        } else {
            $status = "FAIL"
        }
        $message = "Task Error"
    } elseif ($output -match "Success:") {
        $status = "PASS"
        $message = "Success"
    } elseif ($output -match "Failed:") {
        # Handled failure (e.g. logic returned success=false)
        if ($output -match "Insufficient funds") {
            $status = "PASS (No Funds)"
        } else {
            $status = "FAIL (Logic)"
        }
        $message = "Failed"
    } else {
        $status = "Review"
        $message = "Unexpected Output"
    }

    # Log details
    "--------------------------------------------------" | Out-File -FilePath $logFile -Append -Encoding utf8
    "TASK $i" | Out-File -FilePath $logFile -Append -Encoding utf8
    $output | Out-File -FilePath $logFile -Append -Encoding utf8
    
    # Update Summary
    "| $i | $status | $message |" | Out-File -FilePath $summaryFile -Append -Encoding utf8
}

Write-Host "Done. Check $summaryFile and $logFile"
