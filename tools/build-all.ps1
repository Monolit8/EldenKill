# Builds both halves and refreshes dist\EldenKill (what a player installs):
#   dist\EldenKill\eldenkill.dll            Elden Ring half (me3 loads it)
#   dist\EldenKill\ULTRAKILL-plugin\        ULTRAKILL half (copy into ULTRAKILL\BepInEx\plugins\EldenKill)
# The ULTRAKILL build also copies its DLL straight into your ULTRAKILL's BepInEx\plugins\EldenKill.
param([string]$Ultrakill = "")
# (Continue: Windows PowerShell 5.1 turns cargo's normal stderr output into errors; exit codes are checked)
$ErrorActionPreference = 'Continue'
$root = Split-Path $PSScriptRoot

Push-Location "$root\host-eldenring"
try { cargo build --release; if ($LASTEXITCODE -ne 0) { throw "cargo build failed" } } finally { Pop-Location }

Push-Location "$root\guest-ultrakill"
try {
    $args_ = @("build", "-c", "Release")
    if ($Ultrakill) { $args_ += "-p:GameDir=$Ultrakill" }
    dotnet @args_
    if ($LASTEXITCODE -ne 0) { throw "dotnet build failed" }
} finally { Pop-Location }

$dist = "$root\dist\EldenKill"
New-Item -ItemType Directory -Force "$dist\ULTRAKILL-plugin" | Out-Null
# swap by rename: a running Elden Ring keeps the old DLL loaded (er-mario's build.ps1 trick)
$dll = "$root\host-eldenring\target\x86_64-pc-windows-msvc\release\eldenkill.dll"
Get-ChildItem "$dist\eldenkill.dll.old*" -ErrorAction SilentlyContinue | ForEach-Object { try { Remove-Item $_ -Force -ErrorAction Stop } catch {} }
if (Test-Path "$dist\eldenkill.dll") { Move-Item "$dist\eldenkill.dll" "$dist\eldenkill.dll.old$(Get-Date -Format HHmmss)" -Force }
Copy-Item $dll "$dist\eldenkill.dll"
Copy-Item "$root\guest-ultrakill\bin\Release\netstandard2.1\EldenKill.Guest.dll" "$dist\ULTRAKILL-plugin\" -Force
Write-Host "dist ready: $dist"
