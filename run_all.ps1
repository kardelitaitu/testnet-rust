$ErrorActionPreference = "Continue"
$failed = @()
for ($i = 1; $i -le 38; $i++) {
    Write-Host "Running Task $i..."
    $output = cargo run --bin debug_task -- --task $i --wallet 15 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Task $i FAILED (Exit Code)" -ForegroundColor Red
        $failed += $i
    } elseif ($output -match "Task Error") {
        Write-Host "Task $i FAILED (Task Error)" -ForegroundColor Red
        $failed += $i
    } else {
        Write-Host "Task $i PASSED" -ForegroundColor Green
    }
}
Write-Host "Failed Tasks: $failed"
