@echo off
setlocal

set "VSSDIR=%LOCALAPPDATA%\VSS"
set "LIVE_EXE=C:\ProgramData\Ableton\Live 12 Standard\Program\Ableton Live 12 Standard.exe"

if not exist "%VSSDIR%" mkdir "%VSSDIR%"

set "NIH_LOG=%VSSDIR%\vss-live.log"
echo NIH_LOG=%NIH_LOG%

if not exist "%LIVE_EXE%" (
    echo ERROR: Ableton Live 12 Standard.exe not found at:
    echo   %LIVE_EXE%
    echo Check the install path under "C:\ProgramData\Ableton\Live 12 Standard\Program\" and update this script.
    exit /b 1
)

start "" "%LIVE_EXE%"

endlocal
