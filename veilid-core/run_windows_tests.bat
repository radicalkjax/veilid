@echo off
set RUST_LOG=#common=debug
cargo nextest run
if errorlevel 1 exit /b %errorlevel%
cargo nextest run --no-default-features --features=default-async-std 
if errorlevel 1 exit /b %errorlevel%
