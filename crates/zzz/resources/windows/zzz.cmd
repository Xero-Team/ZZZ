@echo off
setlocal

set "SCRIPT_DIR=%~dp0"
set "CLI_EXE=%SCRIPT_DIR%zzz.exe"
if exist "%CLI_EXE%" goto run_cli

set "APP_EXE=%SCRIPT_DIR%..\ZZZ.exe"
if exist "%APP_EXE%" goto run_app

set "APP_EXE=%SCRIPT_DIR%..\zzz.exe"
if exist "%APP_EXE%" goto run_app

echo zzz.cmd: could not find ZZZ executable next to this script. 1>&2
echo Looked for: 1>&2
echo   %CLI_EXE% 1>&2
echo   %SCRIPT_DIR%..\ZZZ.exe 1>&2
echo   %SCRIPT_DIR%..\zzz.exe 1>&2
exit /b 1

:run_cli
"%CLI_EXE%" %*
exit /b %ERRORLEVEL%

:run_app
"%APP_EXE%" %*
exit /b %ERRORLEVEL%
