@echo off
setlocal

set "VSSDIR=%LOCALAPPDATA%\VSS"
set "LIVE_EXE=C:\ProgramData\Ableton\Live 12 Standard\Program\Ableton Live 12 Standard.exe"
set "LIVE_PROC=Ableton Live 12 Standard.exe"

if not exist "%VSSDIR%" mkdir "%VSSDIR%"

set "NIH_LOG=%VSSDIR%\vss-live.log"
echo NIH_LOG=%NIH_LOG%

tasklist /FI "IMAGENAME eq %LIVE_PROC%" /FO CSV /NH 2>NUL | find /I "%LIVE_PROC%" >NUL
if not errorlevel 1 (
    echo Close Live first: NIH_LOG only applies to a fresh launch
    exit /b 1
)

if not exist "%LIVE_EXE%" (
    echo ERROR: Ableton Live 12 Standard.exe not found at:
    echo   %LIVE_EXE%
    echo Check the install path under "C:\ProgramData\Ableton\Live 12 Standard\Program\" and update this script.
    exit /b 1
)

if defined VSS_DRY_RUN (
    echo VSS_DRY_RUN set: would start "%LIVE_EXE%"
    exit /b 0
)

start "" "%LIVE_EXE%"

endlocal
