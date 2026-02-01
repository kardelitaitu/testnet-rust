
$walletId = 15
$taskRange = 1..59
$results = @()
$logDir = "test_logs"
if (!(Test-Path $logDir)) { New-Item -ItemType Directory -Path $logDir }

$binary = "target\debug\debug_task.exe"

foreach ($taskId in $taskRange) {
    Write-Host "--- Running Task $taskId ---" -ForegroundColor Cyan
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    
    $startTime = Get-Date
    
    # Start the process and capture it for resource usage
    $process = Start-Process -FilePath $binary -ArgumentList "--task $taskId", "--wallet $walletId" -NoNewWindow -PassThru -Wait -RedirectStandardOutput "$logDir\task_$taskId.log" -RedirectStandardError "$logDir\task_$taskId.err"
    
    $endTime = Get-Date
    $duration = ($endTime - $startTime).TotalSeconds
    
    $exitCode = $process.ExitCode
    
    # Resource usage (Approximation)
    # Using dummy values if process is already gone, but PassThru captures some stats
    # Note: Stats are captured after Wait, so they represent end state or peak if available
    $peakMemory = $process.PeakWorkingSet64 / 1MB
    $cpuTime = $process.TotalProcessorTime.TotalSeconds
    
    $output = Get-Content "$logDir\task_$taskId.log" -Raw
    $errors = Get-Content "$logDir\task_$taskId.err" -Raw
    
    $result = [PSCustomObject]@{
        task_number = $taskId
        timestamp   = $timestamp
        exit_code   = $exitCode
        duration_s  = $duration
        memory_mb   = $peakMemory
        cpu_time_s  = $cpuTime
        success     = ($exitCode -eq 0)
    }
    
    $results += $result
    
    if ($exitCode -eq 0) {
        Write-Host "Task $taskId Success ($($duration)s)" -ForegroundColor Green
    }
    else {
        Write-Host "Task $taskId Failed ($($duration)s)" -ForegroundColor Red
    }
}

$results | ConvertTo-Json | Out-File "test_results_raw.json"
Write-Host "Testing complete. Raw results saved to test_results_raw.json" -ForegroundColor Magenta
