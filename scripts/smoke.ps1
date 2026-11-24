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
.EXAMPLE
  .\scripts\smoke.ps1 -Sessions 1 -Preview 'crates\kuntur-preview\tests\datos\ejemplo.md;crates\kuntur-preview\tests\datos\ejemplo.svg' -Out target\smoke-preview.png
.EXAMPLE
  .\scripts\smoke.ps1 -Sessions 1 -Preview 'crates\kuntur-preview\tests\datos\ejemplo.md' -PreviewEdit -PreviewDiff -Out target\smoke-diff.png
.EXAMPLE
  .\scripts\smoke.ps1 -Sessions 1 -Commands 'cd crates\kuntur-preview\tests\datos','dir; Write-Host ver .\ejemplo.md:3 y https://example.com' -Links -Out target\smoke-links.png
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
    [string[]]$DockCommands = @(),
    # Vista previa (F8): archivos (rutas relativas a la raíz del repo, separadas por ';') que se abren
    # al iniciar; usar solo los archivos SINTÉTICOS de crates\kuntur-preview\tests\datos.
    [string]$Preview = '',
    # Las pestañas de la vista previa abren en modo diff ("Cambios").
    [switch]$PreviewDiff,
    # Copia -Preview (un solo archivo) a target\smoke-preview\ y, a los pocos segundos, le agrega y
    # cambia líneas desde este script: comprueba la recarga automática y el diff contra la versión anterior.
    [switch]$PreviewEdit,
    # Subraya los enlaces (rutas/URLs) de la primera sesión después de sus comandos y abre el menú
    # contextual del primero que sea una ruta.
    [switch]$Links,
    # Pantalla donde se abre la ventana, contando de izquierda a derecha desde 0. Por defecto, la de
    # más a la derecha que NO sea la principal (en la PC del usuario: el monitor Teros), para no
    # interrumpir lo que pasa en la principal. La ventana se abre sin tomar el foco.
    [int]$Display = -1,
    # Permite usar la pantalla principal (solo si se pide explícitamente).
    [switch]$AllowPrimary
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
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
}
"@
[KunturSmoke]::SetProcessDPIAware() | Out-Null

# Pantalla de la prueba: nunca la principal salvo -AllowPrimary (ahí el usuario puede estar jugando).
Add-Type -AssemblyName System.Windows.Forms
$screens = @([System.Windows.Forms.Screen]::AllScreens | Sort-Object { $_.Bounds.X })
if ($Display -lt 0) {
    $Display = -1
    for ($i = $screens.Count - 1; $i -ge 0; $i--) { if (-not $screens[$i].Primary) { $Display = $i; break } }
    if ($Display -lt 0) { throw 'No hay otra pantalla además de la principal: usa -AllowPrimary si de verdad quieres abrirla ahí.' }
}
if ($Display -ge $screens.Count) { throw "No existe la pantalla $Display (hay $($screens.Count))." }
if ($screens[$Display].Primary -and -not $AllowPrimary) { throw "La pantalla $Display es la principal: usa -AllowPrimary para abrir la ventana ahí." }

# Lanzar Kuntur como desde el menú Inicio: sin variables heredadas de la herramienta que corre la prueba.
# - NO_COLOR hace que PowerShell quite los colores.
# - CLAUDECODE / CLAUDE_* (si la prueba corre dentro de Claude Code) impiden abrir `claude` dentro de Kuntur.
$saved = @{}
Get-ChildItem Env: | Where-Object { $_.Name -eq 'NO_COLOR' -or $_.Name -like 'CLAUDE*' } | ForEach-Object {
    $saved[$_.Name] = $_.Value
    Remove-Item "Env:$($_.Name)"
}
$smokeEnv = @{
    KUNTUR_CONFIG        = (Join-Path $root 'target\smoke-config.toml')
    KUNTUR_SMOKE_INPUT   = ($Commands -join '\n')
    KUNTUR_SMOKE_DISPLAY = "$Display"
}
if ($Sessions -gt 0) { $smokeEnv.KUNTUR_SMOKE_SESSIONS = "$Sessions" }
if ($Theme) { $smokeEnv.KUNTUR_SMOKE_THEME = $Theme }
if ($Palette -or $PaletteQuery -or $PaletteRename) { $smokeEnv.KUNTUR_SMOKE_PALETTE = $(if ($PaletteRename) { 'rename' } else { '1' }) }
if ($PaletteQuery) { $smokeEnv.KUNTUR_SMOKE_PALETTE_QUERY = $PaletteQuery }
if ($Panel) { $smokeEnv.KUNTUR_SMOKE_PANEL = $Panel }
if ($Usage) { $smokeEnv.KUNTUR_SMOKE_USAGE = $Usage }
if ($Dock) { $smokeEnv.KUNTUR_SMOKE_DOCK = $Dock }
if ($Dock -and $DockCommands.Count -gt 0) { $smokeEnv.KUNTUR_SMOKE_DOCK_INPUT = ($DockCommands -join '\n') }
$editFile = $null
if ($PreviewEdit -and $Preview) {
    # Copia de trabajo fuera del repo (target\ no se sube), para poder modificarla.
    $dir = Join-Path $root 'target\smoke-preview'
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $editFile = Join-Path $dir (Split-Path $Preview -Leaf)
    Copy-Item (Join-Path $root $Preview) $editFile -Force
    $Preview = $editFile
}
if ($Preview) {
    $smokeEnv.KUNTUR_SMOKE_PREVIEW = (($Preview -split ';') | Where-Object { $_ } | ForEach-Object {
            if ([System.IO.Path]::IsPathRooted($_)) { $_ } else { Join-Path $root $_ }
        }) -join ';'
}
if ($PreviewDiff) { $smokeEnv.KUNTUR_SMOKE_PREVIEW_DIFF = '1' }
if ($Links) { $smokeEnv.KUNTUR_SMOKE_LINKS = '1' }
foreach ($k in $smokeEnv.Keys) { Set-Item "Env:$k" $smokeEnv[$k] }
$p = Start-Process $exe -PassThru
foreach ($k in $smokeEnv.Keys) { Remove-Item "Env:$k" }
foreach ($k in $saved.Keys) { Set-Item "Env:$k" $saved[$k] }

$deadline = (Get-Date).AddSeconds(20)
while ((Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 300; $p.Refresh(); if ($p.MainWindowHandle -ne 0) { break } }
if ($p.MainWindowHandle -eq 0) { Stop-Process -Id $p.Id -Force; throw 'Kuntur no abrió ventana' }
# Comprobar que se abrió en la pantalla pedida y sin quitarle el foco a lo que el usuario usa.
$wr = New-Object KunturSmoke+RECT; [KunturSmoke]::GetWindowRect($p.MainWindowHandle, [ref]$wr) | Out-Null
$cxw = [int](($wr.L + $wr.R) / 2); $cyw = [int](($wr.T + $wr.B) / 2); $sb = $screens[$Display].Bounds
$enPantalla = $cxw -ge $sb.Left -and $cxw -lt $sb.Right -and $cyw -ge $sb.Top -and $cyw -lt $sb.Bottom
"Pantalla $Display ($($sb.Width)x$($sb.Height) en x=$($sb.X)): ventana $(if ($enPantalla) { 'dentro' } else { "FUERA (centro $cxw,$cyw)" }); foco: $(if ([KunturSmoke]::GetForegroundWindow() -eq $p.MainWindowHandle) { 'Kuntur lo TOMÓ' } else { 'no lo tomó' })"
if (-not $enPantalla) { Stop-Process -Id $p.Id -Force; throw 'La ventana no se abrió en la pantalla pedida.' }
# La acoplada se abre después de los comandos de la principal y escribe los suyos; 'reopen' la oculta
# y la vuelve a mostrar (~2 s más).
$dockWait = 0
if ($Dock) { $dockWait = 5 + $DockCommands.Count }
if ($Dock -eq 'reopen') { $dockWait += 3 }
if ($Links) { $dockWait += 3 }
if ($editFile) {
    # Cambia la copia como lo haría un agente: una línea modificada y dos nuevas.
    Start-Sleep -Seconds 3
    $text = [System.IO.File]::ReadAllText($editFile)
    $lines = $text -split "`n"
    if ($lines.Count -gt 2) { $lines[2] = $lines[2] + ' (línea cambiada por la prueba)' }
    $text = ($lines -join "`n") + "`n## Agregado por la prueba de humo`n`nEsta sección no estaba al abrir el archivo.`n"
    [System.IO.File]::WriteAllText($editFile, $text, (New-Object System.Text.UTF8Encoding $false))
    $dockWait += 1
}
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
