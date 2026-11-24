<#
.SYNOPSIS
  Prueba de humo de Kuntur: compila, abre la app, escribe comandos DESDE DENTRO (variable
  KUNTUR_SMOKE_INPUT, nunca teclado simulado del sistema), captura solo su ventana y mide RAM.
  Nunca usa la configuración real: KUNTUR_CONFIG apunta siempre a target\smoke-config.toml
  (si no existe, la app usa los valores por defecto).
.EXAMPLE
  .\scripts\smoke.ps1 -Commands 'Write-Host rojo -ForegroundColor Red','dir'
.EXAMPLE
  .\scripts\smoke.ps1 -Sessions 3 -Theme blue -Out target\smoke-blue.png
.EXAMPLE
  .\scripts\smoke.ps1 -Sessions 2 -Theme white -Palette -PaletteQuery 'tema' -Out target\smoke-palette.png
.EXAMPLE
  .\scripts\smoke.ps1 -Sessions 1 -Panel files -Out target\smoke-files.png
.EXAMPLE
  .\scripts\smoke.ps1 -Sessions 1 -Usage 30 -Theme dark -Out target\smoke-usage.png
.EXAMPLE
  .\scripts\smoke.ps1 -Sessions 2 -Commands 'cd C:\Windows' -Dock reopen -DockCommands 'Get-Location' -Out target\smoke-dock.png
#>
param(
    [string[]]$Commands = @('Write-Host rojo -ForegroundColor Red; Write-Host verde -ForegroundColor Green', 'Get-ChildItem | Select-Object -First 5'),
    [string]$Out = (Join-Path $PSScriptRoot '..\target\smoke.png'),
    [int]$MaxRamMB = 90,
    [int]$WaitSec = 7,
    # Sesiones que abre la app al arrancar (no se guardan). 0 = las de la configuración de prueba.
    [ValidateRange(0, 9)][int]$Sessions = 0,
    # Tema inicial, aplicado en vivo sobre las sesiones ya abiertas.
    [ValidateSet('', 'blue', 'dark', 'white')][string]$Theme = '',
    # Abre la paleta de comandos al iniciar (solo para capturarla), con texto opcional en el filtro.
    # -PaletteRename la abre ya en "Renombrar sesión" (el texto se agrega al nombre actual).
    [switch]$Palette,
    [string]$PaletteQuery = '',
    [switch]$PaletteRename,
    # Panel izquierdo: completo, compacto (solo íconos) o en "Archivos". Vacío = el de la configuración.
    [ValidateSet('', 'full', 'compact', 'files')][string]$Panel = '',
    # Abre "Uso de agentes" con datos SINTÉTICOS (nunca el historial real) en ese período.
    [ValidateSet('', 'hoy', '7', '30', 'ano', 'todo')][string]$Usage = '',
    # Terminal acoplada de la primera sesión, abierta después de los comandos de la principal (hereda
    # su carpeta). 'reopen' además la oculta y la vuelve a mostrar: el historial debe seguir ahí.
    [ValidateSet('', 'open', 'reopen')][string]$Dock = '',
    # Comandos que se escriben en la terminal acoplada (desde dentro de la app, como -Commands).
    [string[]]$DockCommands = @()
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

# Lanzar Kuntur como desde el menú Inicio: sin variables heredadas de la herramienta que corre la prueba.
# - NO_COLOR hace que PowerShell quite los colores.
# - CLAUDECODE / CLAUDE_* (si la prueba corre dentro de Claude Code) impiden abrir `claude` dentro de Kuntur.
$saved = @{}
Get-ChildItem Env: | Where-Object { $_.Name -eq 'NO_COLOR' -or $_.Name -like 'CLAUDE*' } | ForEach-Object {
    $saved[$_.Name] = $_.Value
    Remove-Item "Env:$($_.Name)"
}
$smokeEnv = @{
    KUNTUR_CONFIG      = (Join-Path $root 'target\smoke-config.toml')
    KUNTUR_SMOKE_INPUT = ($Commands -join '\n')
}
if ($Sessions -gt 0) { $smokeEnv.KUNTUR_SMOKE_SESSIONS = "$Sessions" }
if ($Theme) { $smokeEnv.KUNTUR_SMOKE_THEME = $Theme }
if ($Palette -or $PaletteQuery -or $PaletteRename) { $smokeEnv.KUNTUR_SMOKE_PALETTE = $(if ($PaletteRename) { 'rename' } else { '1' }) }
if ($PaletteQuery) { $smokeEnv.KUNTUR_SMOKE_PALETTE_QUERY = $PaletteQuery }
if ($Panel) { $smokeEnv.KUNTUR_SMOKE_PANEL = $Panel }
if ($Usage) { $smokeEnv.KUNTUR_SMOKE_USAGE = $Usage }
if ($Dock) { $smokeEnv.KUNTUR_SMOKE_DOCK = $Dock }
if ($Dock -and $DockCommands.Count -gt 0) { $smokeEnv.KUNTUR_SMOKE_DOCK_INPUT = ($DockCommands -join '\n') }
foreach ($k in $smokeEnv.Keys) { Set-Item "Env:$k" $smokeEnv[$k] }
$p = Start-Process $exe -PassThru
foreach ($k in $smokeEnv.Keys) { Remove-Item "Env:$k" }
foreach ($k in $saved.Keys) { Set-Item "Env:$k" $saved[$k] }

$deadline = (Get-Date).AddSeconds(20)
while ((Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 300; $p.Refresh(); if ($p.MainWindowHandle -ne 0) { break } }
if ($p.MainWindowHandle -eq 0) { Stop-Process -Id $p.Id -Force; throw 'Kuntur no abrió ventana' }
# La acoplada se abre después de los comandos de la principal y escribe los suyos; 'reopen' la oculta
# y la vuelve a mostrar (~2 s más).
$dockWait = 0
if ($Dock) { $dockWait = 5 + $DockCommands.Count }
if ($Dock -eq 'reopen') { $dockWait += 3 }
Start-Sleep -Seconds ($WaitSec + $Commands.Count + $dockWait)

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
