<#
.SYNOPSIS
  Genera el instalador MSI de Kuntur (target\wix\kuntur-<versión>-x86_64.msi) con WiX Toolset v3 y
  cargo-wix. NO lo instala. Con -Check lo extrae con "msiexec /a" (instalación administrativa:
  solo copia archivos a una carpeta temporal, no registra nada) y lista lo que contiene.
.DESCRIPTION
  Requisitos (una vez):
    - WiX Toolset v3.14 (binarios oficiales) en una carpeta con bin\candle.exe y la variable WIX
      apuntando a ella (por ejemplo %LOCALAPPDATA%\Programs\WiX\).
    - cargo install cargo-wix --locked
.EXAMPLE
  .\scripts\msi.ps1 -Check
#>
param([switch]$Check)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
if (-not $env:WIX) { $env:WIX = [Environment]::GetEnvironmentVariable('WIX', 'User') }
if (-not $env:WIX -or -not (Test-Path (Join-Path $env:WIX 'bin\candle.exe'))) {
    throw 'No se encontró WiX Toolset v3: la variable WIX debe apuntar a la carpeta que contiene bin\candle.exe.'
}
Push-Location $root
# Privacidad (ajustes v0.2): Rust guarda en el .exe las rutas de los fuentes (las de las dependencias
# están bajo la carpeta del usuario, en .cargo\registry). Se reemplazan por rutas neutras al compilar
# y, antes de armar el MSI, se comprueba que no quede el nombre ni la carpeta del usuario.
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME '.cargo' }
$remap = @(
    "--remap-path-prefix=$HOME=~"
    "--remap-path-prefix=$cargoHome=cargo"
    "--remap-path-prefix=$root=kuntur"
) -join ' '
$savedFlags = $env:RUSTFLAGS
$env:RUSTFLAGS = (@($savedFlags, $remap) | Where-Object { $_ }) -join ' '
try {
    cargo build -p kuntur-app -p kuntur-hook --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build falló' }
    $leaks = @($env:USERNAME, $HOME, "$root") | Where-Object { $_ -and $_.Length -ge 3 } | Select-Object -Unique
    foreach ($exe in 'kuntur.exe', 'kuntur-hook.exe') {
        $bytes = [System.IO.File]::ReadAllBytes((Join-Path $root "target\release\$exe"))
        $text = [System.Text.Encoding]::Latin1.GetString($bytes)
        foreach ($leak in $leaks) {
            if ($text.IndexOf($leak, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                throw "$exe contiene una ruta o el nombre de usuario de esta PC: no se arma el MSI."
            }
        }
    }
    'Binarios revisados: sin rutas ni nombre de usuario de esta PC.'
    $version = (cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).packages |
        Where-Object { $_.name -eq 'kuntur-app' } | ForEach-Object { $_.version }
    $out = Join-Path $root "target\wix\kuntur-$version-x86_64.msi"
    cargo wix -p kuntur-app --no-build --nocapture --output $out
    if ($LASTEXITCODE -ne 0) { throw 'cargo wix falló' }
} finally {
    $env:RUSTFLAGS = $savedFlags
    Pop-Location
}

$msi = Get-Item $out
"MSI: $($msi.FullName) ($([Math]::Round($msi.Length / 1MB, 1)) MB)"
# Tablas que importan: archivos, acceso directo con su AUMID y registro.
$installer = New-Object -ComObject WindowsInstaller.Installer
$db = $installer.GetType().InvokeMember('OpenDatabase', 'InvokeMethod', $null, $installer, @($msi.FullName, 0))
foreach ($q in "SELECT FileName, FileSize FROM File", "SELECT Name, Target FROM Shortcut",
    'SELECT `PropertyKey`, `PropVariantValue` FROM `MsiShortcutProperty`', 'SELECT `Root`, `Key`, `Name`, `Value` FROM `Registry`',
    "SELECT Property, Value FROM Property WHERE Property='ProductVersion' OR Property='ProductName' OR Property='UpgradeCode'") {
    $view = $db.GetType().InvokeMember('OpenView', 'InvokeMethod', $null, $db, @($q))
    $view.GetType().InvokeMember('Execute', 'InvokeMethod', $null, $view, $null) | Out-Null
    "-- $q"
    while ($rec = $view.GetType().InvokeMember('Fetch', 'InvokeMethod', $null, $view, $null)) {
        $n = $rec.GetType().InvokeMember('FieldCount', 'GetProperty', $null, $rec, $null)
        '   ' + ((1..$n | ForEach-Object { $rec.GetType().InvokeMember('StringData', 'GetProperty', $null, $rec, @([int]$_)) }) -join ' | ')
    }
    $view.GetType().InvokeMember('Close', 'InvokeMethod', $null, $view, $null) | Out-Null
}
[System.Runtime.InteropServices.Marshal]::ReleaseComObject($db) | Out-Null

if ($Check) {
    $dest = Join-Path ([System.IO.Path]::GetTempPath()) ("kuntur-msi-" + [guid]::NewGuid().ToString('N'))
    $p = Start-Process msiexec.exe -ArgumentList '/a', "`"$($msi.FullName)`"", '/qn', "TARGETDIR=`"$dest`"" -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "msiexec /a terminó con $($p.ExitCode)" }
    'Contenido (instalación administrativa, nada se registró en el sistema):'
    Get-ChildItem -Recurse -File $dest | Where-Object { $_.Extension -ne '.msi' } | ForEach-Object {
        '  {0,-60} {1,10:N0} bytes' -f $_.FullName.Substring($dest.Length + 1), $_.Length
    }
    Remove-Item -Recurse -Force $dest
}
