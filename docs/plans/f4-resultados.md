# F4 · Terminal acoplada — Resultados

Rama `fase/f4-acoplada` (sale de `fase/f3-v5`). Plan: [`2026-11-23-f4-terminal-acoplada.md`](2026-11-23-f4-terminal-acoplada.md). Criterio de [`PLAN.md`](../PLAN.md) §6: *"Abrir/cerrar sin perder historial; sigue la ruta de la sesión"*.

## Verificación automática ✅

| Verificación | Resultado |
|---|---|
| `cargo test --workspace` | ✅ 151 pruebas (+2 ignoradas a propósito que leen el historial real y solo imprimen totales) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ sin advertencias |
| `cargo fmt --all -- --check` | ✅ |
| CI de GitHub | ⏳ sin comprobar desde esta PC (falta `gh auth login`) |

Lo nuevo con pruebas (12): `dock.rs` (estado inicial, Ctrl+` en sus tres casos, sin foco cuando está oculta, cierre por `exit` conservando el alto, proporción guardada y valores rotos, límites del separador, área chica o rota, carpeta inicial, Ctrl+` / Ctrl+Ñ), `SessionList::get/get_mut`, `dock_ratio` opcional en archivos viejos y la acción de la paleta.

### Pruebas de humo (`scripts/smoke.ps1`)

| Comando | RAM | Qué muestra la captura |
|---|---|---|
| `-Sessions 2 -Commands 'cd C:\Windows\Media' -Dock reopen -DockCommands 'Write-Host acoplada-ok …','Get-Location'` (Dark) | 66 MB | La acoplada abajo (~35 %), encabezado "Terminal · PowerShell · C:\Windows\Media" con ×, arrancó en la carpeta de la sesión; después de **ocultarla y volver a mostrarla** conserva todo lo escrito |
| Lo mismo con `-Theme blue` | 69 MB | Igual sobre el degradado Blue; el botón de la barra resaltado; cursor **hueco** en la principal (sin teclado) y lleno en la acoplada |
| `-Sessions 1 -Theme white -Panel full -Commands 'cd C:\Windows' -Dock open -DockCommands 'dir …'` | 66 MB | Panel de agentes completo + acoplada en `C:\Windows`; línea inferior con la carpeta del panel con foco |

Presupuesto de F4: < 100 MB ✅. El shell de la acoplada es otro proceso (pwsh + conhost) y no suma a la RAM de Kuntur.

## Pruebas manuales ⏳ (para el usuario)

Abrir `target\release\kuntur.exe` normal.

- [ ] ⏳ **Ctrl+`** (teclado de EE. UU.) o **Ctrl+Ñ** (teclado en español) abre la acoplada en la carpeta actual de la sesión; con la principal enfocada la enfoca; con la acoplada enfocada la oculta y el teclado vuelve a la principal.
- [ ] ⏳ Ocultar y volver a mostrar no pierde el historial ni lo que corre (por ejemplo `ping -t localhost`).
- [ ] ⏳ Arrastrar el separador cambia el alto (mínimo ~110 px la acoplada y ~140 px la principal); cerrar y volver a abrir Kuntur recuerda el alto de cada sesión.
- [ ] ⏳ Clic en un panel le da el teclado (el cursor lleno muestra cuál); al cambiar de pestaña y volver, el teclado vuelve al último panel usado.
- [ ] ⏳ `exit` en la acoplada la cierra y la próxima Ctrl+` abre una nueva; cerrar la sesión cierra también su acoplada (sin procesos `pwsh` huérfanos en el Administrador de tareas).
- [ ] ⏳ Desde la acoplada siguen funcionando Ctrl+Shift+T/W/P/U/B/E, Ctrl+Tab, Ctrl+1…9, Ctrl+= / Ctrl+-.
- [ ] ⏳ El botón de la barra (ícono de panel inferior) y "Terminal acoplada: mostrar/ocultar" en la paleta hacen lo mismo que Ctrl+`.
- [ ] ⏳ Cambiar de tema o de tamaño de letra cambia también la acoplada (aunque esté oculta).

## Notas y decisiones

- **Ctrl+` como VS Code:** cerrada → se abre y enfoca; abierta sin teclado → se enfoca; abierta con teclado → se oculta. Es la única excepción a "atajos con Ctrl+Shift" y la pide el plan.
- **Ctrl+Ñ:** GPUI informa la tecla según la distribución; en los teclados en español la tecla física del acento grave de EE. UU. es la Ñ (VS Code hace lo mismo). Ctrl+Alt (AltGr) no cuenta, para no chocar con caracteres escritos con AltGr.
- **"Sigue la ruta de la sesión"** = arranca en la carpeta actual de la sesión. Al volver a mostrarla no se escribe ningún `cd` en su shell (sería ejecutar algo por el usuario); si se quiere la carpeta nueva, `exit` y Ctrl+` abre otra.
- **Mismo shell que la sesión** (`PLAN.md` §5.3): si la sesión usa un shell guardado, la acoplada usa ese.
- El alto se guarda como proporción por sesión (`dock_ratio` en `config.toml`, solo si no es el de siempre, 0,35). Se escribe al soltar el separador.
- Los agentes que se abran **en la acoplada** no aparecen en el panel de agentes (se vigila el shell principal de cada sesión): la acoplada es "sin IA" por diseño. Revisar en F5.
- **Pendiente para F5:** abrir la acoplada en el **worktree de un agente** (selector "en:" y "↧ Terminal aquí"), "Enviar a terminal". Para F6: canal SSH acoplado en la misma conexión.
- Sin hacer (no lo pide el criterio): maximizar y limpiar desde el encabezado (la simulación los muestra; `cls` ya limpia).
