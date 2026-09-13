@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion
rem yanmo startup / IME diagnose (Windows entry).
rem
rem   diagnose-ime.bat
rem
rem It records: program start -> window shown -> window activated -> UI ready ->
rem what the page sees about focus and IME composition. Plus the WebView2 runtime
rem versions installed on this machine (the IME attachment differs between them).
rem
rem If more than one WebView2 runtime is installed, it ALSO runs yanmo once against
rem the older one (WEBVIEW2_BROWSER_EXECUTABLE_FOLDER) so we can tell whether the
rem problem is the runtime or our code. Nothing here touches your library.
rem
rem NOTE FOR EDITORS: keep this file ASCII-only and do NOT use caret-escaped parentheses.

set "HERE=%~dp0"
set "LOG1=%HERE%yanmo-diagnose.log"
set "LOG2=%HERE%yanmo-diagnose-oldruntime.log"
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

> "%LOG1%" echo === yanmo startup diagnose ===
>>"%LOG1%" echo windows:
ver >>"%LOG1%" 2>&1
>>"%LOG1%" echo webview2 runtime ^(registry^):
reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv >>"%LOG1%" 2>&1
>>"%LOG1%" echo webview2 runtime folders ^(newest first^):
dir /b /o-n "%LOCALAPPDATA%\Microsoft\EdgeWebView\Application" >>"%LOG1%" 2>&1
>>"%LOG1%" echo --- timeline ---

echo ================================================
echo    yanmo diagnose - step 1 of 2
echo ================================================
echo program : %EXE%
echo log     : %LOG1%
echo.
echo A yanmo window opens. Please do ALL THREE, in this order:
echo   1. just press the IME hotkey and type pinyin - do NOT click first
echo   2. click inside the text once, then type again
echo   3. switch to English, type a few letters, switch back to Chinese, type again
echo Then close the window with its X button.
echo.
pause
"%EXE%" --diagnose --out "%LOG1%"

rem Second run against the OLDEST other runtime, if there is one.
set "OLD="
for /f "skip=1 delims=" %%V in ('dir /b /o-n "%LOCALAPPDATA%\Microsoft\EdgeWebView\Application" 2^>nul') do set "OLD=%LOCALAPPDATA%\Microsoft\EdgeWebView\Application\%%V"

if "%OLD%"=="" (
  echo.
  echo Only one WebView2 runtime is installed - skipping the second run.
  goto :report
)
if not exist "%OLD%\msedgewebview2.exe" (
  echo.
  echo No usable older WebView2 runtime - skipping the second run.
  goto :report
)

> "%LOG2%" echo === yanmo startup diagnose ^(older WebView2 runtime^) ===
>>"%LOG2%" echo runtime folder: %OLD%
>>"%LOG2%" echo --- timeline ---

echo.
echo ================================================
echo    yanmo diagnose - step 2 of 2
echo ================================================
echo Now it runs again against a DIFFERENT WebView2 runtime:
echo   %OLD%
echo Same three steps again in the window that opens.
echo.
pause
set "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER=%OLD%"
"%EXE%" --diagnose --out "%LOG2%"
set "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER="

:report
echo.
echo ------------------------------------------------------------
echo LOG 1 (current runtime):
type "%LOG1%"
if exist "%LOG2%" (
  echo ------------------------------------------------------------
  echo LOG 2 (older runtime):
  type "%LOG2%"
)
echo ------------------------------------------------------------
echo Send both logs back (or the files themselves).
echo.
pause
