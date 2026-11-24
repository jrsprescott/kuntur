# Integración F5–F9 — rama `integracion/f5-f9`

Rama `integracion/f5-f9`, creada desde `origin/fase/f5-agentes` (F5a + F5b). Las demás fases se unieron con **merge** (sin rebase, para conservar la historia), una por una y con el árbol en verde después de cada una:

| Orden | Rama | Último commit unido | Commit de merge | Pruebas después |
|---|---|---|---|---|
| base | `fase/f5-agentes` | `764dc35` | — | 222 |
| 1 | `fase/f6-ssh` | `e15da52` | `8dc4fe9` | 267 |
| 2 | `fase/f8-vista-previa` | `bec5ce5` | `e76a1a8` | 293 |
| 3 | `fase/f9-autocompletado` | `45457b9` | `1cdfa87` | 331 |
| 4 | `fase/f7-archivos` (sale de F6) | `e23e849` | `9650de5` | 349 |

En cada paso: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo test --workspace` sin errores ni advertencias. No se tocaron `main` ni las ramas `fase/*`.

## Conflictos importantes y cómo se resolvieron

Regla: **se conservan todas las funciones de todas las fases**; nunca se descartó un lado.

### `kuntur-term`: un solo escáner de OSC (F5 + F6 + F9)
- F5 había separado un escáner genérico (`osc.rs`) que entregaba OSC 7 / 9;9 y OSC 7501; F9 había extendido el escáner viejo de `cwd.rs` con OSC 133 y la posición de cada marca; F6 cambió el lector para que hable con cualquier `Transport` (PTY local o canal SSH).
- Unificado: `OscScanner::feed_at` (en `osc.rs`) entrega cada OSC con la posición **después** de su terminador; `feed` sigue existiendo encima. En `cwd.rs`, `OscEvent::{Cwd, Mark, Agent}` + `OscEvent::parse` y `CwdScanner::feed_events` (lista con posiciones) / `feed` (última carpeta). El lector de `pane.rs` usa ese único escáner: alimenta el emulador por tramos en cada marca 133 (F9), guarda la carpeta (OSC 7 / 9;9) y la señal del agente (OSC 7501, F5), y escribe las respuestas por el `Transport` (F6).
- Se conserva el script de arranque de PowerShell de F9 (OSC 9;9 + 133 `A`/`B`/`D;código`).
- Prueba nueva: `un_solo_escaner_para_carpeta_marcas_y_agentes` (los cuatro tipos en un mismo trozo, en orden y con su posición).

### `kuntur-app`
- **`workspace.rs`**: campos `audit` + `connections` + `preview`; en `Workspace::new` se llaman todos los ganchos de humo (`smoke_connections`, `smoke_preview`, `smoke_files`, además de los de F5); `on_key` con Ctrl+Shift+A, O y R; `run_action` con las acciones de F5 y F6; `render` con la vista de auditoría, el overlay exacto (instalador, avisos), el panel de conexiones y el menú de enlaces. Abrir la paleta cierra el panel de conexiones y carga los agentes instalados (F5).
- **`command_palette.rs`**: `PaletteAction` con las variantes de F5 y F6; `ITEMS: [PaletteItem; 28]`; se conservan las pruebas de ambas fases.
- **`title_bar.rs`**: botones de ícono Terminal acoplada · Tema · Uso · **Auditoría** (F5) · **Conexiones** (F6) · Acciones. `main.rs`: `ExtraIcons = [Command, ChartColumn, ScrollText, Server, Pencil, Trash, Download]`.
- **Agentes del servidor (F6) con el catálogo configurable (F5)**: F5 cambió los agentes instalados de `AgentKind` a ids de texto y quitó `CATALOG`; `connections::remote_agent_kinds` pasó a `remote_agent_ids(&Catalog, nombres)`, que devuelve ids del catálogo (con insignia) y el resto por nombre. `side_panel.rs` usa esos ids en el pie y en el modo compacto.
- **`dock_view.rs`**: la acoplada remota (F6, otro canal en la misma conexión) y la acoplada en el worktree del agente (F5b) conviven en `ensure_dock`; cada terminal nueva se registra para los agentes (F5) y vigila sus enlaces (F8).
- **Visor web (F8)**: además de con la paleta, el uso y el menú de enlaces, el WebView2 se oculta mientras están abiertos la auditoría (F5), el gestor de conexiones (F6) o el instalador de ganchos (F5b) (`ExactAgents::installer_open`).
- **Autocompletado (F9) en pestañas remotas (F6)**: se apaga (`TerminalView::set_remote`). El motor completa rutas del disco de esta PC y guarda historial y carpetas locales: en un servidor sugeriría rutas que no existen allí y mezclaría su historial con el local.
- **`TerminalView::paste_text`** (F5 y F7 lo agregaron con firmas distintas): queda la de F5 (`String` + demora); las rutas "para el agente" de F7 la llaman con `Duration::ZERO`.
- **`settings.rs`**: `alertas` (F5), `autocompletado` (F9), `agentes` (F5) y `conexiones` (F6) juntos, con las pruebas de cada fase.

### Otros
- `Cargo.toml`: `notify`/`similar`/`wry` (F8) y `nucleo-matcher` (F9). `crates/kuntur-app/Cargo.toml`: `kuntur-ssh`, `kuntur-preview`, `kuntur-complete`, `kuntur-audit`, `wry` y la notificación de Windows. `Cargo.lock`: en cada conflicto se tomó el de la rama integrada y `cargo` lo completó.
- `scripts/smoke.ps1`: todos los parámetros y ganchos de F5 (`-Audit`, `-Badges`, `-Launch`, `-Config`, `-AgentEvents`, `-SendSuggestion`, `-HooksInstall`), F6 (`-Connections`), F7 (`-Files`), F8 (`-Preview`, `-PreviewDiff`, `-PreviewEdit`, `-Links`) y F9 (`-Type`, `-History`). Conserva el BOM UTF-8 (`EF BB BF`, comprobado en el archivo y en el blob del commit) y los finales CRLF.
- `docs/plans/ESTADO.md` y `README.md`: combinadas las filas y secciones de cada fase.

## Atajos de teclado (sin choques)

| Atajo | Fase | Qué hace |
|---|---|---|
| Ctrl+` | F4 | Mostrar/ocultar la terminal acoplada |
| Ctrl+Shift+A | F5 | Auditoría de comandos |
| Ctrl+Shift+U | F3 | Uso de agentes |
| Ctrl+Shift+O | F6 | Gestor de conexiones SSH |
| Ctrl+Shift+R | F6 | Reconectar la sesión remota (en una local no hace nada) |
| Ctrl+Shift+E | F2/F7 | Panel "Archivos" (local o remoto según la pestaña) |
| Ctrl+Shift+P / T / W / B, Ctrl+Tab, Ctrl+1…9, Ctrl+= / − / 0 | F2 | Paleta, sesiones, panel, tamaño de letra |
| Ctrl+mouse / Ctrl+clic / clic derecho sobre un enlace | F8 | Subrayar / abrir / menú de enlaces |
| Tab, → (sin modificadores, con sugerencia visible) | F9 | Aceptar el texto fantasma; sin sugerencia llegan a PowerShell |

No hay dos fases con el mismo atajo. **Esc** se resolvió por capas: lo usan primero las capas que están encima de la terminal (paleta, gestor de conexiones y su formulario, buscador de agentes, auditoría, uso) y, **nuevo en la integración**, el menú de enlaces de F8 (era un pendiente de F8); si no hay ninguna abierta, Esc llega a la terminal, donde oculta el texto fantasma de F9 si hay uno y, si no, va al shell (o cancela un diálogo SSH de F6). Ctrl+Tab (ciclar sesiones) no choca con Tab del fantasma, que solo actúa sin modificadores.

## Pruebas

- `cargo fmt --all -- --check` ✅ · `cargo clippy --workspace --all-targets -- -D warnings` ✅
- `cargo test --workspace`: **349 pruebas** ✅ (+2 ignoradas a propósito que leen el historial real). Cuadra con la suma: F5 222 + F6 45 + F8 26 + F9 37 + F7 18 + 1 nueva de la integración (escáner único).

### Pruebas de humo en la rama integrada

Todas en el monitor secundario (pantalla 2), sin tomar el foco ("foco: no lo tomó"), con datos sintéticos y sin notificaciones reales.

| Fase | Comando | RAM | Captura |
|---|---|---|---|
| F5a | `-Sessions 2 -Audit -AuditQuery 'git' -Badges` | 68 MB | Auditoría con búsqueda, filtros y botón resaltado; insignia "1" en la pestaña 2 |
| F5b | `-Sessions 3 -Panel full -AgentEvents scripts\smoke-agentes.jsonl` | 65 MB | Subagentes con su acción, "pide permiso" con Permitir/Rechazar, "terminó" con Enviar a terminal |
| F6 | `-Sessions 1 -Connections list` | 70 MB | Panel "Conexiones" con las cuatro conexiones sintéticas bajo su botón |
| F7 | `-Sessions 1 -Connections remote -Files remote -Theme dark` | 71 MB | Pestaña remota (franja "Desconectado · Reconectar"), árbol remoto en memoria con "remoto · demo@127.0.0.1:1", zona del agente y cola (43 %, conflicto, listo) |
| F8 | `-Sessions 1 -Theme dark -Preview '…\ejemplo.md;…\ejemplo.svg'` | 68 MB | Zona central dividida, dos pestañas, SVG dibujado |
| F9 | `-Sessions 1 -Commands 'cls' -Type 'cd C:\Program'` | 63 MB | `cd C:\Program` y en gris `→ 'C:\Program Files\'` |
| F4 | `-Sessions 2 -Commands 'cd C:\Windows' -Dock reopen -DockCommands 'Get-Location'` | 63 MB | Acoplada reabierta en `C:\Windows` con su historial; rayas de bloque (F9) en ambas |
| F3 | `-Sessions 1 -Usage 30 -Theme dark` | 72–73 MB | Uso de agentes (30 días) completo |

Ninguna captura muestra problemas visuales; la barra de título tiene los seis botones de ícono. RAM entre 63 y 73 MB (presupuesto < 90 MB).

## Pendientes

- Las pruebas manuales ⏳ de cada fase siguen como estaban (`f5-resultados.md`, `f6-resultados.md`, `f7-resultados.md`, `f8-resultados.md`, `f9-resultados.md`), ahora sobre la rama integrada.
- Enlaces (F8) en pestañas remotas: no se activan (solo archivos locales; los remotos necesitan abrir por SFTP, F7 + F8).
- Autocompletado remoto (F9 sobre F6): apagado en pestañas remotas; haría falta completar por SFTP y un historial por servidor.
- OSC 7501 de agentes remotos: el escáner ya lo lee en cualquier transporte, pero el panel de agentes de una pestaña remota no lo usa todavía.
- Vista previa de archivos remotos desde el árbol de F7 (F8 sigue siendo solo local).
- Decisión del usuario sobre el panel izquierdo (agentes arriba + árbol abajo, especificación §3), abierta desde F7.
