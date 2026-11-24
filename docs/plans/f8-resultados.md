# F8 · Vista previa y diff — Resultados

Rama `fase/f8-vista-previa` (sale de `fase/f4-acoplada`). Plan: [`2026-11-23-f8-vista-previa-y-diff.md`](2026-11-23-f8-vista-previa-y-diff.md). Especificación §9 (vista previa) y §14 (diff a colores). Alcance: **solo archivos locales** (los remotos por SFTP llegan con F7). Spike del visor web: [`f8-spike-wry.md`](f8-spike-wry.md).

## Verificación automática ✅

| Verificación | Resultado |
|---|---|
| `cargo test --workspace` | ✅ 177 pruebas (+2 ignoradas a propósito que leen el historial real y solo imprimen totales) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ sin advertencias |
| `cargo fmt --all -- --check` | ✅ |
| CI de GitHub | ⏳ sin comprobar desde esta PC |

Lo nuevo con pruebas (26):
- `kuntur-term/src/links.rs` (9): URLs con puntuación final, `?` y paréntesis balanceados; rutas absolutas `C:\…` y `C:/…`, el prompt `PS C:\x>`, `--out=C:\…`; entre comillas con espacios; relativas (`src\main.rs`, `.\a.md`, `..\b`, `nombre.ext`); lo que **no** es ruta (`1.5`, `v0.11`, `--help`, `C:` suelto); sufijos `:12`, `:12:5`, `(12,5)`, `(40)`; varias por línea; columnas con caracteres anchos (CJK); resolución contra la carpeta de la sesión **solo si existe**, `..`, prefijos `a/` `b/` de `git diff`.
- `kuntur-preview` (14): tipo por extensión y por contenido (UTF-16 con BOM = texto); decodificación UTF-8/BOM/UTF-16; Markdown, binario, carpeta e inexistente; truncado a 2 MB; archivos de ejemplo del repo; parseo de `git diff` (meta, hunks, números viejo/nuevo, contexto vacío, `\ No newline`, +/−); diff entre dos versiones (`similar`); **`git diff` real** en un repo temporal (fuera de repo, sin cambios, con cambios); vigilancia con `notify` (aviso al modificar, carpetas por conteo); detección de LibreOffice (PATH y Program Files); URL `file:///` codificada.
- `kuntur-app` (3): proporción del separador de la vista previa (límites y valores rotos), tamaños legibles, formato de imagen por extensión.

### Pruebas de humo (`scripts/smoke.ps1`)

| Comando | RAM de Kuntur | Qué muestra la captura |
|---|---|---|
| `-Sessions 1 -Theme dark -Preview '…\ejemplo.md;…\ejemplo.svg'` | 80 MB | Zona central dividida: terminal \| vista previa con dos pestañas; SVG dibujado; barra con ruta, "Cambios", abrir con su programa, mostrar en carpeta, cerrar |
| `-Sessions 1 -Theme blue -Preview '…\ejemplo.md'` | 70 MB | Markdown renderizado (títulos, negrita/cursiva, código en línea, listas, bloque de código, tabla, cita) sobre el degradado Blue |
| `-Preview '…\ejemplo.md' -PreviewEdit -PreviewDiff` | 64 MB | El script modifica la copia (una línea cambiada y cuatro nuevas): **recarga automática** y diff "contra la versión anterior que se vio" con +5 −1, fondos verde/rojo y números viejo \| nuevo |
| `-Commands "cd '<datos>'",'Get-ChildItem -Name','Write-Host "ver .\ejemplo.rs:4 y https://example.com/docs?x=1."' -Links` | 63–67 MB | Rutas y URLs **subrayadas** (prompt, nombres de archivo, `.\ejemplo.rs:4`, la URL sin el punto final; `-Name` y "ver" no) y el **menú contextual** de la primera ruta (una carpeta: "Abrir carpeta" / "Copiar ruta") |
| `-Preview '…\ejemplo.pdf'` | 67–68 MB | PDF en el **WebView2 integrado** (visor de Edge con su barra), en el rectángulo exacto de la vista previa |

Presupuesto: < 100 MB ✅ (Kuntur no cuenta la RAM de WebView2, que vive en sus propios procesos: ~141 MB privados con HTML, ~256 MB con PDF, 0 al cerrar; ver el spike). Los archivos de prueba son sintéticos (`crates/kuntur-preview/tests/datos/`); `-PreviewEdit` modifica una copia en `target\smoke-preview\`.

## Pruebas manuales ⏳ (para el usuario)

- [ ] ⏳ Con **Ctrl** pulsado, pasar el mouse por una ruta o URL de la salida la subraya y pone la mano; al soltar Ctrl se quita (también sin mover el mouse).
- [ ] ⏳ **Ctrl+clic** en una URL abre el navegador; en un archivo, la vista previa (si tenía `:línea`, la línea queda resaltada en texto/código; todavía no se desplaza sola hasta ella); en una carpeta, el Explorador. Sin Ctrl el clic sigue seleccionando (y en nvim/htop sigue yendo al programa).
- [ ] ⏳ **Clic derecho** sobre una ruta (sin selección): "Vista previa", "Abrir con su programa", "Mostrar en carpeta", "Copiar ruta"; sobre una URL: "Abrir en el navegador", "Copiar URL". Fuera de un enlace, el clic derecho sigue copiando/pegando.
- [ ] ⏳ Arrastrar el separador vertical cambia el ancho (20–80 %).
- [ ] ⏳ Pestañas: abrir el mismo archivo otra vez activa su pestaña; × cierra; el × de la barra cierra toda la vista previa.
- [ ] ⏳ Editar un archivo abierto (o que lo edite un agente) lo recarga solo; "Cambios" muestra el `git diff` si el archivo está en un repo con cambios, si no el diff contra la versión anterior.
- [ ] ⏳ PDF/HTML: el visor se ve en su sitio; al abrir la paleta (Ctrl+Shift+P), el menú de enlaces o "Uso de agentes" se oculta y vuelve al cerrarlos; al cerrar la última pestaña PDF/HTML desaparecen los `msedgewebview2.exe` del Administrador de tareas.
- [ ] ⏳ **Teclado tras hacer clic en el PDF**: hacer clic en la terminal devuelve el teclado (Ctrl+Shift+… y escribir funcionan).
- [ ] ⏳ Pantalla al 125–150 %: el visor web sigue en su rectángulo.

## Notas y decisiones

- **Detección pura** en `kuntur-term::links` sobre las celdas de `Screen` (la foto de la cuadrícula de `alacritty_terminal`), con columnas exactas. Una ruta solo se subraya si **existe** (absoluta o relativa a `Pane::cwd()`, la carpeta por OSC 9;9), así "e.g." o versiones no se marcan. La detección corre solo con Ctrl pulsado (o en la prueba de humo).
- **Sin Ctrl, nada cambia** salvo el clic derecho **sobre un enlace** (sin selección y sin modo mouse del programa), que abre el menú en vez de pegar.
- **Vista previa por ventana**, no por sesión: las pestañas siguen visibles al cambiar de sesión. Se cierra con su × (no se guarda en `config.toml`).
- **Diff:** primero `git diff HEAD -- archivo` (si el repo no tiene commits, contra el índice); si no hay cambios en git, contra la última versión que se vio en la pestaña (`similar`, 3 líneas de contexto). `git` se lanza sin ventana de consola.
- **Recarga:** `notify` vigila la carpeta de cada archivo abierto (los guardados atómicos renombran) y junta avisos durante 150 ms. Las imágenes se leen a memoria (`Image::from_bytes`) para que la caché de GPUI no muestre la versión vieja.
- **Texto grande:** se leen como máximo 2 MB ("solo el comienzo"); texto y diff usan lista virtual (`uniform_list`).
- **Visor web integrado** (el spike funcionó): un único WebView2 por ventana, creado al abrir una pestaña PDF/HTML y destruido al cerrar la última; oculto mientras haya algo de Kuntur que deba dibujarse encima; `with_focused(false)`; datos en `%LOCALAPPDATA%\Kuntur\WebView2`.
- **Office:** si LibreOffice está instalado (`soffice.exe` en el PATH o en Program Files) se convierte a PDF en segundo plano en `%TEMP%\kuntur-preview\<n>\` (se borra al cerrar la pestaña) y se muestra en el visor web; si no, un aviso y "Abrir con su programa". En esta PC **no** está instalado: la conversión no se probó con un documento real.
- **Gancho mínimo en archivos compartidos:** `terminal_view.rs` (submódulo `terminal_view/links.rs`, 3 campos, 2 llamadas y el subrayado), `workspace.rs` (módulo, campo, humo, envolver el área central, menú de enlaces), `workspace/dock_view.rs` (suscribir los enlaces de cada terminal nueva).
- **Pendiente:** archivos remotos por SFTP y su recarga por fecha (F6/F7); enlaces partidos por el salto de línea de la terminal; resaltado de sintaxis; desplazarse hasta la `:línea` pedida; Esc para cerrar el menú de enlaces; "descargar" (solo tiene sentido con remotos).
