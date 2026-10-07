@echo off
setlocal

PUSHD %~dp0
SET SCRIPTDIR=%CD%
POPD
PUSHD %SCRIPTDIR%\..

FOR %%X IN (cargo-docs-rs.exe) DO (SET CARGO_DOCS_RS_FOUND=%%~$PATH:X)
IF NOT DEFINED CARGO_DOCS_RS_FOUND (
    echo cargo-docs-rs is not installed. Please install it with: cargo install cargo-docs-rs --locked 
    goto end
)

set TOOLCHAIN=%1
if "%TOOLCHAIN%"=="" (
    set TOOLCHAIN=nightly
)

cargo test --doc
cargo +%TOOLCHAIN% docs-rs -p veilid-core
cargo +%TOOLCHAIN% docs-rs -p veilid-tools
cargo +%TOOLCHAIN% docs-rs -p veilid-remote-api

POPD

:end

endlocal
