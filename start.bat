@echo off
rem Opens the headless server and the desktop app, each in its own window.
rem The server uses its own data folder so it does not lock the desktop database.
if /i "%~1"=="server" goto server
if /i "%~1"=="desktop" goto desktop

start "Hashlark Server" cmd /k ""%~f0" server"
start "Hashlark Desktop" cmd /k ""%~f0" desktop"
exit /b 0

:server
cd /d "%~dp0"
set "HASHLARK_DEFINITIONS_DIR=%~dp0definitions\hashlark-defs"
set "HASHLARK_DATA_DIR=%~dp0.target\hashlark-server-data"
set "HASHLARK_TOKEN=devtoken"
set "HASHLARK_BIND=127.0.0.1:18787"
set "HASHLARK_CORS_ORIGINS=http://localhost:5173"
cargo run -p hashlark-server
exit /b

:desktop
cd /d "%~dp0apps\desktop"
set "HASHLARK_DEFINITIONS_DIR=%~dp0definitions\hashlark-defs"
pnpm tauri dev
exit /b
