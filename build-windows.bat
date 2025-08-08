@echo off
REM FastResize Windows Build Script (Batch version)
REM This script builds FastResize on Windows

echo FastResize Windows Build Script
echo ================================

REM Check if Rust is installed
where cargo >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo Rust is not installed. Please install from https://rustup.rs/
    pause
    exit /b 1
)

echo.
echo Rust version:
rustc --version
cargo --version

REM Build the project
echo.
echo Building FastResize (Release mode)...
cargo build --release

if %ERRORLEVEL% EQU 0 (
    echo.
    echo Build successful!
    echo Executable location: target\release\fastresize.exe
    
    REM Test the executable
    echo.
    echo Testing executable...
    target\release\fastresize.exe --version
) else (
    echo.
    echo Build failed. Please check the error messages above.
    pause
    exit /b 1
)

echo.
echo Done!
pause