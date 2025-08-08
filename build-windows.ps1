# FastResize Windows Build Script
# This script builds FastResize on Windows

Write-Host "FastResize Windows Build Script" -ForegroundColor Cyan
Write-Host "================================" -ForegroundColor Cyan

# Check if Rust is installed
if (!(Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "Rust is not installed. Please install from https://rustup.rs/" -ForegroundColor Red
    exit 1
}

Write-Host "`nRust version:" -ForegroundColor Yellow
rustc --version
cargo --version

# Build the project
Write-Host "`nBuilding FastResize (Release mode)..." -ForegroundColor Green
cargo build --release

if ($LASTEXITCODE -eq 0) {
    Write-Host "`nBuild successful!" -ForegroundColor Green
    Write-Host "Executable location: target\release\fastresize.exe" -ForegroundColor Cyan
    
    # Show file info
    $exe = "target\release\fastresize.exe"
    if (Test-Path $exe) {
        $fileInfo = Get-Item $exe
        Write-Host "`nFile info:" -ForegroundColor Yellow
        Write-Host "  Size: $([math]::Round($fileInfo.Length / 1MB, 2)) MB"
        Write-Host "  Created: $($fileInfo.CreationTime)"
        
        # Test the executable
        Write-Host "`nTesting executable..." -ForegroundColor Yellow
        & $exe --version
    }
} else {
    Write-Host "`nBuild failed. Please check the error messages above." -ForegroundColor Red
    exit 1
}

# Optional: Add to PATH
Write-Host "`nWould you like to add FastResize to your PATH? (y/n)" -ForegroundColor Cyan
$response = Read-Host
if ($response -eq 'y') {
    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $targetPath = (Get-Item "target\release").FullName
    if ($currentPath -notlike "*$targetPath*") {
        [Environment]::SetEnvironmentVariable("Path", "$currentPath;$targetPath", "User")
        Write-Host "Added to PATH. Please restart your terminal." -ForegroundColor Green
    } else {
        Write-Host "Already in PATH." -ForegroundColor Yellow
    }
}

Write-Host "`nDone!" -ForegroundColor Green