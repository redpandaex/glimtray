[CmdletBinding()]
param(
    # Keep this pin in sync with .github/workflows/build.yml.
    [string] $Toolchain = 'nightly-2026-10-18'
)

$ErrorActionPreference = 'Stop'
$target = 'x86_64-pc-windows-msvc'
$installed = & rustup component list --toolchain $Toolchain --installed 2>$null
if ($LASTEXITCODE -ne 0) {
    throw "Rust toolchain $Toolchain is not installed. Run: rustup toolchain install $Toolchain --profile minimal --component rust-src"
}
if (-not ($installed -match '^rust-src')) {
    throw "Rust toolchain $Toolchain is missing rust-src. Run: rustup component add rust-src --toolchain $Toolchain"
}

# The cargo alias in .cargo/config.toml owns the build-std and rustflags.
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
    & cargo "+$Toolchain" release-build
    if ($LASTEXITCODE -ne 0) { throw 'Size-optimized release build failed.' }
}
finally { Pop-Location }
Write-Host "Built target/$target/release/glimtray.exe"
