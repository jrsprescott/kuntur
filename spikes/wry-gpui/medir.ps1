<#
.SYNOPSIS
  Mide el spike de wry: abre la ventana en una pantalla secundaria y sin foco (como scripts\smoke.ps1),
  espera, captura solo su ventana (PrintWindow) y suma la RAM del proceso y de sus msedgewebview2.exe.
.EXAMPLE
  .\spikes\wry-gpui\medir.ps1 -Step menu -Out target\spike-menu.png
#>
param(
    [ValidateSet('', 'menu', 'menu-hide', 'resize', 'close')][string]$Step = '',
    [string]$File = '',
    [string]$Out = (Join-Path $PSScriptRoot 'target\spike.png'),
    [int]$WaitSec = 8
)
$ErrorActionPreference = 'Stop'
$exe = Join-Path $PSScriptRoot 'target\release\wry-gpui.exe'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class SpikeWin {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
"@
[SpikeWin]::SetProcessDPIAware() | Out-Null
$screens = @([System.Windows.Forms.Screen]::AllScreens | Sort-Object { $_.Bounds.X })
$display = -1
for ($i = $screens.Count - 1; $i -ge 0; $i--) { if (-not $screens[$i].Primary) { $display = $i; break } }
if ($display -lt 0) { throw 'No hay pantalla secundaria: el spike no se abre en la principal.' }

$env:SPIKE_DISPLAY = "$display"
$env:SPIKE_STEP = $Step
if ($File) { $env:SPIKE_FILE = (Resolve-Path $File).Path } else { Remove-Item Env:SPIKE_FILE -ErrorAction SilentlyContinue }
$p = Start-Process $exe -PassThru
Remove-Item Env:SPIKE_DISPLAY, Env:SPIKE_STEP -ErrorAction SilentlyContinue
Remove-Item Env:SPIKE_FILE -ErrorAction SilentlyContinue

$deadline = (Get-Date).AddSeconds(20)
while ((Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 300; $p.Refresh(); if ($p.MainWindowHandle -ne 0) { break } }
if ($p.MainWindowHandle -eq 0) { Stop-Process -Id $p.Id -Force; throw 'El spike no abrió ventana' }
"Foco: $(if ([SpikeWin]::GetForegroundWindow() -eq $p.MainWindowHandle) { 'lo TOMÓ' } else { 'no lo tomó' })"
Start-Sleep -Seconds $WaitSec

$r = New-Object SpikeWin+RECT; [SpikeWin]::GetClientRect($p.MainWindowHandle, [ref]$r) | Out-Null
$bmp = New-Object System.Drawing.Bitmap ([Math]::Max(1, $r.R)), ([Math]::Max(1, $r.B))
$g = [System.Drawing.Graphics]::FromImage($bmp); $hdc = $g.GetHdc()
[SpikeWin]::PrintWindow($p.MainWindowHandle, $hdc, 3) | Out-Null
$g.ReleaseHdc($hdc); $g.Dispose()
New-Item -ItemType Directory -Force -Path (Split-Path $Out) | Out-Null
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()

# RAM: el proceso del spike + todos sus descendientes msedgewebview2.exe.
$all = Get-CimInstance Win32_Process
$ids = @($p.Id); $added = $true
while ($added) {
    $added = $false
    foreach ($q in $all) { if ($ids -contains $q.ParentProcessId -and -not ($ids -contains $q.ProcessId)) { $ids += $q.ProcessId; $added = $true } }
}
$web = @($all | Where-Object { $ids -contains $_.ProcessId -and $_.Name -eq 'msedgewebview2.exe' })
$p.Refresh()
$own = [Math]::Round($p.WorkingSet64 / 1MB)
$webWs = [Math]::Round((($web | ForEach-Object { (Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue).WorkingSet64 } | Measure-Object -Sum).Sum) / 1MB)
$webPriv = [Math]::Round((($web | ForEach-Object { (Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue).PrivateMemorySize64 } | Measure-Object -Sum).Sum) / 1MB)
Stop-Process -Id $p.Id -Force
"Captura: $Out"
"RAM del spike (GPUI): $own MB"
"WebView2: $($web.Count) procesos · $webWs MB de memoria de trabajo · $webPriv MB privados"
