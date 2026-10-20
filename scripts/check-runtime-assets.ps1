[CmdletBinding()]
param([string] $BuildDirectory)

$ErrorActionPreference = 'Stop'
# Windows PowerShell 5.1 does not define $PSScriptRoot inside parameter defaults.
if (-not $BuildDirectory) {
    $BuildDirectory = Join-Path $PSScriptRoot '../target/x86_64-pc-windows-msvc/release/build'
    if (-not (Test-Path -LiteralPath $BuildDirectory)) {
        $BuildDirectory = Join-Path $PSScriptRoot '../target/release/build'
    }
}
# Plain builds use build/glimtray-<hash>/out; build-std uses build/glimtray/<hash>/out.
$sources = @(Get-ChildItem -LiteralPath $BuildDirectory -Recurse -Filter 'app-window.rs' -File |
    Where-Object { $_.Directory.Name -eq 'out' })
if (-not $sources.Count) { throw 'Build the release executable before checking embedded images.' }

$loadPattern = 'load_image_from_embedded_data\s*\(\s*(?<resource>SLINT_EMBEDDED_RESOURCE_\d+)\s*\.\s*into\s*\(\s*\)\s*,\s*sp\s*::\s*Slice\s*::\s*from_slice\s*\(\s*b"(?<format>[^"]+)"'
$imageCount = 0
foreach ($source in $sources) {
    $body = Get-Content -LiteralPath $source.FullName -Raw
    $seen = [System.Collections.Generic.HashSet[string]]::new()
    foreach ($load in [regex]::Matches($body, $loadPattern)) {
        $resource = $load.Groups['resource'].Value
        $format = $load.Groups['format'].Value
        if (-not $seen.Add("$resource/$format")) { continue }
        if ($format -ne 'png') { throw "Unsupported runtime image format: $format ($resource). Only PNG decoding is enabled." }
        $imageCount++
    }
}
if (-not $imageCount) { throw 'No embedded images inspected; review the generated Rust format.' }
[pscustomobject]@{ Sources = $sources.Count; Images = $imageCount }
