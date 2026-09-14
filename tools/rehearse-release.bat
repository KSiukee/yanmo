@echo off
rem Rehearse the CI release steps locally, on a runner-like console.
rem
rem   tools\rehearse-release.bat
rem
rem Why: the GitHub Windows runner is a clean *Western* machine.
rem   - default code page 1252 (cp1252): Python printing Chinese dies with UnicodeEncodeError;
rem   - core.autocrlf=true: sources are checked out with CRLF, not LF;
rem   - it has none of the internal check scripts, so tests fall back to the public pair.
rem None of that shows up on a local machine. This script sets the code page to 1252 for the
rem release-notes step, so that class of difference surfaces *before* a tag burns a cloud run.
rem
rem Run it when: you changed anything under tools\, changed .github\workflows, or moved machines.
rem For step 2 alone: "chcp 1252" then "python tools\release_notes.py release-notes.md".
rem
rem NOTE: keep this file ASCII-only. cmd reads .bat in the machine's OEM code page, so UTF-8
rem non-ASCII comments make it mis-parse lines (seen twice in practice). Chinese explanation
rem belongs in RELEASING.md.
setlocal
set "PY=%YANMO_PY%"
if "%PY%"=="" set "PY=python"
cd /d "%~dp0.."

echo [rehearse 1/2] build (same command as CI; it switches itself to UTF-8)
call "%~dp0build-release.bat"
if errorlevel 1 exit /b 1

echo.
echo [rehearse 2/2] release notes (CI runs this step WITHOUT changing the code page)
rem Runner default code page is 1252. Keep this line.
for /f "tokens=2 delims=:" %%c in ('chcp') do set "BACK=%%c"
chcp 1252 >nul
"%PY%" "%~dp0release_notes.py" release-notes.md
set "CODE=%ERRORLEVEL%"
if not "%BACK%"=="" chcp %BACK% >nul
if not "%CODE%"=="0" exit /b 1

echo.
echo rehearse OK: the CI steps pass here too.
exit /b 0
