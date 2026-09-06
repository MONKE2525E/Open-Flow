# Regenerates every packaged Verenu icon from the canonical five-bar SVG.
#
#   powershell -ExecutionPolicy Bypass -File scripts/generate-icons.ps1
#
# The canonical mark is deliberately a bare 254x208 SVG. Each output applies
# only a presentation transform (tile padding and uniform scale); no output
# owns a second set of bar coordinates.

Add-Type -AssemblyName System.Drawing

$repoRoot = Split-Path -Parent $PSScriptRoot
$iconDir = Join-Path $repoRoot 'src-tauri\icons'
$sourceSvg = Join-Path $iconDir 'verenu-mark.svg'
$generatedRust = Join-Path $repoRoot 'src-tauri\src\generated_icon_geometry.rs'
$accent = '#d97757'
$lightTile = '#f9f7f3'

function Read-CanonicalMark($path) {
  $content = Get-Content $path -Raw
  $viewBoxMatch = [regex]::Match($content, 'viewBox="0 0 254 208"')
  if (-not $viewBoxMatch.Success) { throw "Canonical mark must use viewBox 0 0 254 208: $path" }

  $rects = @()
  foreach ($match in [regex]::Matches($content, '<rect\s+x="([\d.]+)"\s+y="([\d.]+)"\s+width="([\d.]+)"\s+height="([\d.]+)"\s+rx="([\d.]+)"')) {
    $rects += [pscustomobject]@{
      x = [double]$match.Groups[1].Value
      y = [double]$match.Groups[2].Value
      w = [double]$match.Groups[3].Value
      h = [double]$match.Groups[4].Value
      r = [double]$match.Groups[5].Value
    }
  }
  if ($rects.Count -ne 5) { throw "Expected exactly 5 canonical bar rects in $path, found $($rects.Count)" }
  return $rects
}

function Add-RoundedRect([System.Drawing.Drawing2D.GraphicsPath]$path, $x, $y, $w, $h, $r) {
  if ($r -le 0) {
    $path.AddRectangle([System.Drawing.RectangleF]::new($x, $y, $w, $h))
    return
  }
  $d = $r * 2
  $path.AddArc($x, $y, $d, $d, 180, 90)
  $path.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
  $path.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
  $path.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
  $path.CloseFigure()
}

function Convert-HexColor($hex) {
  [System.Drawing.Color]::FromArgb(
    255,
    [Convert]::ToInt32($hex.Substring(1, 2), 16),
    [Convert]::ToInt32($hex.Substring(3, 2), 16),
    [Convert]::ToInt32($hex.Substring(5, 2), 16)
  )
}

# Platform presentation changes only tile padding and uniform scale. The bar
# geometry itself always comes from the canonical source.
function New-IconBitmap($size, $rects, $presentation) {
  $ss = [math]::Max($size * 8, 512)
  $big = New-Object System.Drawing.Bitmap($ss, $ss, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($big)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.Clear([System.Drawing.Color]::Transparent)
  $scaleToSs = $ss / 512.0

  if ($presentation -eq 'macos') {
    $tileX = 64.0; $tileY = 64.0; $tileSize = 384.0; $tileRadius = 76.0
    $glyphWidth = 254.0
  } elseif ($presentation -eq 'tray') {
    $tileX = 0.0; $tileY = 0.0; $tileSize = 512.0; $tileRadius = 96.0
    $glyphWidth = 336.0
  } else {
    $tileX = 0.0; $tileY = 0.0; $tileSize = 512.0; $tileRadius = 96.0
    $glyphWidth = 368.0
  }

  $tilePath = New-Object System.Drawing.Drawing2D.GraphicsPath
  Add-RoundedRect $tilePath ($tileX * $scaleToSs) ($tileY * $scaleToSs) ($tileSize * $scaleToSs) ($tileSize * $scaleToSs) ($tileRadius * $scaleToSs)
  $tileBrush = [System.Drawing.SolidBrush]::new((Convert-HexColor $lightTile))
  $g.FillPath($tileBrush, $tilePath)
  $tileBrush.Dispose(); $tilePath.Dispose()

  $canonicalScale = ($glyphWidth / 254.0) * $scaleToSs
  $glyphHeight = 208.0 * $canonicalScale
  $originX = ($ss - (254.0 * $canonicalScale)) / 2.0
  $originY = ($ss - $glyphHeight) / 2.0
  $barBrush = [System.Drawing.SolidBrush]::new((Convert-HexColor $accent))
  foreach ($rect in $rects) {
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    Add-RoundedRect $path `
      ($originX + $rect.x * $canonicalScale) `
      ($originY + $rect.y * $canonicalScale) `
      ($rect.w * $canonicalScale) `
      ($rect.h * $canonicalScale) `
      ($rect.r * $canonicalScale)
    $g.FillPath($barBrush, $path)
    $path.Dispose()
  }
  $barBrush.Dispose(); $g.Dispose()

  $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $dg = [System.Drawing.Graphics]::FromImage($bmp)
  $dg.Clear([System.Drawing.Color]::Transparent)
  $dg.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $dg.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $dg.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
  $attr = New-Object System.Drawing.Imaging.ImageAttributes
  $attr.SetWrapMode([System.Drawing.Drawing2D.WrapMode]::TileFlipXY)
  $dest = New-Object System.Drawing.Rectangle 0, 0, $size, $size
  $dg.DrawImage($big, $dest, 0, 0, $ss, $ss, [System.Drawing.GraphicsUnit]::Pixel, $attr)
  $attr.Dispose(); $dg.Dispose(); $big.Dispose()
  return $bmp
}

function Get-PngBytes($bitmap) {
  $stream = New-Object System.IO.MemoryStream
  $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
  $bitmap.Dispose()
  $data = $stream.ToArray()
  $stream.Dispose()
  return ,$data
}

function Save-Png($bitmap, $path) {
  [System.IO.File]::WriteAllBytes($path, (Get-PngBytes $bitmap))
  Write-Output "wrote $path"
}

function New-IcoFile($icoPath, $sizes, $rects) {
  $frames = @()
  foreach ($size in $sizes) {
    $frames += ,(Get-PngBytes (New-IconBitmap $size $rects 'windows'))
  }

  $directorySize = 6 + 16 * $sizes.Count
  $offset = $directorySize
  $ico = New-Object System.Collections.Generic.List[byte]
  $ico.AddRange([byte[]](0, 0, 1, 0))
  $ico.AddRange([BitConverter]::GetBytes([uint16]$sizes.Count))
  for ($i = 0; $i -lt $sizes.Count; $i++) {
    $dim = if ($sizes[$i] -eq 256) { 0 } else { $sizes[$i] }
    $ico.Add([byte]$dim); $ico.Add([byte]$dim)
    $ico.AddRange([byte[]](0, 0, 1, 0, 32, 0))
    $ico.AddRange([BitConverter]::GetBytes([uint32]$frames[$i].Length))
    $ico.AddRange([BitConverter]::GetBytes([uint32]$offset))
    $offset += $frames[$i].Length
  }
  foreach ($frame in $frames) { $ico.AddRange($frame) }
  [System.IO.File]::WriteAllBytes($icoPath, $ico.ToArray())
  Write-Output "wrote $icoPath"
}

function New-IcnsFile($icnsPath, $rects) {
  $entries = @(
    [pscustomobject]@{ tag = 'icp4'; size = 16 },
    [pscustomobject]@{ tag = 'icp5'; size = 32 },
    [pscustomobject]@{ tag = 'icp6'; size = 48 },
    [pscustomobject]@{ tag = 'ic07'; size = 128 },
    [pscustomobject]@{ tag = 'ic08'; size = 256 },
    [pscustomobject]@{ tag = 'ic09'; size = 512 },
    [pscustomobject]@{ tag = 'ic10'; size = 1024 }
  )
  $chunks = @()
  foreach ($entry in $entries) {
    $png = Get-PngBytes (New-IconBitmap $entry.size $rects 'macos')
    # Chunk integers are big-endian; reverse the little-endian bytes explicitly.
    $length = [BitConverter]::GetBytes([uint32](8 + $png.Length)); [array]::Reverse($length)
    $chunk = New-Object System.Collections.Generic.List[byte]
    $chunk.AddRange([Text.Encoding]::ASCII.GetBytes($entry.tag))
    $chunk.AddRange($length)
    $chunk.AddRange($png)
    $chunks += ,$chunk.ToArray()
  }
  $totalLength = 8 + (($chunks | ForEach-Object { $_.Length } | Measure-Object -Sum).Sum)
  $output = New-Object System.Collections.Generic.List[byte]
  $output.AddRange([Text.Encoding]::ASCII.GetBytes('icns'))
  $headerLength = [BitConverter]::GetBytes([uint32]$totalLength); [array]::Reverse($headerLength)
  $output.AddRange($headerLength)
  foreach ($chunk in $chunks) { $output.AddRange($chunk) }
  [System.IO.File]::WriteAllBytes($icnsPath, $output.ToArray())
  Write-Output "wrote $icnsPath"
}

function Write-RustGeometry($rects, $path) {
  $lines = @(
    '// Generated by scripts/generate-icons.ps1 from icons/verenu-mark.svg.',
    '// Do not edit the bar coordinates here; change the canonical SVG and regenerate.',
    '',
    '#[allow(dead_code)]',
    '#[derive(Clone, Copy)]',
    'pub(crate) struct CanonicalBar {',
    '    pub x: u32,',
    '    pub y: u32,',
    '    pub width: u32,',
    '    pub height: u32,',
    '    pub radius: u32,',
    '}',
    '',
    '#[allow(dead_code)]',
    'pub(crate) const CANONICAL_MARK_WIDTH: u32 = 254;',
    'pub(crate) const CANONICAL_MARK_HEIGHT: u32 = 208;',
    'pub(crate) const CANONICAL_BARS: [CanonicalBar; 5] = ['
  )
  foreach ($rect in $rects) {
    $lines += "    CanonicalBar { x: $([int]$rect.x), y: $([int]$rect.y), width: $([int]$rect.w), height: $([int]$rect.h), radius: $([int]$rect.r) },"
  }
  $lines += '];'
  Set-Content -LiteralPath $path -Value $lines -Encoding utf8
  Write-Output "wrote $path"
}

$rects = Read-CanonicalMark $sourceSvg
Write-RustGeometry $rects $generatedRust

Save-Png (New-IconBitmap 512 $rects 'windows') (Join-Path $iconDir 'icon.png')
Save-Png (New-IconBitmap 32  $rects 'windows') (Join-Path $iconDir '32x32.png')
Save-Png (New-IconBitmap 64  $rects 'windows') (Join-Path $iconDir '64x64.png')
Save-Png (New-IconBitmap 128 $rects 'windows') (Join-Path $iconDir '128x128.png')
Save-Png (New-IconBitmap 256 $rects 'windows') (Join-Path $iconDir '128x128@2x.png')
New-IcoFile (Join-Path $iconDir 'icon.ico') @(16, 20, 24, 32, 48, 64, 256) $rects
New-IcnsFile (Join-Path $iconDir 'icon.icns') $rects

Write-Output 'done'
