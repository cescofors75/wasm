@echo off
setlocal

cd /d "%~dp0"

powershell -NoProfile -ExecutionPolicy Bypass -File ".\build.ps1" -Serve -Port 8080
exit /b %ERRORLEVEL%
