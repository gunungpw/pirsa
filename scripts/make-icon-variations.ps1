# Generates transparent, glyph-only Pirsa icon alternatives from the window SVG.
# Requires ImageMagick 7 (`magick`) with SVG support.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$assets = Join-Path $root 'assets'
$output = Join-Path $assets 'icon-variations'
$sourcePath = Join-Path $assets 'andrsrxn--app-window.svg'

if (-not (Get-Command magick -ErrorAction SilentlyContinue)) {
    throw 'ImageMagick (magick) is required to generate the icon variations.'
}

$source = Get-Content -Path $sourcePath -Raw
$start = $source.IndexOf('<g fill="none">')
$end = $source.LastIndexOf('</g>') + 4

if ($start -lt 0 -or $end -le 0) {
    throw 'Could not find the icon group in the source SVG.'
}

$inner = $source.Substring($start, $end - $start)
New-Item -ItemType Directory -Force -Path $output | Out-Null

$variants = @(
    @{ Name = 'teal'; Label = 'Teal'; Glyph = '#14B8A6'; GradientTop = $null; GradientBottom = $null }
    @{ Name = 'teal-light'; Label = 'Teal Light'; Glyph = '#5EEAD4'; GradientTop = $null; GradientBottom = $null }
    @{ Name = 'teal-deep'; Label = 'Teal Deep'; Glyph = '#0F766E'; GradientTop = $null; GradientBottom = $null }
    @{ Name = 'teal-gradient'; Label = 'Teal Gradient'; Glyph = $null; GradientTop = '#5EEAD4'; GradientBottom = '#0F766E' }
)

foreach ($variant in $variants) {
    $defs = ''
    $glyphColor = $variant.Glyph

    if ($variant.GradientTop) {
        $defs = @"
  <defs>
    <linearGradient id="glyph" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="$($variant.GradientTop)"/>
      <stop offset="1" stop-color="$($variant.GradientBottom)"/>
    </linearGradient>
  </defs>
"@
        $glyphColor = 'url(#glyph)'
    }

    $glyph = $inner.Replace('currentColor', $glyphColor)
    $svg = @"
<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
$defs
  <g transform="translate(20 20) scale(42)">
$glyph
  </g>
</svg>
"@

    $svgPath = Join-Path $output "$($variant.Name).svg"
    $pngPath = Join-Path $output "$($variant.Name).png"
    $icoPath = Join-Path $output "$($variant.Name).ico"
    Set-Content -Path $svgPath -Value $svg -Encoding utf8

    & magick -background none $svgPath -resize 512x512 $pngPath
    if ($LASTEXITCODE -ne 0) {
        throw "PNG generation failed for $($variant.Name)."
    }

    & magick $pngPath -define icon:auto-resize=256,128,64,48,32,24,16 $icoPath
    if ($LASTEXITCODE -ne 0) {
        throw "ICO generation failed for $($variant.Name)."
    }
}

$montageArguments = @(
    '-background', '#101827',
    '-fill', '#DCE7F7',
    '-font', 'C:/Windows/Fonts/segoeui.ttf',
    '-pointsize', '20'
)

foreach ($variant in $variants) {
    $montageArguments += @('-label', $variant.Label, (Join-Path $output "$($variant.Name).png"))
}

$montageArguments += @('-tile', '2x2', '-geometry', '280x280+28+18', (Join-Path $output 'contact-sheet.png'))
& magick montage @montageArguments
if ($LASTEXITCODE -ne 0) {
    throw 'Contact sheet generation failed.'
}

Write-Host "Generated $($variants.Count) transparent icon variations in $output" -ForegroundColor Green
