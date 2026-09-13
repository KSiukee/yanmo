@echo off
chcp 65001 >nul
setlocal
rem yanmo acceptance run (Windows entry): seed a big library, measure, write a report.
rem
rem   run-acceptance.bat                     default: 1000 chapters x 3000 chars (~3M chars)
rem   run-acceptance.bat --chapters 300      smaller run
rem   run-acceptance.bat --chars 1000        shorter chapters
rem
rem It finds yanmo.exe in this order: next to this script -> anywhere under %LOCALAPPDATA% -> PATH.
rem The report is written next to this script. Nothing here touches your real library:
rem the acceptance run uses its own folder under %TEMP%.
rem
rem NOTE FOR EDITORS: keep this file ASCII-only and do NOT use caret-escaped parentheses.
rem cmd.exe reads .bat in the OEM code page (non-ASCII text turns into garbage) and a caret
rem escape inside an if-block desyncs the parser - the whole script then runs as garbage.

set "REPORT=%~dp0acceptance-report"
set "WORK=%TEMP%\yanmo-acceptance"
set "EXE="

if exist "%~dp0yanmo.exe" set "EXE=%~dp0yanmo.exe"
if "%EXE%"=="" for /d %%D in ("%LOCALAPPDATA%\*") do if exist "%%~fD\yanmo.exe" set "EXE=%%~fD\yanmo.exe"
if "%EXE%"=="" for %%I in (yanmo.exe) do if not "%%~$PATH:I"=="" set "EXE=%%~$PATH:I"

if "%EXE%"=="" (
  echo.
  echo Cannot find yanmo.exe.
  echo Put this script next to yanmo.exe, or install yanmo first.
  echo.
  pause
  exit /b 2
)

rem Version gate: the acceptance mode exists from 0.28.0. An older build would ignore
rem --self-test-* and just open a window, leaving this script waiting forever - so read the
rem FILE VERSION instead of running it. Nothing is launched, so an old build cannot hang us.
powershell -NoProfile -Command "if ([version](Get-Item '%EXE%').VersionInfo.FileVersion -lt [version]'0.28.0') { exit 3 }"
if errorlevel 1 (
  echo.
  echo This yanmo.exe is too old - it has no acceptance mode.
  echo Please install yanmo 0.28.0 or newer, or put this script next to that yanmo.exe.
  echo.
  pause
  exit /b 3
)
set "VERSIONFILE=%TEMP%\yanmo-acceptance-version.txt"
powershell -NoProfile -Command "(Get-Item '%EXE%').VersionInfo.FileVersion" > "%VERSIONFILE%" 2>nul
set /P VERSION=<"%VERSIONFILE%"

echo ================================================
echo    yanmo acceptance
echo ================================================
echo version : %VERSION%
echo program : %EXE%
echo work dir: %WORK%
echo report  : %REPORT%-VERSION-DATE
echo.

echo [1/2] core benchmark: seed + read/search/write/reopen/backup ...
rem headless: this step does not open a window
"%EXE%" --self-test-bench --dir "%WORK%" --report "%REPORT%" %*
if errorlevel 1 (
  echo core benchmark FAILED.
  pause
  exit /b 1
)

echo [2/2] UI cold start: a window opens and closes by itself ...
"%EXE%" --self-test-ui --dir "%WORK%" --report "%REPORT%"
if errorlevel 1 echo UI step did not finish cleanly - see the report note.

echo.
echo done. open the .md file next to this script to read the numbers.
echo.
pause
