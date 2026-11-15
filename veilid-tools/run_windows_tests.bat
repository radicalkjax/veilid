@echo off
set RUST_LOG=#common=debug
cargo nextest run test --features=tracing
if errorlevel 1 exit /b %errorlevel%
cargo nextest run test --no-default-features --features=rt-async-std,tracing
if errorlevel 1 exit /b %errorlevel%
cargo nextest run test
if errorlevel 1 exit /b %errorlevel%
cargo nextest run test --no-default-features --features=rt-async-std
if errorlevel 1 exit /b %errorlevel%
cargo test --doc
if errorlevel 1 exit /b %errorlevel%
