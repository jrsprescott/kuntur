<#
.SYNOPSIS
  Prueba de humo de Kuntur: compila, abre la app, escribe comandos DESDE DENTRO (variable
  KUNTUR_SMOKE_INPUT, nunca teclado simulado del sistema), captura solo su ventana y mide RAM.
.EXAMPLE
  .\scripts\smoke.ps1 -Commands 'Write-Host rojo -ForegroundColor Red','dir'
#>
param(
    [string[]]$Commands = @('Write-Host rojo -ForegroundColor Red; Write-Host verde -ForegroundColor Green', 'Get-ChildItem | Select-Object -First 5'),
    [string]$Out = (Join-Path $PSScriptRoot '..\target\smoke.png'),
    [int]$MaxRamMB = 80,
    [int]$WaitSec = 7
)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $root
try { cargo build -p kuntur-app --release -q } finally { Pop-Location }
$exe = Join-Path $root 'target\release\kuntur.exe'

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class KunturSmoke {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[KunturSmoke]::SetProcessDPIAware() | Out-Null

# NO_COLOR heredado (p. ej. de herramientas de IA) hace que PowerShell quite los colores.
$savedNoColor = $env:NO_COLOR; Remove-Item Env:NO_COLOR -ErrorAction SilentlyContinue
$env:KUNTUR_SMOKE_INPUT = ($Commands -join '\n')
$p = Start-Process $exe -PassThru
Remove-Item Env:KUNTUR_SMOKE_INPUT
if ($savedNoColor) { $env:NO_COLOR = $savedNoColor }

$deadline = (Get-Date).AddSeconds(20)
while ((Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 300; $p.Refresh(); if ($p.MainWindowHandle -ne 0) { break } }
if ($p.MainWindowHandle -eq 0) { Stop-Process -Id $p.Id -Force; throw 'Kuntur no abrió ventana' }
Start-Sleep -Seconds ($WaitSec + $Commands.Count)

$r = New-Object KunturSmoke+RECT; [KunturSmoke]::GetClientRect($p.MainWindowHandle, [ref]$r) | Out-Null
$bmp = New-Object System.Drawing.Bitmap ([Math]::Max(1, $r.R)), ([Math]::Max(1, $r.B))
$g = [System.Drawing.Graphics]::FromImage($bmp); $hdc = $g.GetHdc()
[KunturSmoke]::PrintWindow($p.MainWindowHandle, $hdc, 3) | Out-Null
$g.ReleaseHdc($hdc); $g.Dispose()
New-Item -ItemType Directory -Force -Path (Split-Path $Out) | Out-Null
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()

$p.Refresh(); $ram = [Math]::Round($p.WorkingSet64 / 1MB)
Stop-Process -Id $p.Id -Force
"Captura: $Out"
"RAM de Kuntur: $ram MB (máximo $MaxRamMB MB)"
if ($ram -gt $MaxRamMB) { throw "RAM por encima del objetivo: $ram MB" }
'SMOKE OK'
