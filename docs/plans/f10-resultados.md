# F10 · MCP y comandos — Resultados

Rama `fase/f10-mcp` (sale de `main` con F1–F9). Plan: [`2026-11-23-f10-mcp-y-comandos.md`](2026-11-23-f10-mcp-y-comandos.md). Especificación §4 y §11, y `docs/PLAN.md` (hoja Ctrl+K: MCP por pestaña, perfiles, modelo/esfuerzo/permisos, comandos con atajo).

## Verificación automática ✅

| Verificación | Resultado |
|---|---|
| `cargo test --workspace` | ✅ 388 pruebas (349 de `main` + 39 nuevas; +2 ignoradas a propósito que leen el historial real) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ sin advertencias |
| `cargo fmt --all` | ✅ |
| CI de GitHub | ⏳ sin comprobar desde esta PC |

Lo nuevo con pruebas (39):
- `kuntur-config` (4): `[[mcp]]` stdio/http, `[[mcp_perfiles]]` (servidores + modelo/esfuerzo/permisos), `[[comandos]]` con valores por defecto, y la lista `mcp` / `perfil` de cada `[[sessions]]`; ida y vuelta sin secretos en el archivo.
- `kuntur-mcp` (26 unitarias): validación del catálogo (nombre apto para cada CLI, variables, secreto con valor en texto plano = error, URL), cuentas del Administrador de credenciales, resolución de secretos y de `${KUNTUR_SALIDA}`, activar/desactivar por sesión, perfiles, renombrar/borrar en las listas; JSON de Claude Code, sobrescrituras TOML de Codex (comprobadas con el lector `toml`), JSON V1/V2 de OpenCode; flags de modelo/esfuerzo/permisos por CLI; línea final en PowerShell/cmd/POSIX (`cmd /c` para `npx` y otros `.cmd`); carpeta temporal por sesión y limpieza de huérfanas; capturas nuevas; atajos (reglas y prioridad sesión > global).
- `kuntur-mcp` **integración con servidores MCP falsos** (9): binario `kuntur-mcp-falso` (stdio) → conectado, con ruido no-JSON en stdout, error JSON-RPC, proceso que se cae (código + stderr), tiempo límite; servidor http escrito en la prueba → JSON, **SSE con *chunked* sin esperar el cierre**, 401, puerto cerrado.

### Pruebas de humo (`scripts/smoke.ps1 -Mcp`, servidores **sintéticos**, secretos en memoria)

| Comando | RAM | Qué muestra la captura |
|---|---|---|
| `-Sessions 2 -Mcp probar` (Dark) | 86 MB | Botón MCP resaltado; panel «MCP · PowerShell» con perfiles `godot` (activo: «modelo sonnet · esfuerzo high · permisos: aceptar ediciones») y `web`; `escena-godot` **conectado · kuntur-mcp-falso 0.1.0 (MCP 2025-06-18)**; `base-datos` error «configuración falsa inválida» + «Falta DB_PASSWORD»; `navegador` «no se pudo lanzar»; `docs` (http) «no se pudo conectar» + «Falta el token»; comandos con atajo; detrás, la **captura que dejó el MCP abierta sola en la vista previa** (`captura-falsa.svg`) |
| `-Sessions 1 -Mcp linea -Commands 'cls' -Theme white` | 63 MB | La línea generada, pegada **sin Enter** (nunca se ejecuta): `claude --mcp-config …\<pid>-1\claude.json --model sonnet --effort high --permission-mode acceptEdits` |

Presupuesto < 90 MB ✅ (86 MB con panel + vista previa abiertos: es el caso más pesado).

## Formatos verificados (documentación oficial, 2026-11-23)

| CLI | Cómo recibe los MCP de la sesión | Opciones |
|---|---|---|
| **Claude Code** ([CLI](https://code.claude.com/docs/en/cli-reference), [MCP](https://code.claude.com/docs/en/mcp)) | `--mcp-config <sesión>\claude.json` con `{"mcpServers": {n: {"type":"stdio","command","args","env"} \| {"type":"http","url","headers"}}}`; `--strict-mcp-config` solo con `mcp_exclusivo = true` (ignora `.mcp.json`, `~/.claude.json` y plugins) | `--model`, `--effort low\|medium\|high\|xhigh\|max`, `--permission-mode default\|acceptEdits\|plan\|bypassPermissions` |
| **Codex** ([MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli), [comandos](https://learn.chatgpt.com/docs/developer-commands?surface=cli), [referencia](https://learn.chatgpt.com/docs/config-file/config-reference)) | `-c mcp_servers.<n>={command='…',args=[…],env={…},env_vars=[…]}` / `{url='…',http_headers={…},bearer_token_env_var='…'}` (los valores de `-c` se leen como TOML). **No** se usa un `CODEX_HOME` temporal: Codex guarda ahí la autenticación | `--model`, `-c model_reasoning_effort='…'`, `--sandbox read-only\|workspace-write` + `--ask-for-approval on-request\|never`, `--dangerously-bypass-approvals-and-sandbox` |
| **OpenCode** ([CLI V1, variables](https://opencode.ai/v1/docs/cli/), [MCP V1](https://opencode.ai/v1/docs/mcp-servers/), [MCP actual](https://opencode.ai/docs/mcp-servers/), [permisos](https://opencode.ai/v1/docs/permissions/)) | `OPENCODE_CONFIG=<sesión>\opencode.json` solo para ese comando (se suma a la configuración global, no la reemplaza); V1: `mcp.<n> = {type: local\|remote, command: […], environment, url, headers, enabled}`; V2 (`mcp_opencode_v2 = true`): `mcp.servers.<n>` | `--model proveedor/modelo`, `--agent plan`, `--auto`; `permission` (`edit`/`bash`: `ask`/`allow`) en el archivo (V1). Sin flag de esfuerzo (aviso) |

Otros agentes (Qwen, Gemini, Kimi…): se abren igual, sin MCP, con un aviso.

## Pruebas manuales ⏳ (para el usuario)

- [ ] ⏳ Agregar un MCP real a `config.toml` (`[[mcp]]`), abrir el gestor (botón 🔌, Ctrl+Shift+M o la paleta), activarlo en la pestaña y «Probar» → conectado.
- [ ] ⏳ Un servidor con `secretos = ["X"]`: clic en «Falta X», escribirlo, Enter → queda en el Administrador de credenciales («Kuntur MCP», cuenta `servidor/X`) y nunca en `config.toml`.
- [ ] ⏳ Con el MCP activo, clic en Claude Code instalado → `/mcp` dentro de Claude lo muestra; la configuración global (`~/.claude.json`) no cambia. Igual con Codex (`/mcp`) y OpenCode.
- [ ] ⏳ Cerrar la pestaña → desaparece `%LOCALAPPDATA%\Kuntur\mcp\<pid>-<sesión>\`.
- [ ] ⏳ Un MCP de capturas (p. ej. Godot) configurado con `${KUNTUR_SALIDA}` o la variable `KUNTUR_MCP_SALIDA` → la captura se abre sola en la vista previa y se recarga.
- [ ] ⏳ Un `[[comandos]]` con `atajo = "Ctrl+Shift+1"` y `destino = "acoplada"` → se escribe en la acoplada (la abre si hace falta).
- [ ] ⏳ Perfil con `permisos = "total"` en Codex/Claude: comprobar que el CLI arranca en ese modo.

## Notas y decisiones

- **Estado de MCP (decisión de la especificación):** el servidor lo lanza cada agente; Kuntur no ve su estado. El indicador es el de «Probar» (lanzar / conectar + `initialize`, 15 s máx.): conectado (nombre, versión, protocolo) / error (mensaje del servidor, código de salida y últimas líneas de stderr, HTTP 401…) / detenido. Un proxy MCP queda como mejora.
- **Secretos:** el catálogo solo guarda los **nombres** (`secretos`, `token = true`); los valores van al Administrador de credenciales. Al lanzar, **sí** quedan en texto plano en los archivos temporales de la sesión (`claude.json`, `opencode.json`, `codex-entorno.ps1/.cmd`), dentro del perfil del usuario, y se borran al cerrar la sesión (y los huérfanos al arrancar). En Codex nunca van en la línea (se pasan por `env_vars` / `bearer_token_env_var` desde el script, que se quita después con `Remove-Item Env:…`). Si dos servidores usan el mismo nombre de variable secreta con Codex, gana el último.
- **PowerShell 5.1** pierde las comillas dobles al pasar argumentos a programas nativos: las sobrescrituras de Codex usan cadenas literales TOML (`'…'`).
- **`npx` y otros `.cmd`/`.bat`** se generan como `cmd /c …` (en Windows no se pueden lanzar como programa); «Probar» hace lo mismo y termina el árbol de procesos (`taskkill /T`).
- **Shells POSIX** (WSL, Git Bash) y **pestañas remotas**: el agente se abre sin los MCP de la sesión (aviso); las rutas son de esta PC.
- **Capturas → vista previa:** cada servidor stdio recibe `KUNTUR_MCP_SALIDA` y `${KUNTUR_SALIDA}` se reemplaza en argumentos y entorno. Kuntur mira esa carpeta cada 2 s (solo el primer nivel, ignora `.tmp`/`.part`/ocultos) y abre lo más nuevo (se puede apagar en el pie del panel). **Límite:** solo los MCP que se puedan configurar para escribir en una carpeta; los que devuelven la imagen dentro de la respuesta MCP no se ven (haría falta el proxy).
- **Perfiles:** clic en un perfil aplica sus servidores y sus opciones a la pestaña; volver a pulsarlo quita las opciones (los servidores quedan). `mcp_perfil_inicial` se aplica a las sesiones nuevas.
- **Catálogo:** se edita en `config.toml` (se vuelve a leer al abrir el gestor); en la interfaz: activar/desactivar, perfiles, Probar, escribir secretos. Formulario de alta/edición: pendiente.
- **Comandos con atajo:** solo Ctrl+Shift+tecla (Alt opcional) o una tecla F con modificador; nada de Ctrl+Alt (AltGr en teclados en español) ni los atajos de Kuntur (Ctrl+Shift+T/W/B/E/P/U/A/O/R/M/C/V). Los de la sesión ganan a los globales. `enter = false` solo escribe. Sin entrada propia en la paleta todavía.
- **Atajo Ctrl+Shift+M** (gestor de MCP): libre según la tabla de `integracion-f5-f9.md`. Esc cierra el panel (o cancela el secreto que se está escribiendo).

## Archivos compartidos tocados (para integrar con otras fases)
`crates/kuntur-app/src/workspace.rs` (ganchos: `mod mcp_view`, campo `mcp`, `start_mcp`, restaurar/guardar la lista de cada sesión, `mcp_new_session`/`mcp_close_session`, `agent_launch_line` en `launch_agent`, comandos con atajo y Ctrl+Shift+M en `on_key`, panel en `render`, la paleta cierra el gestor), `workspace/agents_exact.rs` (`agent_launch_line` en el worktree; `show_notice` pasa a `pub(super)`), `workspace/preview_view.rs` (el visor web se oculta con el gestor abierto), `workspace/title_bar.rs` (botón MCP), `command_palette.rs` (`PaletteAction::Mcp`, `ITEMS: [_; 29]`), `main.rs` (ícono `Plug`), `crates/kuntur-app/Cargo.toml`, `crates/kuntur-config/src/{lib.rs,settings.rs}`, `scripts/smoke.ps1` (`-Mcp`, compila también `kuntur-mcp`), `Cargo.lock`, `docs/plans/{README.md,ESTADO.md}`. Nuevos: `crates/kuntur-mcp/` (crate + binario `kuntur-mcp-falso` + `tests/probar.rs`), `crates/kuntur-config/src/mcp.rs`, `workspace/mcp_view.rs`. No se tocó `terminal_view.rs`.
