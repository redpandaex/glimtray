[CmdletBinding()]
param(
    [string] $OutputDirectory = 'dist',
    [string] $UpxPath
)

$ErrorActionPreference = 'Stop'
# Windows PowerShell 5.1 does not auto-load the ZIP assemblies used below.
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$metadata = & cargo metadata --locked --no-deps --format-version 1 --manifest-path (Join-Path $PSScriptRoot '../Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Unable to read Cargo package metadata.' }
$version = (($metadata | ConvertFrom-Json).packages | Where-Object name -eq 'glimtray').version
if (-not $version) { throw 'GlimTray package version is missing.' }
# Prefer the newest release executable: the size-optimized build lands under the
# target triple directory, while a plain stable build uses target/release.
$executable = @(
    (Join-Path $PSScriptRoot '../target/x86_64-pc-windows-msvc/release/glimtray.exe'),
    (Join-Path $PSScriptRoot '../target/release/glimtray.exe')
) | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
    Sort-Object { (Get-Item -LiteralPath $_).LastWriteTimeUtc } -Descending |
    Select-Object -First 1
if (-not $executable) { throw 'Build the release executable before packaging.' }
Write-Host "Using $executable"

New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$OutputDirectory = (Resolve-Path -LiteralPath $OutputDirectory).Path
$variant = 'standard'
if ($UpxPath) { $variant = 'compact' }
$stem = "glimtray-$version-windows-x64-$variant"
$packagedExecutable = Join-Path $OutputDirectory "$stem.exe"
Copy-Item -LiteralPath $executable -Destination $packagedExecutable -Force
if ($UpxPath) {
    $upx = (Resolve-Path -LiteralPath $UpxPath -ErrorAction Stop).Path
    & $upx --best --lzma --compress-icons=0 --compress-resources=0 $packagedExecutable | ForEach-Object { Write-Host $_ }
    if ($LASTEXITCODE -ne 0) { throw 'UPX compression failed.' }
    & $upx -t $packagedExecutable | ForEach-Object { Write-Host $_ }
    if ($LASTEXITCODE -ne 0) { throw 'UPX integrity check failed.' }
}
$executable = $packagedExecutable
$name = "$stem.zip"
$archive = Join-Path $OutputDirectory $name
if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive -Force }
$zip = [System.IO.Compression.ZipFile]::Open([System.IO.Path]::GetFullPath($archive), [System.IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($entry in @(
        @{ Source = $executable; Name = 'glimtray.exe' },
        @{ Source = (Join-Path $PSScriptRoot '../README.md'); Name = 'README.md' }
    )) {
        [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
            $zip, [System.IO.Path]::GetFullPath($entry.Source), $entry.Name,
            [System.IO.Compression.CompressionLevel]::Optimal
        ) | Out-Null
    }
}
finally { $zip.Dispose() }
$hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
$executableHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
$checksums = Join-Path $OutputDirectory "$stem-SHA256SUMS.txt"
@(
    "$executableHash  $stem.exe"
    "$hash  $name"
) | Set-Content -LiteralPath $checksums -Encoding ascii
[pscustomobject]@{
    Version = $version
    Variant = $variant
    Executable = $executable
    ExecutableBytes = (Get-Item -LiteralPath $executable).Length
    Archive = $archive
    Checksums = $checksums
    SHA256 = $hash
}
