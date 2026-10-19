$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$guard = Join-Path $PSScriptRoot 'check-release.ps1'
$workspace = Split-Path $PSScriptRoot -Parent
$fixture = Join-Path $workspace "work/release-guard-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path (Join-Path $fixture 'src') -Force | Out-Null

function Run-Git([string[]] $Arguments) {
    $result = & git -c user.name='GlimTray Tests' -c user.email='tests@localhost' @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "Fixture git failed: $result" }
    return ($result -join "`n").Trim()
}
function Expect-Rejection([string] $Tag, [string] $Reason) {
    try { $null = & $guard -Tag $Tag }
    catch {
        if ($_.Exception.Message -notmatch $Reason) { throw }
        return
    }
    throw "Unexpectedly allowed $Tag; expected $Reason"
}
function Set-Version([string] $Version) {
    "[package]`nname = `"glimtray`"`nversion = `"$Version`"`nedition = `"2024`"" |
        Set-Content -LiteralPath Cargo.toml -Encoding utf8NoBOM
}

Push-Location $fixture
try {
    $null = Run-Git @('init', '-q', '-b', 'main')
    Set-Version '0.1.0'
    'fn main() {}' | Set-Content -LiteralPath src/main.rs -Encoding utf8NoBOM
    $null = Run-Git @('add', '.')
    $null = Run-Git @('commit', '-qm', 'Initial fixture')
    $null = Run-Git @('update-ref', 'refs/remotes/origin/main', 'HEAD')
    $null = Run-Git @('tag', 'v0.1.0')
    $result = & $guard -Tag 'v0.1.0'
    if ($result.Version -ne '0.1.0' -or $result.Prerelease) { throw 'Valid main tag rejected.' }

    Expect-Rejection 'vbad' 'Invalid version tag'
    $null = Run-Git @('tag', 'v0.2.0')
    Expect-Rejection 'v0.2.0' 'does not match Cargo'

    $null = Run-Git @('checkout', '-qb', 'feature')
    Set-Version '0.1.1'
    $null = Run-Git @('add', 'Cargo.toml')
    $null = Run-Git @('commit', '-qm', 'Feature-only commit')
    $null = Run-Git @('tag', 'v0.1.1')
    Expect-Rejection 'v0.1.1' 'not on main'
    Expect-Rejection 'v0.1.0' 'checked-out commit'

    $null = Run-Git @('checkout', '-q', 'main')
    Set-Version '0.1.0-rc.1'
    $null = Run-Git @('add', 'Cargo.toml')
    $null = Run-Git @('commit', '-qm', 'Prerelease on main')
    $null = Run-Git @('update-ref', 'refs/remotes/origin/main', 'HEAD')
    $null = Run-Git @('tag', '-a', 'v0.1.0-rc.1', '-m', 'Annotated prerelease')
    $result = & $guard -Tag 'v0.1.0-rc.1'
    if (-not $result.Prerelease) { throw 'Annotated prerelease was not recognized.' }

    $null = Run-Git @('checkout', '-q', 'v0.1.0')
    $result = & $guard -Tag 'v0.1.0'
    if ($result.Version -ne '0.1.0') { throw 'Tag in main history was rejected.' }
    Write-Output 'Release guard: 7 cases passed (all tags belong to an isolated fixture).'
}
finally {
    Pop-Location
    $resolved = [System.IO.Path]::GetFullPath($fixture)
    $allowed = [System.IO.Path]::GetFullPath((Join-Path $workspace 'work')) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($allowed, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove a fixture outside the workspace: $resolved"
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
