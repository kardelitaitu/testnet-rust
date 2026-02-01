#!/usr/bin/env pwsh
#
# Build script for testnet-framework
# Usage: .\scripts\build.ps1 [-Mode <debug|release|clean>] [-Benchmark] [-Check]
#

param(
    [Parameter(Position = 0)]
    [ValidateSet("debug", "release", "clean")]
    [string]$Mode = "debug",

    [switch]$Benchmark,
    [switch]$Check,
    [switch]$AllTargets
)

$ErrorActionPreference = "Stop"

function Test-CargoInstalled {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Error "Cargo/Rust not found. Please install from https://rustup.rs/"
        return $false
    }
    return $true
}

function Test-Dependencies {
    $missing = @()
    if (-not (Test-Path "$PSScriptRoot\..\Cargo.toml")) {
        $missing.Add("Cargo.toml")
    }
    if ($missing.Count -gt 0) {
        Write-Error "Missing required files: $($missing -join ', ')"
        return $false
    }
    return $true
}

function New-Directory {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        New-Item -ItemType Directory -Force -Path $Path | Out-Null
    }
}

function Invoke-Build {
    param(
        [string]$BuildMode,
        [switch]$RunBenchmarks,
        [switch]$CheckOnly
    )

    $projectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
    Push-Location $projectRoot

    try {
        $cargoArgs = @("build")
        if ($CheckOnly) {
            $cargoArgs += "--check"
        }
        if ($BuildMode -eq "release") {
            $cargoArgs += "--release"
        }
        if ($RunBenchmarks) {
            $cargoArgs += "--benches"
        }
        if ($AllTargets) {
            $cargoArgs += "--all-targets"
        } else {
            $cargoArgs += "--workspace"
        }

        Write-Host "Running: cargo $($cargoArgs -join ' ')" -ForegroundColor Cyan
        $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()

        $process = Start-Process -FilePath "cargo" -ArgumentList $cargoArgs -NoNewWindow -PassThru -Wait

        $stopwatch.Stop()

        if ($process.ExitCode -eq 0) {
            $timeStr = if ($stopwatch.Elapsed.TotalSeconds -gt 60) {
                "{0:m}m {1:s}s" -f $stopwatch.Elapsed, $stopwatch.Elapsed
            } else {
                "{0:N1}s" -f $stopwatch.Elapsed.TotalSeconds
            }
            Write-Host "Build completed successfully in $timeStr" -ForegroundColor Green
        } else {
            Write-Error "Build failed with exit code $($process.ExitCode)"
        }
    }
    finally {
        Pop-Location
    }
}

function Invoke-Clean {
    $projectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
    Write-Host "Cleaning build artifacts..." -ForegroundColor Yellow
    cargo clean --manifest-path "$projectRoot\Cargo.toml"
    Write-Host "Clean completed" -ForegroundColor Green
}

function New-BuildInfo {
    $projectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $gitHash = git rev-parse --short HEAD 2>$null | Out-String -NoNewline

    $buildInfo = @{
        Timestamp = $timestamp
        GitHash = $gitHash.Trim()
        Mode = $Mode
    }

    $buildInfo | ConvertTo-Json | Out-File -FilePath "$projectRoot\build_info.json" -Encoding utf8
    Write-Host "Build info written to build_info.json" -ForegroundColor Gray
}

# Main execution
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  testnet-framework Build Script" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

if (-not (Test-CargoInstalled)) { exit 1 }
if (-not (Test-Dependencies)) { exit 1 }

switch ($Mode) {
    "clean" {
        Invoke-Clean
    }
    "release" {
        Invoke-Build -BuildMode "release" -RunBenchmarks:$Benchmark -CheckOnly:$Check
        New-BuildInfo
    }
    "debug" {
        Invoke-Build -BuildMode "debug" -RunBenchmarks:$Benchmark -CheckOnly:$Check
    }
}

Write-Host ""
Write-Host "Build complete!" -ForegroundColor Green
