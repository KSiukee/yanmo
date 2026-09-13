@echo off
chcp 65001 >nul
setlocal
rem yanmo one-command release build (Windows entry).
rem
rem   tools\build-release.bat               self-check -> tests -> bundle -> collect + SHA256
rem   tools\build-release.bat --no-bundle   executable only (no installer)
rem   tools\build-release.bat --skip-tests  skip tests (local repeated builds only)
rem
rem Which python: YANMO_PY env var -> python on PATH.
set "PY=%YANMO_PY%"
if "%PY%"=="" set "PY=python"

"%PY%" "%~dp0build_release.py" %*
set "FAIL=%ERRORLEVEL%"
echo.
if not "%FAIL%"=="0" echo Build FAILED ^(exit %FAIL%^)
if "%FAIL%"=="0" echo Build OK
exit /b %FAIL%
