# Regenerates the Windows icon assets from the source SVG.
# Requires ImageMagick 7 (`magick`) with SVG support.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$assets = Join-Path $root 'assets'
$sourcePath = Join-Path $assets 'andrsrxn--app-window.svg'

if (-not (Get-Command magick -ErrorAction SilentlyContinue)) {
    throw 'ImageMagick (magick) is required to regenerate the icon.'
}

$source = Get-Content -Path $sourcePath -Raw
$start = $source.IndexOf('<g fill="none">')
$end = $source.LastIndexOf('</g>') + 4

if ($start -lt 0 -or $end -le 0) {
    throw 'Could not find the icon group in the source SVG.'
}

$inner = $source.Substring($start, $end - $start).Replace('currentColor', '#FFFFFF')
$wrapper = @"
<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#8AB4FF"/>
      <stop offset="1" stop-color="#5B7FE0"/>
    </linearGradient>
  </defs>
  <rect x="24" y="24" width="976" height="976" rx="224" fill="url(#bg)"/>
  <g transform="translate(256 256) scale(21.333333)">
$inner
  </g>
</svg>
"@

Set-Content -Path (Join-Path $assets 'pirsa-icon.svg') -Value $wrapper -Encoding utf8

& magick -background none (Join-Path $assets 'pirsa-icon.svg') -resize 1024x1024 (Join-Path $assets 'icon.png')
if ($LASTEXITCODE -ne 0) { throw 'PNG generation failed.' }

& magick (Join-Path $assets 'icon.png') -define icon:auto-resize=256,128,64,48,32,24,16 (Join-Path $assets 'icon.ico')
if ($LASTEXITCODE -ne 0) { throw 'ICO generation failed.' }

& magick (Join-Path $assets 'icon.png') -resize '64x64!' -depth 8 "rgba:$(Join-Path $assets 'icon-64.rgba')"
if ($LASTEXITCODE -ne 0) { throw 'RGBA generation failed.' }

Write-Host 'Regenerated assets/icon.png, icon.ico, and icon-64.rgba.' -ForegroundColor Green
