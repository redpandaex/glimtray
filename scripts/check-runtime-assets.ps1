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
$svgCount = 0
foreach ($source in $sources) {
    $body = Get-Content -LiteralPath $source.FullName -Raw
    $seen = [System.Collections.Generic.HashSet[string]]::new()
    foreach ($load in [regex]::Matches($body, $loadPattern)) {
        $resource = $load.Groups['resource'].Value
        $format = $load.Groups['format'].Value
        if (-not $seen.Add("$resource/$format")) { continue }
        if ($format -notin @('png', 'svg')) { throw "Unsupported runtime image format: $format ($resource)." }
        $imageCount++
        if ($format -ne 'svg') { continue }

        $definitionPrefix = '(?s)\bstatic\s+' + [regex]::Escape($resource) + '\b[^=]*=\s*'
        $definition = [regex]::Match($body, $definitionPrefix + 'b"(?<data>(?:\\.|[^"\\])*)"\s*;')
        if ($definition.Success) {
            # Rust byte strings escape non-ASCII UTF-8 as individual byte values.
            $svg = [regex]::Unescape($definition.Groups['data'].Value)
            $bytes = [System.Text.Encoding]::GetEncoding(28591).GetBytes($svg)
            $svgInput = [System.IO.MemoryStream]::new($bytes, $false)
        } else {
            $includePattern = $definitionPrefix + '(?:(?:::\s*)|(?:[A-Za-z_][A-Za-z0-9_]*\s*::\s*))*include_bytes\s*!\s*\(\s*"(?<path>(?:\\.|[^"\\])*)"\s*\)\s*;'
            $included = [regex]::Match($body, $includePattern)
            if (-not $included.Success) { throw "Unable to inspect $resource; review the generated Rust format." }
            $svgPath = [regex]::Unescape($included.Groups['path'].Value)
            if (-not [System.IO.Path]::IsPathRooted($svgPath)) { $svgPath = Join-Path $source.DirectoryName $svgPath }
            $svgInput = [System.IO.File]::OpenRead($svgPath)
        }
        $settings = [System.Xml.XmlReaderSettings]::new()
        $settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
        $settings.XmlResolver = $null
        $settings.MaxCharactersInDocument = 4MB
        $reader = $null
        try {
            $reader = [System.Xml.XmlReader]::Create($svgInput, $settings)
            $document = [System.Xml.XmlDocument]::new()
            $document.XmlResolver = $null
            $document.Load($reader)
            $unsupported = $document.SelectNodes("//*[local-name()='text' or local-name()='tspan' or local-name()='textPath' or local-name()='image' or local-name()='foreignObject']")
            if ($document.DocumentElement.LocalName -ne 'svg' -or $unsupported.Count) {
                throw "SVG $resource requires disabled text or raster-image support."
            }
        }
        finally {
            if ($reader) { $reader.Dispose() }
            $svgInput.Dispose()
        }
        $svgCount++
    }
}
if (-not $imageCount) { throw 'No embedded images inspected; review the generated Rust format.' }
[pscustomobject]@{ Sources = $sources.Count; Images = $imageCount; VectorSVGs = $svgCount }
