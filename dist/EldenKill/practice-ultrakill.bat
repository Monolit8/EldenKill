@echo off
rem Plain ULTRAKILL for learning its mechanics: full screen, straight into the Sandbox,
rem every weapon for the session (your save isn't changed). Never connects to Elden Ring,
rem even while EldenKill is running.
set UK=%USERPROFILE%\Desktop\ULTRAKILL
rem ULTRAKILL remembers its window position; the hidden copy EldenKill runs is parked far
rem off-screen, so the window is put back on the screen first
set KEY=HKCU\Software\Hakita\ULTRAKILL
reg add "%KEY%" /v "Screenmanager Window Position X_h4088080503" /t REG_DWORD /d 0 /f >nul
reg add "%KEY%" /v "Screenmanager Window Position Y_h4088080502" /t REG_DWORD /d 0 /f >nul
cd /d "%UK%"
start "" "%UK%\ULTRAKILL.exe" -ultrakill-practice -window-mode borderless -screen-fullscreen 1 -screen-width 3440 -screen-height 1440
