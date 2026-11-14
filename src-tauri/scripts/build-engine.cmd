@echo off
rem Builds HotSwap's mod engine (src/bin/hsengine.rs) and puts it where tauri.conf.json's
rem externalBin expects it, so it's bundled as hotswap-engine.exe next to hotswap.exe.
rem Run by tauri.conf.json before "cargo tauri dev" (debug) and "cargo tauri build" (release).
setlocal
cd /d "%~dp0.."
if "%1"=="release" (
  cargo build --release --bin hsengine || exit /b 1
  set "SRC=target\release\hsengine.exe"
) else (
  cargo build --bin hsengine || exit /b 1
  set "SRC=target\debug\hsengine.exe"
)
if not exist binaries mkdir binaries
copy /Y "%SRC%" "binaries\hotswap-engine-x86_64-pc-windows-msvc.exe" >nul || exit /b 1
