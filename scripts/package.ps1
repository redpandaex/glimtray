[CmdletBinding()]
param([string] $OutputDirectory = 'dist')

$ErrorActionPreference = 'Stop'
$metadata = & cargo metadata --locked --no-deps --format-version 1
if ($LASTEXITCODE -ne 0) { throw 'Unable to read Cargo package metadata.' }
$version = (($metadata | ConvertFrom-Json).packages | Where-Object name -eq 'glimtray').version
if (-not $version) { throw 'GlimTray package version is missing.' }
$executable = Join-Path $PSScriptRoot '../target/release/glimtray.exe'
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw 'Build the release executable before packaging.' }

New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$name = "glimtray-$version-windows-x64.zip"
$archive = Join-Path $OutputDirectory $name
Compress-Archive -LiteralPath $executable, (Join-Path $PSScriptRoot '../README.md') -DestinationPath $archive -CompressionLevel Optimal -Force
$hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $name" | Set-Content -LiteralPath (Join-Path $OutputDirectory 'SHA256SUMS.txt') -Encoding ascii
[pscustomobject]@{ Version = $version; Archive = $archive; SHA256 = $hash }
