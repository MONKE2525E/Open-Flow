param(
  [int]$Loops = 1,
  [switch]$IncludeSmoke
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

function Run-Step {
  param(
    [string]$Name,
    [scriptblock]$Command
  )

  Write-Host ""
  Write-Host "== $Name =="
  $global:LASTEXITCODE = 0
  & $Command
  if ($global:LASTEXITCODE -ne 0) {
    throw "$Name failed with exit code $global:LASTEXITCODE"
  }
}

for ($i = 1; $i -le $Loops; $i++) {
  Write-Host ""
  Write-Host "==== Auto-learn harness loop $i of $Loops ===="

  Run-Step "regression matrix" {
    cargo test --manifest-path src-tauri/Cargo.toml auto_learn_regression_matrix -- --nocapture
  }

  Run-Step "auto-learn unit tests" {
    cargo test --manifest-path src-tauri/Cargo.toml api::auto_learn::tests -- --nocapture
  }

  Run-Step "full rust tests" {
    cargo test --manifest-path src-tauri/Cargo.toml
  }

  Run-Step "lint" {
    npm run lint
  }

  if ($IncludeSmoke) {
    $startedServer = $false
    $serverProcess = $null
    $listener = Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1

    if (-not $listener) {
      $out = Join-Path $repo "vite-auto-learn-harness.out.log"
      $err = Join-Path $repo "vite-auto-learn-harness.err.log"
      $serverProcess = Start-Process -FilePath npm.cmd -ArgumentList @("run", "dev", "--", "--host", "127.0.0.1") -WorkingDirectory $repo -RedirectStandardOutput $out -RedirectStandardError $err -WindowStyle Hidden -PassThru
      $startedServer = $true
      Start-Sleep -Seconds 4
    }

    try {
      Run-Step "frontend smoke" {
        npm run test:smoke
      }
      Run-Step "frontend state smoke" {
        npm run test:smoke:state
      }
    } finally {
      if ($startedServer -and $serverProcess) {
        Stop-Process -Id $serverProcess.Id -ErrorAction SilentlyContinue
      }
    }
  }
}

Write-Host ""
Write-Host "Auto-learn harness passed."
