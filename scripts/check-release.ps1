[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Tag,
    [string] $MainRef = 'refs/remotes/origin/main'
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false

function Read-Git([string[]] $Arguments) {
    $result = & git @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "git $($Arguments -join ' ') failed: $result" }
    return ($result -join "`n").Trim()
}

$versionPattern = '^v(?<version>(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?<prerelease>-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?)$'
if ($Tag -cnotmatch $versionPattern) { throw "Invalid version tag: $Tag. Use v0.1.0 or v0.1.0-rc.1." }
$version = $Matches.version
$prerelease = -not [string]::IsNullOrEmpty($Matches.prerelease)
$commit = Read-Git @('rev-parse', '--verify', "refs/tags/$Tag^{commit}")
$headCommit = Read-Git @('rev-parse', 'HEAD')
if ($commit -ne $headCommit) { throw 'The checked-out commit does not match the release tag.' }
$null = Read-Git @('rev-parse', '--verify', "$MainRef^{commit}")
& git merge-base --is-ancestor $commit $MainRef
if ($LASTEXITCODE -ne 0) { throw "Tag $Tag is not on main. Merge the release commit into main first." }

$metadata = & cargo metadata --locked --no-deps --format-version 1
if ($LASTEXITCODE -ne 0) { throw 'Unable to read Cargo package metadata.' }
$package = ($metadata | ConvertFrom-Json).packages | Where-Object name -eq 'glimtray'
if (@($package).Count -ne 1 -or $package.version -cne $version) {
    throw "Tag version $version does not match Cargo package version $($package.version)."
}

[pscustomobject]@{ Version = $version; Commit = $commit; Prerelease = $prerelease }
