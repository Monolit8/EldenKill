@echo off
rem Starts Elden Ring offline (Easy Anti-Cheat off) with EldenKill, on its own save file
rem (EldenKill.sl2), so your normal save is never touched. Needs me3: https://me3.help
rem --disable-arxan: EldenKill patches the frame rate limit and ultrawide in memory
chcp 65001 >nul
cd /d "%~dp0"
set ME3=%LOCALAPPDATA%\Programs\garyttierney\me3\bin\me3.exe
if not exist "%ME3%" set ME3=me3.exe
"%ME3%" launch --game eldenring --profile "%~dp0eldenkill.me3" --savefile EldenKill.sl2 --disable-arxan %*
