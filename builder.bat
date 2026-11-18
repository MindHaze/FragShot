@echo off
setlocal EnableExtensions DisableDelayedExpansion

rem Always build from this script's directory, including paths with spaces.
pushd "%~dp0" >nul 2>&1
if errorlevel 1 (
    echo ERROR: Cannot open the project directory.
    exit /b 1
)
set "PROJECT_DIR=%CD%"

if not exist "Cargo.toml" (
    echo ERROR: Cargo.toml is missing next to builder.bat.
    goto :failed
)
if exist "fragshot.exe\" (
    echo ERROR: The root fragshot.exe path is a directory, not a file.
    goto :failed
)

rem Rustup's default location may not yet be on this terminal's PATH.
where cargo >nul 2>&1
if errorlevel 1 (
    if exist "%USERPROFILE%\.cargo\bin\cargo.exe" (
        set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
    )
)
where cargo >nul 2>&1
if errorlevel 1 (
    echo ERROR: Cargo not found. Install Rust from https://rustup.rs and try again.
    goto :failed
)
where rustc >nul 2>&1
if errorlevel 1 (
    echo ERROR: rustc not found. Install a Rust toolchain with rustup.
    goto :failed
)

rem Use the native host explicitly so cross-compilation settings cannot copy a stale build.
set "HOST_TARGET="
for /f "tokens=2" %%H in ('rustc -vV 2^>nul ^| findstr /b "host:"') do set "HOST_TARGET=%%H"
if not defined HOST_TARGET (
    echo ERROR: Cannot detect the native Rust target. Check rustc -vV.
    goto :failed
)

echo [1/2] Building the native release for %HOST_TARGET%...
call cargo build --release --locked --target "%HOST_TARGET%" --target-dir "%PROJECT_DIR%\target"
if errorlevel 1 (
    echo ERROR: Build failed. The previous root executable was not replaced.
    goto :failed
)

set "RELEASE_EXE=%PROJECT_DIR%\target\%HOST_TARGET%\release\fragshot.exe"
if not exist "%RELEASE_EXE%" (
    echo ERROR: Cargo finished without creating the expected executable.
    goto :failed
)

echo [2/2] Copying fragshot.exe to the project root...
copy /b /y "%RELEASE_EXE%" "%PROJECT_DIR%\fragshot.exe" >nul
if errorlevel 1 (
    echo ERROR: Copy failed. Close any running root fragshot.exe and check folder permissions.
    goto :failed
)

echo.
echo Build complete: "%PROJECT_DIR%\fragshot.exe"
echo Try: fragshot.exe demo --show-unavailable
popd
exit /b 0

:failed
popd
exit /b 1
