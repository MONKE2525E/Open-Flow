# Regenerates the Windows app icons from src-tauri/icons/icon-source-windows.svg
# (full-bleed paper tile so the taskbar/exe icon renders at full size).
#
#   powershell -ExecutionPolicy Bypass -File scripts/generate-icons.ps1
#
# macOS icons (icon.icns) are generated from the inset icon-source.svg and are
# intentionally left untouched by this script.
#
# Outputs (relative to the repo root):
#   src-tauri/icons/icon.png         512x512
#   src-tauri/icons/32x32.png        32x32
#   src-tauri/icons/64x64.png        64x64
#   src-tauri/icons/128x128.png      128x128
#   src-tauri/icons/128x128@2x.png   256x256
#   src-tauri/icons/icon.ico         frames 16, 24, 32, 48, 64, 256

Add-Type -AssemblyName System.Drawing

$repoRoot = Split-Path -Parent $PSScriptRoot
$iconDir = Join-Path $repoRoot 'src-tauri\icons'
$sourceSvg = Join-Path $iconDir 'icon-source-windows.svg'
$tileColor = [System.Drawing.Color]::FromArgb(255, 249, 247, 243) # #f9f7f3
$barColor  = [System.Drawing.Color]::FromArgb(255, 217, 119, 87)  # #d97757

function Read-SvgRects($path) {
  $content = Get-Content $path -Raw
  $rects = @()
  foreach ($m in [regex]::Matches($content, '<rect\s+x="([\d.]+)"\s+y="([\d.]+)"\s+width="([\d.]+)"\s+height="([\d.]+)"\s+rx="([\d.]+)"\s+fill="(#[\da-fA-F]{6})"/>')) {
    $rects += [pscustomobject]@{
      x = [double]$m.Groups[1].Value
      y = [double]$m.Groups[2].Value
      w = [double]$m.Groups[3].Value
      h = [double]$m.Groups[4].Value
      r = [double]$m.Groups[5].Value
      color = $m.Groups[6].Value
    }
  }
  if ($rects.Count -lt 6) { throw "Expected 6 rects (1 tile + 5 bars) in $path, found $($rects.Count)" }
  return $rects
}

function Add-RoundedRect([System.Drawing.Drawing2D.GraphicsPath]$path, $x, $y, $w, $h, $r) {
  if ($r -le 0) { $path.AddRectangle([System.Drawing.RectangleF]::new($x, $y, $w, $h)) }
  else {
    $d = $r * 2
    $path.AddArc($x, $y, $d, $d, 180, 90)
    $path.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $path.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $path.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $path.CloseFigure()
  }
}

function Convert-HexColor($hex) {
  return [System.Drawing.Color]::FromArgb(
    255,
    [Convert]::ToInt32($hex.Substring(1, 2), 16),
    [Convert]::ToInt32($hex.Substring(3, 2), 16),
    [Convert]::ToInt32($hex.Substring(5, 2), 16)
  )
}

# Renders at `size` by drawing into a much larger buffer and box-filtering down.
# Rasterising small frames directly let each one round differently: the 16px and
# 24px frames came out 68.8% and 70.8% wide against a 65.6% target, so the icon
# visibly changed proportions between shell contexts. Supersampling keeps every
# frame on the same normalized geometry.
function New-IconBitmap($size, $rects) {
  # 8x oversample, but always rasterise at >= 512 so the rounded corners and bar
  # radii are resolved on the same grid the source SVG was authored against.
  $ss = [math]::Max($size * 8, 512)
  $big = New-Object System.Drawing.Bitmap($ss, $ss, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($big)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.Clear([System.Drawing.Color]::Transparent)
  $scale = $ss / 512.0

  foreach ($rect in $rects) {
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    Add-RoundedRect $path `
      ([float]($rect.x * $scale)) ([float]($rect.y * $scale)) `
      ([float]($rect.w * $scale)) ([float]($rect.h * $scale)) `
      ([float]($rect.r * $scale))
    $brush = [System.Drawing.SolidBrush]::new((Convert-HexColor $rect.color))
    $g.FillPath($brush, $path)
    $brush.Dispose()
    $path.Dispose()
  }
  $g.Dispose()

  if ($ss -eq $size) { return $big }

  $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $dg = [System.Drawing.Graphics]::FromImage($bmp)
  $dg.Clear([System.Drawing.Color]::Transparent)
  $dg.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $dg.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $dg.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
  # Draw into an explicit rectangle with a wrap-mode clamp so the bicubic kernel
  # does not sample transparent pixels from beyond the edge and pull in a halo.
  $attr = New-Object System.Drawing.Imaging.ImageAttributes
  $attr.SetWrapMode([System.Drawing.Drawing2D.WrapMode]::TileFlipXY)
  $dest = New-Object System.Drawing.Rectangle 0, 0, $size, $size
  $dg.DrawImage($big, $dest, 0, 0, $ss, $ss, [System.Drawing.GraphicsUnit]::Pixel, $attr)
  $attr.Dispose()
  $dg.Dispose()
  $big.Dispose()
  return $bmp
}

function Save-Png($bitmap, $path) {
  $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  Write-Output "wrote $path"
  $bitmap.Dispose()
}

function New-IconFile($icoPath, $sizes, $rects) {
  $frameStreams = @()
  $frameSizes = @()
  foreach ($size in $sizes) {
    $bmp = New-IconBitmap $size $rects
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    $data = $ms.ToArray()
    $frameStreams += , $data
    $frameSizes += $data.Length
    $ms.Dispose()
  }

  $count = $sizes.Count
  $headerSize = 6 + 16 * $count
  $offset = $headerSize
  $ico = New-Object System.Collections.Generic.List[byte]
  $ico.Add(0); $ico.Add(0); $ico.Add(1); $ico.Add(0)
  $ico.Add([byte]$count); $ico.Add(0)

  for ($i = 0; $i -lt $count; $i++) {
    $dim = if ($sizes[$i] -eq 256) { 0 } else { $sizes[$i] }
    $ico.Add([byte]$dim); $ico.Add([byte]$dim)          # width/height (0 = 256)
    $ico.Add(0); $ico.Add(0)                             # palette, reserved
    $ico.Add(1); $ico.Add(0)                             # color planes
    $ico.Add(32); $ico.Add(0)                            # bits per pixel
    $ico.AddRange([BitConverter]::GetBytes([uint32]$frameSizes[$i]))
    $ico.AddRange([BitConverter]::GetBytes([uint32]$offset))
    $offset += $frameSizes[$i]
  }
  for ($i = 0; $i -lt $count; $i++) { $ico.AddRange($frameStreams[$i]) }

  [System.IO.File]::WriteAllBytes($icoPath, $ico.ToArray())
  Write-Output "wrote $icoPath"
}

$rects = Read-SvgRects $sourceSvg

Save-Png (New-IconBitmap 512 $rects) (Join-Path $iconDir 'icon.png')
Save-Png (New-IconBitmap 32  $rects) (Join-Path $iconDir '32x32.png')
Save-Png (New-IconBitmap 64  $rects) (Join-Path $iconDir '64x64.png')
Save-Png (New-IconBitmap 128 $rects) (Join-Path $iconDir '128x128.png')
Save-Png (New-IconBitmap 256 $rects) (Join-Path $iconDir '128x128@2x.png')
# 20px is the shell's small-icon size at 125% scaling; without it Windows
# downsamples the 24px frame and the bars lose a pixel of weight.
New-IconFile (Join-Path $iconDir 'icon.ico') @(16, 20, 24, 32, 48, 64, 256) $rects

Write-Output 'done'
