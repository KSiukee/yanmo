@echo off
chcp 65001 >nul
setlocal
rem yanmo acceptance run (Windows entry): seed a big library, measure, write a report.
rem
rem   run-acceptance.bat                     default: 1000 chapters x 3000 chars (~3M chars)
rem   run-acceptance.bat --chapters 300      smaller run
rem   run-acceptance.bat --chars 1000        shorter chapters
rem
rem It finds yanmo.exe in this order: next to this script -> %LOCALAPPDATA%\研墨 -> PATH.
rem The report is written next to this script. Nothing here touches your real library:
rem the acceptance run uses its own folder under %TEMP%.
rem
rem NOTE: keep this file ASCII-only. cmd.exe reads .bat in the OEM code page, so
rem non-ASCII text here turns into garbage on a non-UTF8 console.

set "REPORT=%~dp0acceptance-report"
set "WORK=%TEMP%\yanmo-acceptance"
set "EXE="

if exist "%~dp0yanmo.exe" set "EXE=%~dp0yanmo.exe"
if "%EXE%"=="" if exist "%LOCALAPPDATA%\研墨\yanmo.exe" set "EXE=%LOCALAPPDATA%\研墨\yanmo.exe"
if "%EXE%"=="" for %%I in (yanmo.exe) do if not "%%~$PATH:I"=="" set "EXE=%%~$PATH:I"

if "%EXE%"=="" (
  echo.
  echo Cannot find yanmo.exe.
  echo Put this script next to yanmo.exe, or install yanmo first.
  echo.
  pause
  exit /b 2
)

echo ================================================
echo    yanmo acceptance
echo ================================================
echo program : %EXE%
echo work dir: %WORK%
echo report  : %REPORT%-^<version^>-^<date^>
echo.

echo [1/2] core benchmark (seed + read/search/write/reopen/backup) ...
rem headless: this step does not open a window
"%EXE%" --self-test-bench --dir "%WORK%" --report "%REPORT%" %*
if errorlevel 1 (
  echo core benchmark FAILED.
  pause
  exit /b 1
)

echo [2/2] UI cold start (a window opens and closes by itself) ...
"%EXE%" --self-test-ui --dir "%WORK%" --report "%REPORT%"
if errorlevel 1 echo UI step did not finish cleanly - see the report note.

echo.
echo done. open the .md file next to this script to read the numbers.
echo.
pause
