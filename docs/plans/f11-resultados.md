# F11 · Pulido y publicación — resultados

Rama `fase/f11-publicacion` (sale de `main` con F1–F9). Plan: [`2026-11-23-f11-pulido-y-publicacion.md`](2026-11-23-f11-pulido-y-publicacion.md).
El repositorio **sigue privado**: lo publica el usuario cuando termine de probar (no se usó `gh repo edit` ni se crearon releases).

## Qué se hizo

| Parte | Archivos | Estado |
|---|---|---|
| Licencia MIT OR Apache-2.0 (copyright `jrsprescott` 2026) | `LICENSE-MIT`, `LICENSE-APACHE` (texto oficial de apache.org), `license` en `[workspace.package]` y `license.workspace = true` en los 11 crates | ✅ |
| Ícono "K" con ala (degradado Blue) | `assets/kuntur.ico` (16–256 px), `scripts/generar-icono.ps1` | ✅ |
| Ícono y versión dentro de `kuntur.exe` | `crates/kuntur-app/build.rs` (`embed-resource`; recurso 1 = ícono de la ventana en GPUI) | ✅ |
| Instalador MSI | `crates/kuntur-app/wix/main.wxs`, `[package.metadata.wix]`, `scripts/msi.ps1` | ✅ generado y revisado, **no instalado** |
| AUMID propio en las notificaciones | `notify.rs` (`Kuntur.Kuntur` si el registro existe; si no, PowerShell), `main.rs` (`set_app_identity` solo si está instalado) | ✅ con prueba; falta verlo instalado ⏳ |
| Contraste WCAG de los 3 temas | prueba `theme_bridge::contraste`; tema Blue corregido | ✅ |
| Teclado en Uso, Auditoría y Conexiones | `keynav.rs` (lógica con pruebas), `usage_view.rs`, `audit_view.rs`, `connections_view.rs`, 1 línea en `workspace.rs` | ✅ |
| Medición final | `scripts/smoke.ps1 -Measure` (nuevo) | ✅ tabla abajo |
| Documentación pública | `README.md` (inglés), `README.es.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`, `docs/images/*.png` | ✅ |
| Revisión de privacidad del historial | sección abajo | ✅ (decide el usuario) |
| CI | `.github/workflows/ci.yml`: fmt, clippy, pruebas; MSI como artefacto en etiquetas `v*` | ✅ sintaxis revisada a mano; sin ver la ejecución |

Pruebas: **355** (349 + 6 nuevas: contraste ×2, AUMID, teclado ×3), clippy y fmt en verde.

## Instalador MSI

Herramientas (fuera del repo):
- **WiX Toolset v3.14.1** en `%LOCALAPPDATA%\Programs\WiX\bin`, variable de usuario `WIX` apuntando a `%LOCALAPPDATA%\Programs\WiX\`.
  Los `.exe` del zip oficial `wix314-binaries.zip` **no traen firma Authenticode** (`NotSigned`; las DLL sí: "WiX Toolset (.NET Foundation)").
  Verificación alternativa: se descargó el instalador oficial **firmado** `wix314.exe` (firma válida, "WiX Toolset (.NET Foundation)"),
  se extrajeron sus MSI, los SHA-1 de los MSI y CAB coinciden con el manifiesto firmado del paquete, y los 66 archivos del zip son
  idénticos (SHA-256) a los de esos MSI (extraídos con `msiexec /a`, sin instalar). `retina.exe` (no está en los MSI y no se usa) se borró.
  SHA-256 del zip: `6AC824E1642D6F7277D0ED7EA09411A508F6116BA6FAE0AA5F2C7DAA2FF43D31` (el CI lo comprueba).
- **cargo-wix 0.3.9** (`cargo install cargo-wix --locked`).

`.\scripts\msi.ps1 -Check` → `target\wix\kuntur-0.1.0-x86_64.msi` (**12 MB**). Contenido (instalación administrativa a una carpeta temporal, nada registrado):

| Tabla | Contenido |
|---|---|
| Archivos (`Program Files\Kuntur`) | `kuntur.exe` (32,4 MB), `kuntur-hook.exe` (0,29 MB), `kuntur.ico`, `LICENSE-MIT.txt`, `LICENSE-APACHE.txt` |
| Acceso directo | menú Inicio "Kuntur" → `[APPLICATIONFOLDER]kuntur.exe`, ícono propio |
| `MsiShortcutProperty` | `System.AppUserModel.ID = Kuntur.Kuntur` |
| Registro (HKLM, 64 bits) | `Software\Classes\AppUserModelId\Kuntur.Kuntur`: `DisplayName = Kuntur`, `IconUri = …\kuntur.ico` (se borra al desinstalar) |
| Registro (HKCU) | `Software\Kuntur\Installer` (solo la KeyPath del acceso directo que exige Windows Installer, ICE43/ICE57; se borra al desinstalar) |
| Producto | `Kuntur` 0.1.0 (de `Cargo.toml`), por máquina, `MajorUpgrade` (UpgradeCode `56DDC03F-…`), sin página de licencia |

Desinstalar quita archivos, acceso y registro. No toca `%APPDATA%\Kuntur` ni los ganchos de `~/.claude` (se quitan desde Kuntur antes de desinstalar).

## Medición final (`scripts/smoke.ps1 -Measure`)

Monitor secundario, sin foco, `-Commands 'cls'` (sin trabajo pendiente en los shells), CPU medida 20 s en reposo después de la captura.
"5 sesiones" = 5 pestañas + eventos **sintéticos** de agentes (`scripts\smoke-agentes.jsonl`: 3 agentes y 4 subagentes, Permitir/Rechazar, Enviar a terminal).

| Medida | Meta (`PLAN.md` §1) | 1 sesión (3 corridas) | 5 sesiones + agentes (3 corridas) |
|---|---|---|---|
| RAM de Kuntur (working set) | < 120 MB con 5 sesiones; < 80 MB vacía | **63 MB** | **66 MB** |
| Memoria privada de Kuntur | — | 80–81 MB | 91–92 MB |
| Shells (powershell + conhost, aparte) | — | 88 MB | 426–429 MB (5 shells) |
| Arranque hasta la primera ventana | < 300 ms | **265–275 ms** (556 ms en la 1.ª tras compilar: caché fría) | **249–255 ms** |
| CPU en reposo | ~0 % | 1,4–2,0 % de un núcleo | 2,0–2,6 % de un núcleo |

| Binario release | Tamaño |
|---|---|
| `kuntur.exe` | 32,4 MB (F1: 19 MB; crecieron SSH, SQLite, WebView2, vista previa) |
| `kuntur-hook.exe` | 0,29 MB |
| MSI (comprimido) | 12 MB |

Metas de RAM y arranque **cumplidas**. CPU en reposo: no es 0 pero es baja (~1,5 % de **un** núcleo ≈ 0,1 % del total en 16 hilos). Investigación:
- Con un interruptor temporal (ya quitado) se apagó el monitor de 2 s (procesos + RAM + `cx.notify()`): bajó a 0,8–1,3 %. La lectura de procesos (`sysinfo`) cuesta ~11 ms cada 2 s y la lectura lenta (agentes instalados, límites de Codex) ~12 ms cada 30 s.
- El resto (~1 %) es de GPUI/ConPTY con la ventana quieta.
- Una corrida de 30 s dio una vez 13 % y en la siguiente la app se cerró sola durante la espera (no se repitió en las 9 corridas posteriores; ⏳ vigilar).
- **Propuesta** (no aplicada para no chocar con F10 en `workspace.rs`): en `apply_sample`, llamar a `cx.notify()` solo si cambió algo visible (agentes, ocupados, RAM redondeada a MB, alertas) y espaciar el monitor a 3–5 s con la ventana sin foco.

## Accesibilidad

- **Contraste** (prueba `los_tres_temas_cumplen_el_contraste_wcag`): `text` ≥ 4,5:1 y `muted`, `accent`, `ok`, `warn`, `error` ≥ 3:1 sobre cada fondo real (cada color del degradado y `titlebar`, `surface`, `surface_2`, `popover`, `terminal_bg` mezclados encima). Dark y White ya cumplían (Dark: texto 12,2, tenue 5,2; White: texto 14,0, tenue 4,8). **Blue fallaba** (texto 2,95; tenue 1,83; error 1,20 sobre el final claro `#3b82f6`): degradado `#1a379a → #1f4fc2`, `muted #bcd0f5`, `accent #5eeafd`, `ok #6ee7a0`, `warn #fcd34d`, `error #fca5a5` → mínimos 5,36 / 3,87 / 4,21 / 3,91 / 4,18 / 3,18.
- **Teclado**: la paleta ya lo tenía. Nuevo: Uso (←/→ período, Ctrl+S CSV, Esc), Auditoría (escribir busca, ←/→ período, ↑/↓ agente, Ctrl+S CSV, Esc limpia y luego cierra), Conexiones (↑/↓ elegir con la fila resaltada y sus acciones visibles, Enter conectar, F2 editar, Supr pide confirmar → Enter sí / Esc no, Ctrl+N nueva, Esc cierra). **Esc** es igual en todas las capas (cierra la de arriba; sin capas va al shell).
- **Tamaño de letra**: Ctrl+= / Ctrl+− / Ctrl+0 (terminal).
- **Lo que GPUI no permite**: no expone árbol de accesibilidad (UI Automation / MSAA) en Windows, así que **Narrador, NVDA y JAWS no leen** la interfaz ni la terminal; tampoco hay foco por Tab entre controles genéricos (cada panel maneja su teclado a mano), ni respeta "alto contraste" de Windows ni la escala de texto del sistema para la UI (solo la de pantalla). Queda para cuando GPUI lo soporte.

## Capturas para el README (`docs/images/`)

Todas con `smoke.ps1`, datos sintéticos, `cd C:\Windows` antes de capturar, revisadas a ojo:
- `kuntur-dark.png`: `-Sessions 5 -Panel full -Theme dark -AgentEvents scripts\smoke-agentes.jsonl -Commands 'cd C:\Windows','cls','Get-ChildItem -Directory | …' -Dock open -DockCommands 'cd C:\Windows\Media','Get-ChildItem *.wav | …'` (67 MB).
- `kuntur-usage.png`: `-Sessions 1 -Theme blue -Usage 30` (uso sintético).
- `kuntur-preview.png`: `-Sessions 1 -Theme white -Preview '…\ejemplo.md' -PreviewEdit -PreviewDiff`. La barra de ruta de la vista previa mostraba la carpeta del worktree de desarrollo: **se tapó con el color de fondo** (solo esa franja; el resto es la captura tal cual).

## Revisión de privacidad para publicar

Se revisó `git log -p --all` (29 ramas, 3,5 MB de parches) y los archivos binarios de toda la historia.

| Búsqueda | Resultado |
|---|---|
| Autor/committer de todos los commits | solo `jrsprescott <…+jrsprescott@users.noreply.github.com>`; coautor `noreply@anthropic.com` ✅ |
| Nombre real del usuario, `C:\Users\<nombre>` reales | **ninguno** (solo ejemplos `C:\Users\u`, `C:\Users\X`, `C:\Users\demo` en pruebas) ✅ |
| Correos | solo sintéticos (`deploy@srv.test`, `prueba@example.com`, `*.ejemplo.test`) ✅ |
| Tokens/claves (`ghp_`, `github_pat_`, `sk-…`, `AKIA…`, `PRIVATE KEY`) | ninguno ✅ |
| IPs | `127.0.0.1` y rangos privados de ejemplo (`10.0.0.x`, `10.20.30.40`) ✅ |
| Instituciones (universidad, ministerio, banco, `.gob`, `.edu`…) | ninguna ✅ |
| Rutas de desarrollo | `D:\proy\…`, `D:\algo\…`, `D:\año\…` en pruebas (inventadas) ✅ |
| Binarios en la historia | `ejemplo.pdf` (sin metadatos de autor), `ejemplo.png`, `smoke-agentes.jsonl` (sintético), `kuntur.ico` ✅ |
| Usuarios de GitHub de terceros | solo enlaces a proyectos públicos en `docs/INVESTIGACION.md` (alacritty, wez, zellij-org…) ✅ |
| **Hallazgo** | un comentario de `scripts/smoke.ps1` nombraba la **marca del monitor secundario** del usuario. Dato menor, no identifica a nadie. **Corregido en esta rama** ("el monitor secundario"); sigue en los commits viejos que tocaron `smoke.ps1` (8 apariciones en los parches). |

**Recomendación:** no hace falta reescribir la historia. Si el usuario igual quiere quitar esa marca de la historia, antes de publicar (cambiar `<marca>` por la palabra):
```powershell
pip install git-filter-repo
git clone --mirror https://github.com/jrsprescott/kuntur.git kuntur-limpio
cd kuntur-limpio
"<marca>==>secundario" | Out-File -Encoding ascii ..\reemplazos.txt
git filter-repo --replace-text ..\reemplazos.txt
git push --force --mirror   # reescribe TODAS las ramas: avisar a otras PCs/worktrees y volver a clonar
```
Otras cosas a decidir antes de publicar (no son datos personales): las ramas `fase/*` e `integracion/*` (historia de cada fase; se pueden borrar del remoto o dejar), y que `docs/plans/*` cuenta el proceso con detalle (rutas de capturas en `target\`, PCs del usuario en `ESTADO.md` → "Preparar la PC").

## CI

`.github/workflows/ci.yml`: en ramas `main`, `fase/**`, `integracion/**`, PR y etiquetas `v*`: fmt, clippy `-D warnings` y pruebas en `windows-latest` (todos los crates del workspace). Trabajo `msi` solo en etiquetas `v*`: WiX 3.14.1 (zip oficial con SHA-256 fijo), `cargo-wix 0.3.9`, `scripts/msi.ps1 -Check` y el MSI como artefacto `kuntur-msi`. `permissions: contents: read` (no publica releases). No se pudo ejecutar ni validar con un parser YAML aquí (se revisó a mano).

## Pendientes ⏳ (para el usuario)

- Instalar el MSI en una PC o máquina virtual de prueba: menú Inicio, ícono, **notificación con origen "Kuntur"**, que la barra de tareas agrupe con el acceso, actualizar a una versión mayor y desinstalar limpio.
- Ver el CI en GitHub (primer push de esta rama y una etiqueta `v0.1.0` de prueba si se quiere el artefacto MSI).
- Firmar el MSI y `kuntur.exe` (Authenticode) para que SmartScreen no avise: requiere un certificado de firma de código.
- Aplicar (o no) la propuesta de CPU en reposo después de integrar F10.
- Publicar el repositorio (decisión del usuario) y crear la release.
