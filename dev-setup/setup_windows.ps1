# veilid Windows dev-setup — one idempotent script (PowerShell). Checks every
# dependency; -Install ensures minimums, -Upgrade/-Update also pulls newest. Same
# dependency set as setup_macos.sh / setup_linux.sh (no Flutter). Versions come from
# the single source of truth, .dagger/versions.env.
#
# Usage:  pwsh dev-setup\setup_windows.ps1 [-Install | -Upgrade]
# Standalone: no dependency on veilidchat. Supersedes setup_windows.bat.
# Note: after a winget install, a tool may not be on PATH until a new shell — re-run then.
param([switch]$Install, [switch]$Upgrade, [switch]$Update, [switch]$Help,
      [ValidateSet('x64','arm64')][string]$TargetArch, [switch]$BuildOnly, [switch]$Flutter,
      [string]$Prefix)
$ErrorActionPreference = "Stop"
$ProgressPreference = 'SilentlyContinue'   # Invoke-WebRequest progress bar is 10-50x slower over SSH/qcow2
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

if ($Help -or $args -contains '/?' -or $args -contains '--help' -or $args -contains '-h') {
    Write-Host @"
veilid Windows dev-setup — checks development dependencies; installs/upgrades them.

Usage: pwsh setup_windows.ps1 [-Install | -Upgrade] [-Help]
  (default)            check each dependency and report what is missing or out of date
  -Install             install missing deps + bring below-minimum deps up to spec
                       (leaves deps that already meet the requirement untouched)
  -Upgrade, -Update    also upgrade already-satisfied deps to the newest versions
  -TargetArch <x64|arm64>  arch to provision rust/JDK for (default: host). x64-on-arm64
                       installs the cross rust toolchain + an x64 JDK and sets JAVA_HOME.
  -BuildOnly           only build-critical deps (rust+target, cmake, MSVC, +JDK when
                       cross); skips dev/test tools (cargo-nextest, llvm, node, nightly, ...)
  -Flutter             also install Flutter (for veilid-flutter work); only if missing or older than
                       veilid-flutter requires — never downgrades a newer one
  -Prefix <dir>        contained toolchain root (CI cache / flutter-packer): rust, cmake, JDK and
                       flutter install under it instead of LOCALAPPDATA/USERPROFILE. MSVC, git and
                       pwsh are system installers and stay system-wide.
  -Help, /?            show this help
"@
    exit 0
}
$script:DoUpgrade = ($Upgrade -or $Update)
$script:DoInstall = ($Install -or $script:DoUpgrade)

# Target arch + build scope (for CI/cross builds driven by flutter-packer). BuildOnly trims
# to the deps needed to *build* veilid; Cross = building for a non-host CPU arch.
$hostArch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
if (-not $TargetArch) { $TargetArch = $hostArch }
$script:Cross = ($TargetArch -ne $hostArch)
$msvcTriple   = if ($TargetArch -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }

# ---- versions: single source of truth (.dagger/versions.env, KEY="value") ----
$V = @{}
Get-Content (Join-Path $ScriptDir "..\.dagger\versions.env") | ForEach-Object {
    if ($_ -match '^\s*([A-Z_][A-Z0-9_]*)="?([^"#]*?)"?\s*(#.*)?$') { $V[$Matches[1]] = $Matches[2] }
}

# Prune docs/man/locales from a PortableGit tree — unused in builds, thousands of tiny files
function SlimGitTree($root) {
    foreach ($d in "mingw64\share\doc","mingw64\share\man","mingw64\share\locale","usr\share\doc","usr\share\man","usr\share\locale") {
        Remove-Item (Join-Path $root $d) -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# -Prefix: contained toolchain root. CARGO_HOME/RUSTUP_HOME steer rustup-init; $ToolRoot
# replaces LOCALAPPDATA for the veilid-* tool dirs.
if ($Prefix) {
    $Prefix = (New-Item -ItemType Directory -Force -Path $Prefix).FullName
    $env:CARGO_HOME  = Join-Path $Prefix "cargo"
    $env:RUSTUP_HOME = Join-Path $Prefix "rustup"
    $ToolRoot = $Prefix
    # Slim caches populated before minimal profile was pinned (docs = ~49k files)
    Get-ChildItem (Join-Path $env:RUSTUP_HOME "toolchains") -Directory -ErrorAction SilentlyContinue |
        ForEach-Object { Remove-Item (Join-Path $_.FullName "share\doc") -Recurse -Force -ErrorAction SilentlyContinue }
    SlimGitTree (Join-Path $Prefix "veilid-git")
} else { $ToolRoot = $env:LOCALAPPDATA }

# ---- check/report engine ---------------------------------------------------
$script:ok = 0; $script:miss = 0; $script:old = 0; $script:conflict = 0
function VerGE([string]$a, [string]$b) {
    function norm($s) { if ($s -match '(\d+(\.\d+)*)') { return $Matches[1] } return "" }  # bare int or dotted
    $a = norm $a; $b = norm $b; if (-not $a) { return $false }
    try { return [version]$a -ge [version]$b } catch { return ($a -eq $b) }
}
function Say($tag, $name, $detail) { Write-Host ("  {0,-7} {1,-22} {2}" -f $tag, $name, $detail) }
function DepMin($name, $got, $req, $install) {
    if (-not $got) { $script:miss++; Say "[MISS]" $name "not found (need >= $req)"; if ($script:DoInstall -and $install) { RunStep $install } }
    elseif (VerGE $got $req) { $script:ok++; Say "[ok]" $name "$got (>= $req)"; if ($script:DoUpgrade -and $install) { RunStep $install } }
    else { $script:old++; Say "[old]" $name "$got (need >= $req)"; if ($script:DoInstall -and $install) { RunStep $install } }
}
function DepExact($name, $got, $req, $sbs, $install) {
    if (-not $got) { $script:miss++; Say "[MISS]" $name "not found (need == $req)"; if ($script:DoInstall -and $install) { RunStep $install } }
    elseif ($got -eq $req) { $script:ok++; Say "[ok]" $name "$got (== $req)" }
    elseif ($sbs) { $script:old++; Say "[diff]" $name "$got, want $req"; if ($script:DoInstall -and $install) { RunStep $install } }
    elseif (VerGE $got $req) { $script:conflict++; Say "[!!]" $name "$got > required $req - uninstall manually, then re-run" }
    else { $script:old++; Say "[old]" $name "$got (need == $req)"; if ($script:DoInstall -and $install) { RunStep $install } }
}
function DepHave($name, $got, $hint, $install) {
    if ($got) { $script:ok++; Say "[ok]" $name "present"; if ($script:DoUpgrade -and $install) { RunStep $install } }
    else { $script:miss++; Say "[MISS]" $name $hint; if ($script:DoInstall -and $install) { RunStep $install } }
}
function CmdVer($exe, $arg) { try { return (& $exe $arg 2>&1 | Out-String) } catch { return "" } }
function Has($exe) { return [bool](Get-Command $exe -ErrorAction SilentlyContinue) }
function CrateVer($crate) {
    if (-not (Has "cargo")) { return "" }
    $line = (cargo install --list 2>$null | Select-String "^$crate v([0-9.]+):")
    if ($line) { return $line.Matches[0].Groups[1].Value } else { return "" }
}
# Run an install/upgrade scriptblock tolerantly — a failure (e.g. a tool not yet on
# PATH after a winget install) is reported, not fatal, so the run always completes.
function RunStep($step) { if ($step) { try { & $step } catch { Write-Warning ("  install step failed: " + $_.Exception.Message) } } }

# ---- install helpers (winget for packaged tools; downloads for the rest) -----
# Upgrade-safe: winget upgrades if present; downloads no-op when already satisfied.
$CargoBin = if ($env:CARGO_HOME) { Join-Path $env:CARGO_HOME "bin" } else { Join-Path $env:USERPROFILE ".cargo\bin" }
# NB: call winget.exe explicitly — a function named "Winget"/"winget" would shadow
# the executable (PowerShell command resolution is case-insensitive) and self-recurse.
function WinPkg($id) {
    if (-not (Get-Command winget.exe -ErrorAction SilentlyContinue)) { Write-Warning "winget not available; install $id manually"; return }
    if (winget.exe list --id $id -e 2>$null | Select-String ([regex]::Escape($id))) {
        winget.exe upgrade --id $id -e --accept-source-agreements --accept-package-agreements
    } else {
        winget.exe install --id $id -e --accept-source-agreements --accept-package-agreements
    }
}
function DlExtract($url, $pattern) {
    $tmp = Join-Path $env:TEMP ("veilid-dl-" + [System.Guid]::NewGuid())
    New-Item -ItemType Directory -Path $tmp -Force | Out-Null
    $file = Join-Path $tmp (Split-Path $url -Leaf)
    Invoke-WebRequest -Uri $url -OutFile $file
    if ($file -match '\.zip$') { Expand-Archive -Path $file -DestinationPath $tmp -Force } else { tar -xf $file -C $tmp }
    New-Item -ItemType Directory -Path $CargoBin -Force | Out-Null
    Get-ChildItem -Path $tmp -Recurse -Filter $pattern | ForEach-Object { Copy-Item $_.FullName $CargoBin -Force }
    Remove-Item $tmp -Recurse -Force
}
# rustup via the official rustup-init.exe (NOT winget — its catalog needs a network source-index
# update that corrupts to 0x8a15000f on flaky links). Adds .cargo\bin to the session PATH so the
# cross-toolchain step below runs in the SAME pass (no "re-run in a fresh shell"); rustup-init also
# persists PATH for future shells.
$iRust   = {
    # Prefix installs pin minimal profile: rust-docs alone is ~49k files, dwarfing
    # everything else in the CI cache archive. Covers rustup update, cargo-shim
    # on-demand, and cross-toolchain installs. Personal rustups are left alone.
    if ($Prefix -and (Has "rustup")) { try { rustup set profile minimal } catch {} }
    if (Has "rustup") { rustup update; return }
    $rsArch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'aarch64' } else { 'x86_64' }
    $ri = Join-Path $env:TEMP "rustup-init.exe"
    Invoke-WebRequest -Uri "https://static.rust-lang.org/rustup/dist/$rsArch-pc-windows-msvc/rustup-init.exe" -OutFile $ri
    & $ri -y --default-toolchain $V.RUST_VERSION --profile minimal
    $cb = $CargoBin
    if ((Test-Path $cb) -and ($env:PATH -notmatch [regex]::Escape($cb))) { $env:PATH = "$cb;$env:PATH" }
}
# cmake — install the pinned CMAKE_VERSION_PATCH (single source of truth, same as the Earthly/Dagger
# Linux build + ANDROID_CMAKE_VERSION) for the TARGET arch, direct from cmake.org. NOT VS's bundled
# cmake: that's the host arch (arm64 on an ARM64 box), so an x64-on-arm64 build targets ARM64 and the
# VCTargets probe fails ("BaseOutputPath not set"). A TargetArch cmake under Prism targets x64.
$CmakeRoot = Join-Path $ToolRoot "veilid-cmake-$TargetArch"
$iCmake  = {
    $a = if ($TargetArch -eq 'arm64') { 'arm64' } else { 'x86_64' }
    $zip = Join-Path $env:TEMP "cmake-$($V.CMAKE_VERSION_PATCH)-windows-$a.zip"
    Invoke-WebRequest -Uri "https://cmake.org/files/v$($V.CMAKE_VERSION_MINOR)/cmake-$($V.CMAKE_VERSION_PATCH)-windows-$a.zip" -OutFile $zip
    if (Test-Path $CmakeRoot) { Remove-Item $CmakeRoot -Recurse -Force }
    New-Item -ItemType Directory -Force $CmakeRoot | Out-Null
    tar -xf $zip -C $CmakeRoot
    $b = (Get-ChildItem $CmakeRoot -Directory -Filter 'cmake-*' | Select-Object -First 1).FullName + "\bin"
    if (Test-Path "$b\cmake.exe") {
        $u = [Environment]::GetEnvironmentVariable("PATH","User")
        if ($u -notmatch [regex]::Escape($b)) { [Environment]::SetEnvironmentVariable("PATH","$b;$u","User") }
        if ($env:PATH -notmatch [regex]::Escape($b)) { $env:PATH = "$b;$env:PATH" }
    }
}
function CmakeExe { if (Test-Path $CmakeRoot) { $d = Get-ChildItem $CmakeRoot -Directory -Filter 'cmake-*' -ErrorAction SilentlyContinue | Select-Object -First 1; if ($d) { return (Join-Path $d.FullName 'bin\cmake.exe') } } return $null }
# VS 2022 Build Tools (MSVC) via the direct vs_buildtools.exe bootstrapper (NOT winget — unreliable on
# flaky links). The build PINS VS 2022: the SSOT cmake (CMAKE_VERSION_PATCH) only knows the VS 2022 /
# 2019 generators, not newer previews (e.g. VS 18). VC.Tools.x86.x64 is the x64 compiler/linker rust
# needs for *-pc-windows-msvc (incl. x64-on-arm64 under Prism); the Windows SDK gives the system libs.
# Machine-wide -> needs an elevated shell.
$iMsvc = {
    $exe = Join-Path $env:TEMP "vs_buildtools.exe"
    Invoke-WebRequest -Uri "https://aka.ms/vs/$($V.VS_VERSION)/release/vs_BuildTools.exe" -OutFile $exe
    & $exe --quiet --wait --norestart --nocache --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.Windows10SDK.$($V.WINDOWS_SDK_VERSION) | Out-Null
}
# The build needs git AND bash: CI runner images often ship MinGit (git.exe, no bash.exe),
# so presence of git alone must not satisfy the check. Under -Prefix (or without winget),
# install PortableGit into the prefix — self-contained, GitLab-cacheable; else winget full Git.
$PortableGitRoot = Join-Path $ToolRoot "veilid-git"
$iGit  = {
    if ($Prefix -or -not (Get-Command winget.exe -ErrorAction SilentlyContinue)) {
        $exe = Join-Path $env:TEMP $V.GIT_WINDOWS_FILE
        Invoke-WebRequest -Uri "https://github.com/git-for-windows/git/releases/download/$($V.GIT_WINDOWS_TAG)/$($V.GIT_WINDOWS_FILE)" -OutFile $exe
        # PortableGit is a 7-Zip SFX; -y -o extracts silently
        Start-Process -Wait -FilePath $exe -ArgumentList "-y", "-o`"$PortableGitRoot`""
        Remove-Item $exe -Force
        SlimGitTree $PortableGitRoot
    } else { WinPkg "Git.Git" }
}
# bash.exe lives in Git\bin (full Git / PortableGit), not Git\cmd — the Windows build
# (dist\windows\_build.ps1, flutter-packer) needs Git Bash. Put it on PATH (persisted).
function GitBashDir {
    $cands = @()
    $g = (Get-Command git -ErrorAction SilentlyContinue).Source
    if ($g) { $cands += (Join-Path (Split-Path (Split-Path $g)) "bin") }
    $cands += (Join-Path $PortableGitRoot "bin")
    $cands += "${env:ProgramFiles}\Git\bin"
    foreach ($d in $cands) { if (Test-Path (Join-Path $d "bash.exe")) { return $d } }
    return $null
}
function EnsureGitBash {
    $gb = GitBashDir
    if (-not $gb) { return }
    $u = [Environment]::GetEnvironmentVariable("PATH","User")
    if ($u -notmatch [regex]::Escape($gb)) { [Environment]::SetEnvironmentVariable("PATH","$u;$gb","User"); Write-Host "    added Git Bash to PATH: $gb" }
    if ($env:PATH -notmatch [regex]::Escape($gb)) { $env:PATH = "$env:PATH;$gb" }
}
# PowerShell 7 (pwsh) — the build runtime: dist\windows\_build.ps1 + .flutter-packer invoke `pwsh`,
# not Windows PowerShell 5.1 (whose native-command stderr handling mis-fires). Official MSI, direct
# from MS (not winget). The dev-setup scripts themselves stay 5.1-runnable so a bare box can bootstrap.
$iPwsh = {
    $dl = Join-Path $env:TEMP "install-powershell.ps1"
    Invoke-WebRequest -Uri "https://aka.ms/install-powershell.ps1" -OutFile $dl
    & $dl -UseMSI -Quiet
}
$iJava   = { WinPkg "Microsoft.OpenJDK.$($V.JAVA_VERSION)" }   # host-arch JDK (non-cross)
# Cross JDK: download the *target-arch* Microsoft OpenJDK 17 zip to a known dir and point
# JAVA_HOME at it. jni links the JDK's jvm.lib by arch, so a host arm64 jvm.lib fails the x64
# link (LNK4272). winget would give the host arch, hence the explicit arch-specific download.
$JdkRoot = Join-Path $ToolRoot "veilid-jdk-$TargetArch"
$iJavaCross = {
    $jdkArch = if ($TargetArch -eq 'arm64') { 'aarch64' } else { 'x64' }
    $zip = Join-Path $env:TEMP "msjdk$($V.JAVA_VERSION)-$jdkArch.zip"
    Invoke-WebRequest -Uri "https://aka.ms/download-jdk/microsoft-jdk-$($V.JAVA_VERSION)-windows-$jdkArch.zip" -OutFile $zip
    New-Item -ItemType Directory -Path $JdkRoot -Force | Out-Null
    Expand-Archive -Path $zip -DestinationPath $JdkRoot -Force; Remove-Item $zip -Force
    $jdk = Get-ChildItem $JdkRoot -Directory -Filter "jdk-$($V.JAVA_VERSION)*" | Select-Object -First 1
    if ($jdk) { [Environment]::SetEnvironmentVariable('JAVA_HOME', $jdk.FullName, 'Machine'); $env:JAVA_HOME = $jdk.FullName; Write-Host "    JAVA_HOME=$($jdk.FullName)" }
    else { throw "extracted JDK but found no jdk-$($V.JAVA_VERSION)* dir under $JdkRoot" }
}
$iLlvm   = { WinPkg "LLVM.LLVM" }
$iJq     = { WinPkg "jqlang.jq" }
$iPython = { WinPkg "Python.Python.$($V.PYTHON_VERSION)" }
$iNode   = { WinPkg "OpenJS.NodeJS" }
# Downloads — pinned version; skip if the installed one already satisfies (no downgrade on -Upgrade).
$iWopt   = { if ((Has "wasm-opt") -and (VerGE (CmdVer "wasm-opt" "--version") $V.BINARYEN_VERSION)) { return }
             DlExtract "https://github.com/WebAssembly/binaryen/releases/download/version_$($V.BINARYEN_VERSION)/binaryen-version_$($V.BINARYEN_VERSION)-x86_64-windows.tar.gz" "wasm-opt.exe" }
$iCapnp  = { if ((Has "capnp") -and (VerGE (CmdVer "capnp" "--version") $V.CAPNP_MIN_VERSION)) { return }
             DlExtract "https://capnproto.org/capnproto-c++-win32-$($V.CAPNP_MIN_VERSION).zip" "capnp*.exe" }
$iWbg    = { cargo install --locked wasm-bindgen-cli --version $V.WASM_BINDGEN_VERSION }
$iCedit  = { cargo install --locked cargo-edit --version $V.CARGO_EDIT_VERSION }

Write-Host "== veilid Windows dev dependencies ($TargetArch$(if ($BuildOnly) { ', build-only' })) =="

# C/C++ build tools (MSVC) — the Windows analog of build-essential / Xcode CLT. Assumed
# present in CI (flutter-packer bakes in the VS Build Tools); checked here either way.
# Detect a real VS/BuildTools instance with the x64 VC tools via vswhere — NOT a bare Test-Path on
# "Microsoft Visual Studio\", which the VS Installer creates even with no toolchain installed (false
# positive that would skip the install and break the rust link). Fall back to cl on PATH.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
# Require a VS 2022 (17.x) instance with the x64 VC tools (`-version "[17.0,18.0)"`) — the build pins
# VS 2022, so a different VS (e.g. a VS 18 preview) present must NOT count as satisfied, or the install
# is skipped and the SSOT cmake can't configure (it has no VS-18 generator).
$vsInst = if (Test-Path $vswhere) { (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -version "[$($V.VS_VERSION).0,$([int]$V.VS_VERSION + 1).0)" -property installationPath 2>$null) } else { "" }
$msvc = [bool]$vsInst
DepHave "MSVC build tools (VS 2022)" ($(if ($msvc) { "y" } else { "" })) "downloads VS 2022 BuildTools (vs_buildtools.exe: VC x64 + Win SDK)" $iMsvc
# Git (+ Git Bash) — build-critical: dist\windows\_build.ps1 runs bash for the dev-overrides check.
# The check requires BOTH: CI images ship MinGit (git.exe without bash.exe), which must not satisfy it.
DepHave "git (+bash)" ($(if ((Has "git") -and (GitBashDir)) { "y" } else { "" })) "full Git (PortableGit under -Prefix; winget Git.Git otherwise)" $iGit
if ($script:DoInstall) { EnsureGitBash }
# PowerShell 7 — the build runtime (the dev-setup scripts stay 5.1-runnable to bootstrap, but the
# build runs under pwsh; 5.1 mis-handles flutter's native stderr).
DepHave "pwsh (PowerShell 7)" ($(if (Has "pwsh") { "y" } else { "" })) "install-powershell.ps1 -UseMSI (build runtime; not 5.1)" $iPwsh

# Rust toolchain + cargo. Always build-critical.
DepMin   "rust"   (CmdVer "rustc" "--version") $V.RUST_MSRV $iRust
DepHave  "cargo"  ($(if (Has "cargo") { "y" } else { "" })) "installed with rustup" $iRust

# Cross toolchain: an x64-on-arm64 (Prism) build needs the target-arch toolchain installed
# AND default — host rust being present doesn't provide it, so this is its own step (not the
# rust DepMin above). --force-non-host on both: the host CPU can't run the cross rustc.
if ($script:Cross -and (Has "rustup")) {
    $tc = "$($V.RUST_VERSION)-$msvcTriple"
    if (rustup toolchain list 2>$null | Select-String ([regex]::Escape($tc))) {
        $script:ok++; Say "[ok]" "rust ($TargetArch)" "$tc present"; if ($script:DoUpgrade) { try { rustup update $tc } catch {} }
    } elseif ($script:DoInstall) {
        Say "[add]" "rust ($TargetArch)" "installing cross toolchain $tc"
        try {
            rustup toolchain install $tc --force-non-host --profile minimal
            rustup default $tc --force-non-host
            $script:ok++; Write-Host "    default: $((rustc --version) 2>$null)"
        } catch { $script:conflict++; Say "[!!]" "rust ($TargetArch)" "cross toolchain install failed" }
    } else { $script:old++; Say "[old]" "rust ($TargetArch)" "$tc missing (re-run with -Install)" }
}

# Dev/test cargo tools — NOT build deps; skipped under -BuildOnly. On Windows these pull
# aws-lc-sys -> NASM and fail; veilid's own build uses ring, so it needs none of them.
if (-not $BuildOnly) {
    DepExact "wasm-bindgen-cli" (CrateVer "wasm-bindgen-cli") $V.WASM_BINDGEN_VERSION $true $iWbg
    DepExact "cargo-edit"       (CrateVer "cargo-edit")       $V.CARGO_EDIT_VERSION   $true $iCedit
    DepHave  "cargo-nextest"    (CrateVer "cargo-nextest")    "cargo install --locked cargo-nextest"    { cargo install --locked cargo-nextest }
    DepHave  "cargo-public-api" (CrateVer "cargo-public-api") "cargo install --locked cargo-public-api" { cargo install --locked cargo-public-api }
}

# JDK for the jni/jvm link. Provisioned (download + JAVA_HOME, keyed off -TargetArch) whenever we
# build — cross OR -BuildOnly — its own block because the jni link needs the *target-arch* jvm.lib
# and a host-arch `java` passing the version check must not suppress it. Covers native-x64 CI
# (windows-latest): -BuildOnly there installs the x64 JDK rather than skipping it. Full dev mode
# (no -BuildOnly, same arch) uses the normal host JDK via the version check.
if ($script:Cross -or $BuildOnly) {
    $jdk = if (Test-Path $JdkRoot) { Get-ChildItem $JdkRoot -Directory -Filter "jdk-$($V.JAVA_VERSION)*" -ErrorAction SilentlyContinue | Select-Object -First 1 } else { $null }
    if ($jdk) {
        if ($env:JAVA_HOME -ne $jdk.FullName) { [Environment]::SetEnvironmentVariable('JAVA_HOME', $jdk.FullName, 'Machine'); $env:JAVA_HOME = $jdk.FullName }
        $script:ok++; Say "[ok]" "JDK ($TargetArch)" "JAVA_HOME=$($jdk.FullName)"
    } elseif ($script:DoInstall) {
        Say "[add]" "JDK ($TargetArch)" "downloading Microsoft OpenJDK 17 ($TargetArch)"
        try { & $iJavaCross; $script:ok++ } catch { $script:conflict++; Say "[!!]" "JDK ($TargetArch)" ("install failed: " + $_.Exception.Message) }
    } else { $script:old++; Say "[old]" "JDK ($TargetArch)" "need $TargetArch JDK + JAVA_HOME (re-run with -Install)" }
} else {
    DepMin "java" (CmdVer "java" "-version") $V.JAVA_MIN_VERSION $iJava
}
# capnp: veilid-core dev tool (regenerates checked-in capnp Rust), not a veilidchat build dep — skip
# under -BuildOnly. (cmake stays: Windows Flutter desktop genuinely builds with CMake + MSVC.)
if (-not $BuildOnly) { DepMin "capnp" (CmdVer "capnp" "--version") $V.CAPNP_MIN_VERSION $iCapnp }
# cmake: the build needs a TargetArch-matched cmake (see $iCmake) — check the managed install, not a
# host-PATH cmake (which may be the wrong arch from VS). Pinned to CMAKE_VERSION_PATCH (single source).
$cmExe = CmakeExe
DepHave "cmake ($TargetArch $($V.CMAKE_VERSION_PATCH))" ($(if ($cmExe -and (Test-Path $cmExe)) { "y" } else { "" })) "download cmake $($V.CMAKE_VERSION_PATCH) ($TargetArch) from cmake.org" $iCmake
if (-not $BuildOnly) {
    DepMin  "wasm-opt"    (CmdVer "wasm-opt" "--version") $V.BINARYEN_VERSION $iWopt
    DepHave "llvm/clang"  ($(if (Has "clang") { "y" } else { "" })) "winget install --id LLVM.LLVM"        $iLlvm
    DepHave "jq"          ($(if (Has "jq") { "y" } else { "" }))    "winget install --id jqlang.jq"        $iJq
    DepHave "python"      ($(if (Has "python") { "y" } else { "" })) "winget install --id Python.Python.$($V.PYTHON_VERSION)" $iPython
    DepHave "node"        ($(if (Has "node") { "y" } else { "" }))  "winget install --id OpenJS.NodeJS"    $iNode
    DepHave "npm"         ($(if (Has "npm") { "y" } else { "" }))   "comes with node"                      $null
}

# Rust target(s). -BuildOnly: just the target-arch MSVC target (cross is covered by the
# cross-toolchain step; the same-arch host toolchain already has it). Full dev mode: the
# shared set from versions.env (add only the missing — no per-target "is up to date" spam).
if (-not (Has "rustup")) {
    $script:miss++; Say "[MISS]" "rust targets" "rustup not found"
} elseif ($BuildOnly) {
    if (-not $script:Cross) {
        $haveT = rustup target list --installed 2>$null
        if ($haveT -contains $msvcTriple) { $script:ok++; Say "[ok]" "rust target" "$msvcTriple present" }
        elseif ($script:DoInstall) { Say "[add]" "rust target" "adding $msvcTriple"; try { rustup target add $msvcTriple; $script:ok++ } catch { $script:conflict++; Say "[!!]" "rust target" "add failed" } }
        else { $script:old++; Say "[old]" "rust target" "$msvcTriple missing (re-run with -Install)" }
    }
} else {
    $wantT = $V.RUST_STABLE_TARGETS -split '\s+' | Where-Object { $_ }
    $haveT = rustup target list --installed 2>$null
    $addT  = @($wantT | Where-Object { $haveT -notcontains $_ })
    if ($addT.Count -eq 0) { $script:ok++; Say "[ok]" "rust targets" "all $($wantT.Count) present" }
    elseif ($script:DoInstall) {
        Say "[add]" "rust targets" ("adding {0}: {1}" -f $addT.Count, ($addT -join ' '))
        try { rustup target add @addT; $script:ok++ } catch { $script:conflict++; Say "[!!]" "rust targets" "rustup target add failed" }
    } else { $script:old++; Say "[old]" "rust targets" "$($addT.Count) missing (re-run with -Install)" }
}
# nightly toolchain — dev/test only; skipped under -BuildOnly.
if ((-not $BuildOnly) -and $script:DoInstall -and (Has "rustup")) {
    try {
        if (-not (rustup toolchain list 2>$null | Select-String '^nightly')) { rustup install nightly --profile minimal }
        elseif ($script:DoUpgrade) { rustup update nightly }
    } catch { Write-Warning ("  rustup nightly step failed: " + $_.Exception.Message) }
}

# Optional Flutter SDK (-Flutter) — for veilid-flutter developers. Installs the pinned FLUTTER_VERSION
# when flutter is absent or below veilid-flutter's requirement (FLUTTER_MIN_VERSION); never downgrades.
if ($Flutter) {
    Write-Host "-- Flutter (veilid-flutter) --"
    $fdir = Join-Path $ToolRoot "veilid-flutter"
    $iFlutter = {
        $fv2 = $V.FLUTTER_VERSION
        $zip = Join-Path $env:TEMP "flutter_windows_$fv2-stable.zip"
        Write-Host "    installing Flutter $fv2 -> $fdir\flutter"
        Invoke-WebRequest -Uri "https://storage.googleapis.com/flutter_infra_release/releases/stable/windows/flutter_windows_$fv2-stable.zip" -OutFile $zip
        if (Test-Path $fdir) { Remove-Item $fdir -Recurse -Force }
        New-Item -ItemType Directory -Force $fdir | Out-Null
        tar -xf $zip -C $fdir
        $fbin = Join-Path $fdir "flutter\bin"
        if (Test-Path (Join-Path $fbin "flutter.bat")) {
            $u = [Environment]::GetEnvironmentVariable("PATH","User")
            if ($u -notmatch [regex]::Escape($fbin)) { [Environment]::SetEnvironmentVariable("PATH","$fbin;$u","User") }
            if ($env:PATH -notmatch [regex]::Escape($fbin)) { $env:PATH = "$fbin;$env:PATH" }
        } else { Write-Warning "  flutter extract failed" }
    }
    function FlutterVer {
        if (-not (Has "flutter")) { return "" }
        # first-run flutter writes "Building flutter tool..." to stderr; under
        # $ErrorActionPreference=Stop (CI) those records terminate the script.
        # Relax EAP for the probe and merge+stringify both streams.
        $eap = $ErrorActionPreference; $ErrorActionPreference = "Continue"
        try { $v = (& flutter --version 2>&1 | ForEach-Object { "$_" } | Select-String "^Flutter " | Select-Object -First 1) }
        finally { $ErrorActionPreference = $eap }
        return $v
    }
    $fv = FlutterVer
    DepMin "flutter" $fv $V.FLUTTER_MIN_VERSION $iFlutter
}

Write-Host ""
Write-Host ("Summary: {0} ok, {1} missing, {2} outdated, {3} conflict" -f $script:ok, $script:miss, $script:old, $script:conflict)
if ($script:conflict -gt 0) { exit 2 }
if (-not $script:DoInstall -and ($script:miss + $script:old) -gt 0) {
    Write-Host "Some dependencies are missing or out of date. Re-run with one of:"
    Write-Host "    pwsh setup_windows.ps1 -Install     # meet the minimum requirements"
    Write-Host "    pwsh setup_windows.ps1 -Upgrade     # also upgrade everything to newest"
    exit 1
}
exit 0
