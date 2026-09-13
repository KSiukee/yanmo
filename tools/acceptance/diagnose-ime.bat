@echo off
chcp 65001 >nul
setlocal
rem yanmo startup diagnose (Windows entry): find out why the Chinese IME does not attach.
rem
rem   diagnose-ime.bat
rem
rem It records a timeline of: program start -> window shown -> window activated ->
rem UI ready -> what the page sees about focus and IME composition. That timeline is
rem the only way to tell "the window never got activated" from "the IME never attached
rem to the text area".
rem
rem The result is a plain text log next to this script. It never touches your library.
rem
rem NOTE FOR EDITORS: keep this file ASCII-only and do NOT use caret-escaped parentheses
rem (cmd reads .bat in the OEM code page; a caret escape inside an if-block desyncs parsing).

set "HERE=%~dp0"
set "LOG=%HERE%yanmo-diagnose.log"
set "EXE="

if exist "%HERE%yanmo.exe" set "EXE=%HERE%yanmo.exe"
if "%EXE%"=="" for /d %%D in ("%LOCALAPPDATA%\*") do if exist "%%~fD\yanmo.exe" set "EXE=%%~fD\yanmo.exe"
if "%EXE%"=="" for %%I in (yanmo.exe) do if not "%%~$PATH:I"=="" set "EXE=%%~$PATH:I"

if "%EXE%"=="" (
  echo Cannot find yanmo.exe. Put this script next to it, or install yanmo first.
  pause
  exit /b 2
)

rem Version gate: --diagnose exists from 0.29.0. Read the FILE VERSION, never run an old build.
powershell -NoProfile -Command "if ([version](Get-Item '%EXE%').VersionInfo.FileVersion -lt [version]'0.29.0') { exit 3 }"
if errorlevel 1 (
  echo This yanmo.exe is too old - it has no --diagnose mode.
  echo Please install yanmo 0.29.0 or newer.
  pause
  exit /b 3
)

rem Fresh log, with the machine facts that decide most of these cases.
> "%LOG%" echo === yanmo startup diagnose ===
>>"%LOG%" echo windows:
ver >>"%LOG%" 2>&1
>>"%LOG%" echo webview2 runtime:
reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv >>"%LOG%" 2>&1
reg query "HKCU\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv >>"%LOG%" 2>&1
>>"%LOG%" echo --- timeline ---

echo ================================================
echo    yanmo startup diagnose
echo ================================================
echo program : %EXE%
echo log     : %LOG%
echo.
echo A yanmo window will open. Please do ALL THREE, in this order:
echo   1. just type pinyin / press the IME hotkey - do NOT click first
echo   2. click inside the text once, then type again
echo   3. switch to English, type a few letters, switch back to Chinese, type again
echo Then close the window with its X button - the log is written as you go.
echo.
pause

"%EXE%" --diagnose --out "%LOG%"

echo.
echo done. This is what was recorded:
echo ------------------------------------------------------------
type "%LOG%"
echo ------------------------------------------------------------
echo.
echo Send that text back (or the file itself).
echo.
pause
