$ErrorActionPreference = 'Stop'
$checker = Join-Path $PSScriptRoot 'check-runtime-assets.ps1'
$workspace = Split-Path $PSScriptRoot -Parent
$fixture = Join-Path $workspace "work/runtime-assets-$([guid]::NewGuid().ToString('N'))"
$output = Join-Path $fixture 'out'
New-Item -ItemType Directory -Path $output -Force | Out-Null

function Set-Image([string] $Format) {
    @"
static SLINT_EMBEDDED_RESOURCE_0: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0h1v1z\"/></svg>";
load_image_from_embedded_data(SLINT_EMBEDDED_RESOURCE_0.into(), sp::Slice::from_slice(b"$Format"));
"@ | Set-Content -LiteralPath (Join-Path $output 'app-window.rs') -Encoding utf8
}

try {
    Set-Image 'png'
    $result = & $checker -BuildDirectory $fixture
    if ($result.Images -ne 1) { throw 'PNG resource was not inspected.' }
    foreach ($format in @('svg', 'jpeg')) {
        Set-Image $format
        $failure = $null
        try { $null = & $checker -BuildDirectory $fixture }
        catch { $failure = $_.Exception.Message }
        if ($failure -notlike "Unsupported runtime image format: $format *") {
            throw "Expected $format to be rejected; got: $failure"
        }
    }
    Write-Output 'Runtime assets: PNG accepted; SVG and JPEG rejected.'
}
finally {
    $resolvedFixture = (Resolve-Path -LiteralPath $fixture).Path
    $workRoot = [System.IO.Path]::GetFullPath((Join-Path $workspace 'work')) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $resolvedFixture.StartsWith($workRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove fixture outside $workRoot"
    }
    Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
}
