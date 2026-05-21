param(
    [ValidateSet("auto", "wsl-podman", "host", "rancher", "docker-wsl", "none")]
    [string]$Runtime = "auto",
    [string]$Root = "",
    [switch]$BuildAgentImage,
    [int]$Cpus = 2,
    [int]$MemoryMb = 4096,
    [int]$DiskGb = 30
)

$ErrorActionPreference = "Stop"

function Ensure-WingetPackage {
    param(
        [string]$Id,
        [string]$Name
    )

    Write-Host "Checking $Name..."
    winget list --id $Id --exact | Out-Null
    if ($LASTEXITCODE -eq 0) {
        Write-Host "$Name is already installed."
        return
    }

    Write-Host "Installing $Name..."
    winget install --id $Id --exact --source winget --accept-package-agreements --accept-source-agreements --silent
}

Ensure-WingetPackage -Id "Rustlang.Rustup" -Name "Rustup"
Ensure-WingetPackage -Id "MSYS2.MSYS2" -Name "MSYS2"

$env:PATH = "C:\msys64\ucrt64\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"
rustup toolchain install stable-x86_64-pc-windows-gnu
C:\msys64\usr\bin\bash.exe -lc "pacman --noconfirm -Sy mingw-w64-ucrt-x86_64-gcc mingw-w64-ucrt-x86_64-pkgconf"

if ($Runtime -eq "auto" -or $Runtime -eq "wsl-podman" -or $Runtime -eq "host") {
    Ensure-WingetPackage -Id "RedHat.Podman" -Name "Podman"
    $env:PATH = "$env:ProgramFiles\RedHat\Podman;C:\msys64\ucrt64\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"

    $machines = podman machine list --format json | ConvertFrom-Json
    if (-not $machines -or $machines.Count -eq 0) {
        podman machine init --cpus $Cpus --memory $MemoryMb --disk-size $DiskGb
    }

    podman machine start
}
elseif ($Runtime -eq "rancher") {
    Ensure-WingetPackage -Id "SUSE.RancherDesktop" -Name "Rancher Desktop"
    Write-Host "Open Rancher Desktop and select the moby/dockerd engine for Docker API compatibility."
}
else {
    Write-Host "docker-wsl mode is intentionally not automated yet because it modifies a Linux distribution."
    Write-Host "Use the docs/bootstrap-windows.md guide for the manual WSL Docker Engine path."
}

cargo +stable-x86_64-pc-windows-gnu build

$dist = Join-Path $PSScriptRoot "..\dist\windows-x64"
New-Item -ItemType Directory -Force -Path $dist | Out-Null
Copy-Item ".\target\debug\librarian.exe" (Join-Path $dist "librarian.exe") -Force
Copy-Item ".\scripts\librarian-launcher.ps1" (Join-Path $dist "librarian.ps1") -Force

$setupRuntime = switch ($Runtime) {
    "auto" { "auto" }
    "wsl-podman" { "wsl-podman" }
    "host" { "host" }
    "none" { "none" }
    default { "host" }
}

$setupArgs = @("run", "--", "setup", "--yes", "--runtime", $setupRuntime)
if ($Root -ne "") {
    $setupArgs += @("--root", $Root)
}
if ($BuildAgentImage) {
    $setupArgs += "--build-agent-image"
}
cargo +stable-x86_64-pc-windows-gnu @setupArgs

Write-Host "Bootstrap complete."
Write-Host "Repo UI: cargo +stable-x86_64-pc-windows-gnu run -- admin"
Write-Host "Built launcher: $dist\librarian.ps1 admin"
