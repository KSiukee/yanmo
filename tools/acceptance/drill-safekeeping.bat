@echo off
chcp 65001 >nul
setlocal
rem yanmo "no data loss" drill (Windows entry).
rem
rem   drill-safekeeping.bat
rem
rem What it does, in a sandbox folder of its own (never your real library):
rem   1. copies yanmo.exe into %TEMP%\yanmo-drill and turns it into a portable copy
rem   2. seeds a small library there (50 chapters) and records the numbers
rem   3. opens the app on that library and asks you to type a few words
rem   4. KILLS that sandbox copy hard (like a power cut) - only the copy, never
rem      another yanmo you may have open
rem   5. opens it again and prints the numbers: is your text still there?
rem
rem Reports go next to this script. Keep this file ASCII-only (cmd reads .bat in the
rem OEM code page; non-ASCII text here turns into garbage).

set "HERE=%~dp0"
set "SANDBOX=%TEMP%\yanmo-drill"
set "REPORT=%HERE%drill-report"
set "EXE="
if exist "%HERE%yanmo.exe" set "EXE=%HERE%yanmo.exe"
if "%EXE%"=="" if exist "%LOCALAPPDATA%\研墨\yanmo.exe" set "EXE=%LOCALAPPDATA%\研墨\yanmo.exe"
if "%EXE%"=="" for %%I in (yanmo.exe) do if not "%%~$PATH:I"=="" set "EXE=%%~$PATH:I"
if "%EXE%"=="" (
  echo Cannot find yanmo.exe. Put this script next to it, or install yanmo first.
  pause
  exit /b 2
)

echo ================================================
echo    yanmo drill: does the draft survive a hard kill?
echo ================================================
echo program : %EXE%
echo sandbox : %SANDBOX%   (your real library is not touched)
echo.

rem fresh sandbox: a portable copy (data lives next to the program, inside the sandbox)
if exist "%SANDBOX%" rmdir /s /q "%SANDBOX%"
mkdir "%SANDBOX%"
copy /y "%EXE%" "%SANDBOX%\yanmo.exe" >nul
echo portable> "%SANDBOX%\yanmo-portable.txt"

echo [1/5] seeding a small library ...
"%SANDBOX%\yanmo.exe" --self-test-bench --dir "%SANDBOX%\data" --report "%REPORT%-before" --chapters 50 --chars 300
if errorlevel 1 goto :failed
echo       before:
"%SANDBOX%\yanmo.exe" --check "%SANDBOX%\data"

echo.
echo [2/5] opening the app on that library ...
echo       A NEW yanmo window opens (it uses the sandbox library, not yours).
echo       Type a few words in it, then come back here and press a key.
start "" "%SANDBOX%\yanmo.exe"
pause

echo [3/5] killing it hard (no clean shutdown, like a power cut) ...
powershell -NoProfile -Command "Get-CimInstance Win32_Process -Filter \"Name='yanmo.exe'\" | Where-Object { $_.ExecutablePath -like '*yanmo-drill*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }"
timeout /t 2 /nobreak >nul

echo [4/5] checking the library as it is now ...
echo       after:
"%SANDBOX%\yanmo.exe" --check "%SANDBOX%\data"

echo.
echo [5/5] opening it again - your text should still be there ...
start "" "%SANDBOX%\yanmo.exe"
pause
powershell -NoProfile -Command "Get-CimInstance Win32_Process -Filter \"Name='yanmo.exe'\" | Where-Object { $_.ExecutablePath -like '*yanmo-drill*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }"

echo.
echo done. Compare the two "before/after" lines above: chapters and words must be
echo the same (the text you typed is added on top, it must never go down).
echo Reports: %REPORT%*.json / *.md
echo.
pause
exit /b 0

:failed
echo seeding failed - see the report above.
pause
exit /b 1
