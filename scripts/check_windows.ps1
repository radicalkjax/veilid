# Local Windows acceptance gate: format, lint, unit tests, and the mock veilid-python and
# veilid-flutter integration tests, in that order. The Windows counterpart of check_all.sh for
# what has a Windows angle; wasm builds and cross-target clippy are left to that script and CI.
# The first failing step ends the run with that step's exit code.
#
# Usage: pwsh scripts\check_windows.ps1 [-SkipPython] [-SkipFlutter]
param([switch]$SkipPython, [switch]$SkipFlutter, [switch]$Help)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir

if ($Help) {
    Write-Host @"
Usage: pwsh scripts\check_windows.ps1 [-SkipPython] [-SkipFlutter]
  -SkipPython    skip the veilid-python tests against a mock-api veilid-server
  -SkipFlutter   skip dart format, flutter analyze and the headless mock integration tests
"@
    exit 0
}

function Fail([string]$Message, [int]$Code = 1) {
    Write-Error "check_windows: $Message" -ErrorAction Continue
    exit $Code
}

# Run a native command, announce it, and propagate its exit code on failure
function Step([string]$Name, [scriptblock]$Body) {
    Write-Host "==> $Name"
    & $Body
    if ($LASTEXITCODE -ne 0) {
        Fail "FAILED (exit $LASTEXITCODE): $Name" $LASTEXITCODE
    }
}

function Read-Versions([string]$Path) {
    $versions = @{}
    foreach ($line in Get-Content $Path) {
        if ($line -match '^\s*([A-Z_][A-Z0-9_]*)="([^"]*)"') {
            $versions[$Matches[1]] = $Matches[2]
        }
    }
    return $versions
}

function Require-Tool([string]$Tool, [string]$Hint) {
    if (-not (Get-Command $Tool -ErrorAction SilentlyContinue)) {
        Fail "$Tool is not installed. $Hint"
    }
}

Set-Location $RepoRoot
$Versions = Read-Versions (Join-Path $RepoRoot '.dagger\versions.env')
$RustVersion = $Versions['RUST_VERSION']
if (-not $RustVersion) { Fail 'could not read RUST_VERSION from .dagger/versions.env' }

# Pin cargo to the CI toolchain so lints match CI
Require-Tool rustup 'Run dev-setup\setup_windows.ps1 -Install.'
if (-not (rustup toolchain list | Select-String "^$RustVersion-")) {
    Fail "CI-pinned Rust toolchain $RustVersion is not installed. Install it with: rustup toolchain install $RustVersion --profile minimal -c clippy -c rustfmt"
}
$env:RUSTUP_TOOLCHAIN = $RustVersion
Write-Host "Using CI-pinned Rust toolchain $RustVersion."
$components = rustup component list --toolchain $RustVersion --installed
foreach ($cmp in 'clippy', 'rustfmt') {
    if (-not ($components | Select-String "^$cmp")) {
        Step "rustup component add $cmp" { rustup component add --toolchain $RustVersion $cmp }
    }
}
Require-Tool cargo-nextest 'Install it with: cargo install cargo-nextest --locked'

Write-Host 'Performing format check...'
Step 'cargo fmt --all --check' { cargo fmt --all --check }

if (-not $SkipFlutter) {
    Require-Tool flutter 'Run dev-setup\setup_windows.ps1 -Install -Flutter.'
    Require-Tool dart 'Run dev-setup\setup_windows.ps1 -Install -Flutter.'
    # Same file set as scripts/_check_format_dart.sh: veilid-flutter sources minus build output and generated code
    $dartVersion = $Versions['DART_VERSION']
    if (-not $dartVersion) { Fail 'could not read DART_VERSION from .dagger/versions.env' }
    $dartLanguageVersion = ($dartVersion -split '\.')[0..1] -join '.'
    $dartFiles = Get-ChildItem -Path (Join-Path $RepoRoot 'veilid-flutter') -Recurse -Filter '*.dart' -File |
        Where-Object {
            $rel = $_.FullName.Substring($RepoRoot.Length)
            ($rel -notmatch '\\(build|\.dart_tool|\.symlinks|ephemeral)\\') -and ($_.Name -notmatch '\.(freezed|g)\.dart$')
        } | ForEach-Object { $_.FullName } | Sort-Object
    if (-not $dartFiles) { Fail 'no Dart sources found to format-check' }
    Write-Host 'Performing Dart format check...'
    Step 'dart format --set-exit-if-changed' {
        dart format --language-version=$dartLanguageVersion --output=none --set-exit-if-changed -- @dartFiles
    }
}

Write-Host 'Running clippy...'
Step 'cargo clippy (host)' { cargo clippy --locked --all-targets }
Step 'cargo clippy (veilid-core mock-api)' {
    cargo clippy --locked --all-targets -p veilid-core --no-default-features --features=default-tokio,mock-api
}
Step 'cargo clippy (veilid-server mock-api)' {
    cargo clippy --locked --all-targets -p veilid-server --no-default-features --features=mock-api
}

Write-Host 'Running unit tests...'
# The per-crate Windows runners exercise both executors; they include the tests that reach the network
foreach ($crate in 'veilid-tools', 'veilid-core') {
    Step "$crate\run_windows_tests.bat" {
        Push-Location (Join-Path $RepoRoot $crate)
        try { cmd /c run_windows_tests.bat } finally { Pop-Location }
    }
}
Step 'cargo nextest (remaining crates)' {
    cargo nextest run --tests --all-targets --locked --features=debug-locks -p veilid-server -p veilid-cli -p veilid-core-optimized -p veilid-remote-api
}
# The DHT and transaction suites run only against the mock
Step 'cargo nextest (veilid-core mock-api)' {
    cargo nextest run --tests --all-targets --locked -p veilid-core --no-default-features --features=default-tokio,debug-locks,mock-api
}

if (-not $SkipPython) {
    Require-Tool uv 'Install it from https://docs.astral.sh/uv/getting-started/installation/'
    Write-Host 'Running veilid-python tests against a mock-api veilid-server...'
    Step 'cargo build veilid-server (mock-api)' {
        cargo build --locked -p veilid-server --no-default-features --features=mock-api
    }
    # The python client finds the server through the default named pipe
    $pipeName = 'veilid-server\0'
    if ([System.IO.Directory]::GetFiles('\\.\pipe\') -contains "\\.\pipe\$pipeName") {
        Fail 'a veilid-server is already listening on the default pipe; stop it first'
    }
    $server = Start-Process -FilePath (Join-Path $RepoRoot 'target\debug\veilid-server.exe') -PassThru -NoNewWindow
    try {
        $deadline = (Get-Date).AddSeconds(60)
        while (-not ([System.IO.Directory]::GetFiles('\\.\pipe\') -contains "\\.\pipe\$pipeName")) {
            if ($server.HasExited) { Fail "veilid-server exited before listening (exit $($server.ExitCode))" }
            if ((Get-Date) -gt $deadline) { Fail 'veilid-server did not open its pipe within 60s' }
            Start-Sleep -Milliseconds 500
        }
        Step 'uv run pytest' {
            Push-Location (Join-Path $RepoRoot 'veilid-python')
            try { uv run pytest -v } finally { Pop-Location }
        }
    } finally {
        if (-not $server.HasExited) {
            Stop-Process -Id $server.Id -Force
            $server.WaitForExit()
        }
    }
}

if (-not $SkipFlutter) {
    Write-Host 'Running flutter analyze...'
    # One pub workspace: resolving veilid-flutter covers every member, as scripts/_lint_all.sh does
    Step "flutter pub get veilid-flutter" {
        Push-Location (Join-Path $RepoRoot 'veilid-flutter')
        try { flutter pub get } finally { Pop-Location }
    }
    Step "flutter analyze veilid-flutter" {
        Push-Location (Join-Path $RepoRoot 'veilid-flutter')
        try { flutter analyze } finally { Pop-Location }
    }

    Write-Host 'Running headless mock veilid-flutter integration tests...'
    # Same feature selection as scripts/_integration_test_common.sh select_veilid_build: the build
    # hook reads it from the environment, and mock-api replaces the default feature set
    $featureVars = 'CARGO_VEILID_FLUTTER_FEATURES', 'CCACHE_VEILID_FLUTTER_FEATURES'
    $noDefaultVars = 'CARGO_VEILID_FLUTTER_NO_DEFAULT_FEATURES', 'CCACHE_VEILID_FLUTTER_NO_DEFAULT_FEATURES'
    foreach ($v in $featureVars) { Set-Item "env:$v" 'mock-api' }
    foreach ($v in $noDefaultVars) { Set-Item "env:$v" '1' }
    try {
        Step 'flutter test integration_test/app_test.dart (flutter-tester, mock)' {
            Push-Location (Join-Path $RepoRoot 'veilid-flutter\example')
            try {
                flutter test -r github -d flutter-tester integration_test/app_test.dart `
                    --dart-define=LOG_DIRECTIVES='#common=debug' --dart-define=VEILID_MOCK=true
            } finally { Pop-Location }
        }
    } finally {
        foreach ($v in $featureVars + $noDefaultVars) { Remove-Item "env:$v" -ErrorAction SilentlyContinue }
    }
}

Write-Host 'check_windows: all checks passed.'
