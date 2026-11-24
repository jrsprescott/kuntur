<#
.SYNOPSIS
  Genera assets\kuntur.ico: una "K" blanca sobre un cuadrado redondeado con el degradado del tema
  Blue, con un ala estilizada (el cóndor) en el trazo superior. Tamaños 16 a 256 px, cada uno como
  PNG dentro del .ico (Windows Vista o posterior). Solo hace falta correrlo si se cambia el diseño.
.EXAMPLE
  .\scripts\generar-icono.ps1
#>
param([string]$Out = (Join-Path $PSScriptRoot '..\assets\kuntur.ico'))
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

function New-IconPng([int]$s) {
    $bmp = New-Object System.Drawing.Bitmap $s, $s, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    # Fondo: cuadrado redondeado con el degradado del tema Blue (#1a379a → #3b82f6, 140°).
    $r = [Math]::Max(2, [int]($s * 0.22))
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = 2 * $r; $w = $s - 1
    $path.AddArc(0, 0, $d, $d, 180, 90)
    $path.AddArc($w - $d, 0, $d, $d, 270, 90)
    $path.AddArc($w - $d, $w - $d, $d, $d, 0, 90)
    $path.AddArc(0, $w - $d, $d, $d, 90, 90)
    $path.CloseFigure()
    $rect = New-Object System.Drawing.Rectangle 0, 0, $s, $s
    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush $rect,
        ([System.Drawing.ColorTranslator]::FromHtml('#1a379a')),
        ([System.Drawing.ColorTranslator]::FromHtml('#3b82f6')), 50.0
    $g.FillPath($brush, $path)

    # "K" geométrica: un palo vertical y dos brazos. El brazo de arriba termina en punta (ala).
    $white = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::White)
    $u = $s / 16.0
    $stem = New-Object System.Drawing.RectangleF (4.0 * $u), (3.0 * $u), (2.4 * $u), (10.0 * $u)
    $g.FillRectangle($white, $stem)
    $up = [System.Drawing.PointF[]]@(
        (New-Object System.Drawing.PointF (6.4 * $u), (8.6 * $u)),
        (New-Object System.Drawing.PointF (12.6 * $u), (2.6 * $u)),
        (New-Object System.Drawing.PointF (13.2 * $u), (5.4 * $u)),
        (New-Object System.Drawing.PointF (6.4 * $u), (10.8 * $u))
    )
    $g.FillPolygon($white, $up)
    $down = [System.Drawing.PointF[]]@(
        (New-Object System.Drawing.PointF (7.6 * $u), (8.4 * $u)),
        (New-Object System.Drawing.PointF (9.4 * $u), (7.0 * $u)),
        (New-Object System.Drawing.PointF (12.4 * $u), (13.0 * $u)),
        (New-Object System.Drawing.PointF (9.8 * $u), (13.0 * $u))
    )
    $g.FillPolygon($white, $down)
    $g.Dispose()

    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    , $ms.ToArray()
}

$sizes = 16, 24, 32, 48, 64, 128, 256
$images = @($sizes | ForEach-Object { , (New-IconPng $_) })
$fs = New-Object System.IO.MemoryStream
$bw = New-Object System.IO.BinaryWriter $fs
# ICONDIR
$bw.Write([UInt16]0); $bw.Write([UInt16]1); $bw.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $s = $sizes[$i]; $len = $images[$i].Length
    $b = if ($s -ge 256) { 0 } else { $s }
    $bw.Write([byte]$b); $bw.Write([byte]$b); $bw.Write([byte]0); $bw.Write([byte]0)
    $bw.Write([UInt16]1); $bw.Write([UInt16]32); $bw.Write([UInt32]$len); $bw.Write([UInt32]$offset)
    $offset += $len
}
foreach ($img in $images) { $bw.Write($img) }
$bw.Flush()
New-Item -ItemType Directory -Force -Path (Split-Path $Out) | Out-Null
[System.IO.File]::WriteAllBytes($Out, $fs.ToArray())
"Ícono: $Out ($($fs.Length) bytes)"
# Vista previa en grande (no se sube: target\).
$preview = Join-Path $PSScriptRoot '..\target\kuntur-icon-256.png'
New-Item -ItemType Directory -Force -Path (Split-Path $preview) | Out-Null
[System.IO.File]::WriteAllBytes($preview, $images[-1])
