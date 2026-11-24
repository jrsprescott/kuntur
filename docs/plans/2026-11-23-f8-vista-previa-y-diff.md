# F8 · Vista previa y diff — Plan de implementación

> **Para agentes:** ejecutar tarea por tarea (método de F1–F4). Casillas `- [ ]`.

**Objetivo:** las rutas y URLs de la salida de la terminal se vuelven clicables y abren una **vista previa** que divide la zona central (terminal a la izquierda, vista previa a la derecha, con pestañas). Especificación: [`ESPECIFICACION.md`](../ESPECIFICACION.md) §9 (vista previa) y §14 (diff a colores). Esta rama sale de `fase/f4-acoplada` y cubre **solo archivos locales**: los remotos por SFTP llegan con F7.

**Qué entra:**
1. **Enlaces en la salida (§9):** rutas absolutas de Windows (`C:\…`, también entre comillas con espacios), relativas a la carpeta de la sesión (`Pane::cwd()`), con `:línea[:columna]` o `(línea,columna)` opcional, y URLs `http(s)://`. Con **Ctrl** pulsado se subrayan al pasar el mouse; **Ctrl+clic** las abre (URL → navegador; ruta → vista previa). Sin Ctrl todo sigue igual (selección o mouse del programa). **Clic derecho** sobre una ruta → menú "Vista previa" / "Abrir con su programa" / "Copiar ruta" (y "Mostrar en carpeta"); sobre una URL → "Abrir en el navegador" / "Copiar URL".
2. **Panel de vista previa:** crate nuevo `kuntur-preview` (lógica pura: tipo de archivo, lectura con límites, diff, vigilancia) + `workspace/preview_view.rs`. Divide la zona central con un separador arrastrable; pestañas para varios archivos; botones: cambios (diff), abrir con su programa, mostrar en carpeta, cerrar.
3. **Formatos:** Markdown (`TextView::markdown`), imágenes PNG/JPG/GIF/SVG/WebP/BMP (`img` de GPUI), texto/código con números de línea, y **diff a colores (§14):** `git diff` del archivo si está en un repo; si no hay cambios en git, diff contra la versión anterior que se vio (crate `similar`). Parseo de diff unificado puro y con pruebas.
4. **Recarga automática:** `notify` vigila la carpeta de cada archivo abierto (las escrituras atómicas renombran) y recarga con una pequeña espera para juntar ráfagas.
5. **Spike `wry`** (`spikes/wry-gpui`, fuera del workspace): WebView2 como ventana hija de la ventana GPUI vía `HasWindowHandle`; medir RAM de los procesos `msedgewebview2.exe`, posición, redimensionar, z-order con menús. Resultado en [`f8-spike-wry.md`](f8-spike-wry.md). Se integra para PDF/HTML **solo si funciona bien**; si no, PDF/HTML → "Abrir con su programa".
6. **Office** (PPTX/DOCX/XLSX): convertir a PDF con LibreOffice solo si está instalado (se detecta) y solo si el visor web está integrado; si no, "Abrir con su programa".

**Fuera de F8 (pendiente):** archivos remotos por SFTP a temporal y su recarga por fecha (F7/F6); enlaces que cruzan un salto de línea de la terminal; resaltado de sintaxis.

---

## Archivos

| Archivo | Acción | Responsabilidad |
|---|---|---|
| `crates/kuntur-term/src/links.rs` | Crear | Detección pura de URLs y rutas en una fila de la cuadrícula (columnas exactas), sufijos de línea, resolución contra la carpeta de la sesión (TDD) |
| `crates/kuntur-preview/` | Crear | `kind` (tipo por extensión y contenido), `load` (lectura con límites, binarios), `diff` (parseo de diff unificado, diff entre textos, `git diff`), `watch` (`notify`), `office` (detectar LibreOffice) |
| `crates/kuntur-app/src/terminal_view/links.rs` | Crear | Subrayado con Ctrl, Ctrl+clic, clic derecho sobre un enlace → eventos `LinkEvent` |
| `crates/kuntur-app/src/terminal_view.rs` | Gancho mínimo | Llamar a los manejadores del submódulo y dibujar el subrayado |
| `crates/kuntur-app/src/workspace/preview_view.rs` | Crear | Estado de la vista previa, pestañas, separador, dibujo por formato, menú contextual de enlaces, recarga |
| `crates/kuntur-app/src/workspace.rs` | Gancho mínimo | Campo `preview`, envolver el área de terminales, gancho de humo |
| `scripts/smoke.ps1` | Modificar | `-Preview <ruta>` (`KUNTUR_SMOKE_PREVIEW`), `-PreviewDiff` |
| `crates/kuntur-preview/tests/datos/` | Crear | Archivos de ejemplo sintéticos (Markdown, SVG, PNG, texto, diff) |
| `spikes/wry-gpui/` | Crear | Spike del visor web |

## Tareas

- [x] **Task 1 — `links.rs` (TDD).** URL con puntuación final y paréntesis balanceados; ruta absoluta `C:\…` y `C:/…`; entre comillas con espacios; relativa (`src\main.rs`, `.\a.md`, `..\b`, `nombre.ext`); sufijos `:12`, `:12:5`, `(12,5)`; el prompt `PS C:\x>`; resolución: absoluta o relativa a la carpeta y solo si existe; columnas con caracteres anchos.
- [x] **Task 2 — `kuntur-preview` (TDD).** Tipo de archivo; texto con límite de tamaño y detección de binarios; parseo de diff unificado (encabezados, hunks, números de línea, "\ No newline"); diff entre dos textos; `git diff` en un repo temporal (si `git` existe); detección de LibreOffice.
- [x] **Task 3 — Enlaces en la terminal.** Subrayado con Ctrl (también al pulsar Ctrl sin mover el mouse), cursor de mano, Ctrl+clic, clic derecho → menú.
- [x] **Task 4 — Vista previa.** Pestañas (reabrir el mismo archivo lo activa), separador con proporción 0,25–0,75, botones, formatos, diff, recarga con `notify`.
- [x] **Task 5 — Spike `wry`.** Medir y documentar; integrar solo si funciona bien.
- [x] **Task 6 — Humo y verificación.** `smoke.ps1 -Preview`; capturas (Markdown, imagen, diff) con RAM; `fmt`, `clippy`, `test`; `f8-resultados.md`, `ESTADO.md`, `README.md`.

## Pruebas manuales que quedan para el usuario
Ctrl+clic con el mouse real sobre rutas y URLs, clic derecho sobre una ruta, arrastrar el separador, editar un archivo abierto y ver la recarga, abrir un archivo de un repo con cambios y ver el diff.
