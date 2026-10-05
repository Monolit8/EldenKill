# Tests the ULTRAKILL half without Elden Ring: starts the fake host (known collision, three dummy
# enemies, a teleport), starts ULTRAKILL with the EldenKill plugin, and lets the fake host walk,
# turn and shoot through the input ring. Results: the fake host's PASS / FAIL lines, the overlay
# frame it dumps, and BepInEx\LogOutput.log.
#
#   .\tools\test-guest.ps1 [-Seconds 90] [-Ultrakill "C:\path\to\ULTRAKILL"]
param(
    [int]$Seconds = 90,
    [string]$Ultrakill = "$env:USERPROFILE\Desktop\ULTRAKILL",
    [string]$Out = "$PSScriptRoot\..\test-output"
)
# (Continue: Windows PowerShell 5.1 turns cargo's normal stderr output into errors; exit codes are checked)
$ErrorActionPreference = 'Continue'
$root = Split-Path $PSScriptRoot
$fake = "$root\host-eldenring\target\x86_64-pc-windows-msvc\release\fake-host.exe"
if (-not (Test-Path $fake)) {
    Push-Location "$root\host-eldenring"; cargo build --release --bin fake-host; Pop-Location
}
New-Item -ItemType Directory -Force $Out | Out-Null
Get-Process ULTRAKILL, fake-host -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep 1

$log = "$Out\fake-host.log"
$host_ = Start-Process $fake -ArgumentList "$Seconds", "--walk", "--dump", "`"$Out\overlay.bmp`"" -RedirectStandardOutput $log -RedirectStandardError "$Out\fake-host.err.log" -PassThru -NoNewWindow
$uk = Start-Process "$Ultrakill\ULTRAKILL.exe" -WorkingDirectory $Ultrakill -ArgumentList "-screen-fullscreen", "0", "-screen-width", "1280", "-screen-height", "720" -PassThru
Write-Host "fake host pid $($host_.Id), ULTRAKILL pid $($uk.Id); running $Seconds s"
$host_.WaitForExit()
Stop-Process -Id $uk.Id -Force -ErrorAction SilentlyContinue

Copy-Item "$Ultrakill\BepInEx\LogOutput.log" "$Out\bepinex.log" -Force
Write-Host "`n--- fake host results ---"
Select-String -Path $log -Pattern "TEST|driving at|dumped|hit actor|linked" | ForEach-Object { $_.Line }
Write-Host "`n--- EldenKill guest log ---"
Select-String -Path "$Out\bepinex.log" -Pattern "EldenKill\]|Exception" | Select-Object -Last 20 | ForEach-Object { $_.Line }
if (Test-Path "$Out\overlay.bmp") {
    Add-Type -AssemblyName System.Drawing
    $img = [System.Drawing.Image]::FromFile((Resolve-Path "$Out\overlay.bmp"))
    $img.Save("$Out\overlay.png", [System.Drawing.Imaging.ImageFormat]::Png); $img.Dispose()
    Write-Host "`noverlay frame: $Out\overlay.png"
}
