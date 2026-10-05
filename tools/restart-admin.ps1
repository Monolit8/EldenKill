# Closes Elden Ring + ULTRAKILL even when they run as administrator (needs to be run elevated):
#   Start-Process powershell -Verb RunAs -ArgumentList '-ExecutionPolicy Bypass -File tools\restart-admin.ps1'
# Start EldenKill again normally afterwards (launch-eldenkill.bat): started as administrator,
# Elden Ring hung before opening its window.
Get-Process eldenring, ULTRAKILL -ErrorAction SilentlyContinue | Stop-Process -Force
Get-Process eldenring, ULTRAKILL -ErrorAction SilentlyContinue | Wait-Process -Timeout 20
