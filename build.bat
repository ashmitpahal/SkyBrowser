@echo off
REM SkyBrowser gate: Rust driver tests, exe. (Driver-only, no browser page.)
setlocal
echo [1/2] Rust driver tests...
rustc --edition=2021 --test main.rs -o skytest.exe
if errorlevel 1 exit /b 1
skytest.exe
if errorlevel 1 exit /b 1
echo [2/2] Building SkyBrowser.exe...
rustc --edition=2021 -O main.rs -o SkyBrowser.exe
if errorlevel 1 exit /b 1
echo Gate green. Double-click SkyBrowser.exe (or the Admin launcher).
