# F5 · Agentes exactos, alertas y auditoría — Plan

Rama `fase/f5-agentes` (sale de `fase/f4-acoplada`). Fila F5 de [`README.md`](README.md); especificación §3, §5, §12 y §13 ([`ESPECIFICACION.md`](../ESPECIFICACION.md)).
La fase se parte en dos: **F5a** (esta) no necesita hooks; **F5b** agrega el estado exacto.

## F5a · Catálogo configurable, abrir agentes, alertas y auditoría

### 1. Catálogo de agentes configurable (§5) — `crates/kuntur-agents`
- `catalog.rs` pasa de una tabla fija a un `Catalog` con `AgentDef` (id, nombre, comando, argumentos, nombres de ejecutable y de script, letra, color, `usage: Option<AgentKind>` para los que tienen lector de uso, `ide`).
- Se agregan **Kimi** (`kimi`), **DeepSeek** (`deepseek`, CLIs de terceros) y **Antigravity** (`antigravity`, **IDE**: se abre con `.`; no cuenta como "agente corriendo" en el árbol de procesos).
- Agentes extra por `config.toml` (`[[agentes]]`: `nombre`, `ejecutable`, `argumentos`, `letra`, `color` `#RRGGBB`, `registros` reservado). Si el ejecutable es el de uno del catálogo, lo ajusta (argumentos, letra, color, nombre) en vez de duplicarlo.
- Instalados: al iniciar, cada 30 s y **bajo demanda** (paleta: "Agentes: actualizar instalados", que también relee `config.toml`). Un ejecutable con ruta completa se comprueba directo.
- Pruebas: catálogo completo, extra nuevo / que ajusta uno del catálogo / inválido, color y letra, detección en el PATH, coincidencia de procesos (IDE excluido).

### 2. Clic en un agente instalado = abrirlo (§3)
- `launch.rs` (puro): línea de comando con comillas seguras según el shell (PowerShell con `&` si el programa va entre comillas; cmd; sh). Pruebas.
- Clic en la insignia (o "Agentes: abrir X" en la paleta) escribe la línea + Enter en la terminal con foco de la sesión activa. Si esa terminal está **ocupada** (su shell tiene procesos hijos, p. ej. otro agente), se abre una sesión nueva y se escribe ahí: nunca se manda texto a un programa que no es el shell.

### 3. Agentes de la terminal acoplada (pendiente de F4)
- El monitor vigila también el shell de cada acoplada; la fila dice "sesión · acoplada". El estado sale de la actividad de esa terminal.

### 4. Alertas (§12)
- `alerts.rs` (puro, en `kuntur-agents`): `AlertTracker` compara lecturas; trabajando → esperando (confirmado en 2 lecturas seguidas, ~4–6 s sin salida) o el proceso del agente termina, en una pestaña **no visible** (no activa, otra vista abierta o la ventana sin foco) ⇒ alerta. La primera lectura de un agente nunca alerta; cerrar la sesión no alerta.
- Insignia con contador en la pestaña; se limpia al verla (activar la pestaña o volver a la ventana con ella activa).
- Notificación de Windows con `tauri-winrt-notification` 0.8.1, en un hilo aparte. `alertas = true` por defecto en `config.toml`. **Nunca** se muestra si existe cualquier variable `KUNTUR_SMOKE_*` ni desde pruebas (`cfg!(test)`).

### 5. Auditoría (§13) — crate nuevo `crates/kuntur-audit` (sin UI)
- `AuditRecord { ts, agent, session, host: "local", folder, command }`.
- Lectores puros con pruebas sintéticas: Claude Code (`tool_use` `Bash`/`PowerShell` → `input.command`, sin repetir por `id`) y Codex (`function_call` `shell_command`/`shell`/`exec_command`, `local_shell_call`; campos confirmados imprimiendo solo claves).
- `cache.rs`: caché incremental en memoria por archivo (tamaño + fecha), mismo modelo que el almacén de `kuntur-usage`.
- `filter.rs` (texto sin tildes, agente, período) y `export.rs` (CSV con BOM, a prueba de fórmulas).
- `synthetic.rs`: datos sintéticos para las capturas.
- Vista "Auditoría" en `kuntur-app` (`workspace/audit_view.rs`): botón de ícono arriba a la derecha (abre/cierra), paleta (abrir y exportar), Esc vuelve; búsqueda, filtro por agente y período, lista con `uniform_list`, exportar CSV. `KUNTUR_SMOKE_AUDIT` usa datos sintéticos; `scripts/smoke.ps1 -Audit`.

### Verificación
`cargo fmt`, `clippy -D warnings`, `cargo test --workspace`; humo con `-Audit` y con agentes/insignias; resultados en `f5a-resultados.md`.

## F5b · Estado exacto, permisos, subagentes, worktrees y línea de estado

Fuentes confirmadas en la documentación oficial de hooks de Claude Code (2026-11-23): campos comunes
`session_id`, `cwd`, `hook_event_name` y, dentro de un subagente, `agent_id` / `agent_type`;
`PreToolUse`/`PostToolUse` (`tool_name`, `tool_input`, `tool_use_id`), `PostToolUseFailure`,
`PermissionRequest`, `Notification` (`message`, `notification_type`: `permission_prompt`, `idle_prompt`,
`elicitation_dialog`…), `UserPromptSubmit`, `Stop` y `SubagentStop` (`last_assistant_message`),
`SubagentStart`, `SessionStart`, `SessionEnd`. Solo el código de salida 2 bloquea: el gancho siempre
sale con 0 y sin escribir nada en stdout (en `UserPromptSubmit` lo escrito se agregaría al contexto).
La línea de estado recibe `rate_limits.five_hour` / `seven_day` (`used_percentage`, `resets_at`).

### 1. Crate nuevo `crates/kuntur-hook` (biblioteca + binario `kuntur-hook.exe`)
- `event.rs` (puro): `HookEvent` compacto (`pane`, `kind`, `agent_id`, `agent_type`, `tool`, `detail`,
  `notification`, `message` recortado a 16 KB, límites de la línea de estado). `from_claude(json)`
  convierte la entrada de Claude Code; `summary` arma "Bash · npm test", "Read · main.rs",
  `Grep · "patrón"` (recortado). Codificación: una línea JSON.
- `pipe.rs` (Windows): servidor de named pipe `\\.\pipe\kuntur-<pid>` (primera instancia exclusiva,
  rechaza clientes remotos; un hilo lee mensaje por conexión y los entrega por un canal) y cliente
  (`send`: abre, escribe y cierra; si el pipe no existe vuelve enseguida, si está ocupado espera
  como mucho 2 × 100 ms). Pruebas de integración reales: servidor y cliente en el mismo proceso
  (varios mensajes, varios clientes a la vez, pipe inexistente sin bloquear).
- `install.rs` (puro): plan de instalación sobre el texto de `settings.json`: agrega un gancho
  `"<ruta>/kuntur-hook.exe"` (timeout 5 s) en cada evento usado, sin duplicar si ya está, conserva
  el resto y el orden de las claves; agrega `statusLine` solo si no hay otra (si la hay, no la toca y
  lo dice). `apply` hace copia `settings.json.kuntur-<fecha>.bak` y escribe. Pruebas sobre archivos
  temporales: nunca contra `~/.claude`.
- `main.rs`: `kuntur-hook` (gancho) y `kuntur-hook statusline`. Sin `KUNTUR_PIPE` / `KUNTUR_SESSION`
  no hace nada. Nunca falla de forma visible (código 0, sin stderr).
- Kuntur pasa a cada shell `KUNTUR_PIPE` y `KUNTUR_SESSION` (un identificador por terminal, principal
  o acoplada) con `PaneOptions.env`.

### 2. OSC 7501 — `kuntur-term`
- `osc.rs`: escáner genérico de OSC (el de OSC 9;9 pasa a usarlo). `agent_signal.rs` (puro): formato
  propio de Kuntur `ESC ] 7501 ; state=<working|permission|waiting|done|end> [; agent=<id>]
  [; action=<texto>] (BEL | ST)` (valores sin `;`). `Pane::take_agent_signal()` entrega el último.
- Respaldo de los hooks y única fuente para agentes sin hooks (y, en F6, por SSH).

### 3. Estado exacto — `kuntur-agents/src/exact.rs` (puro)
- `ExactTracker` por terminal: `Working` / `Permission` / `Reply` / `Done`, acción actual, permiso
  pendiente (herramienta y detalle), subagentes (`agent_id` → tipo, acción, estado, desde), último
  mensaje y comandos sugeridos. Reglas: `UserPromptSubmit`/`PreToolUse` → trabajando; `PostToolUse`
  limpia la acción; `PermissionRequest` o `Notification` `permission_prompt` → esperando permiso;
  `idle_prompt`/`elicitation_dialog` → esperando respuesta; `Stop` → terminado; `SubagentStart/Stop`
  y eventos con `agent_id` → filas de subagentes; `SessionEnd` → se olvida.
- `AgentState` sigue siendo el estado grueso (trabajando / esperando) para filtros y pestañas; el
  exacto se agrega a la fila. La heurística por actividad queda solo para agentes sin eventos.
- Alertas: `Observation.exact` → con estado exacto se avisa en la primera lectura en espera (sin la
  confirmación de 2 lecturas); `AlertKind::Permission` ("pide permiso").

### 4. Panel de agentes
- Fila: "sesión · Bash · npm test" mientras trabaja; "pide permiso" / "espera respuesta" /
  "terminó" en ámbar. Debajo, una fila de ~22 px por subagente (tipo y acción).
- Permiso pendiente: botones **Permitir** / **Rechazar** en la fila (`permission.rs`, puro: Claude
  Code `1` / Esc, Codex `y` / Esc; otros agentes sin botones). Solo por clic.
- Comando sugerido (bloques ```bash/sh/powershell/pwsh/cmd del último mensaje, `suggest.rs` puro, con
  aviso para `sudo`, `rm -rf`…): **Copiar** / **Enviar a terminal** (se escribe en la acoplada, que se
  abre si hace falta, **sin Enter**). Paleta: "Terminal: enviar selección a la acoplada" y "Terminal:
  enviar el comando sugerido".

### 5. Worktree por agente (+ acoplada en el worktree, pendiente de F4)
- `worktree.rs` (puro): carpeta hermana `<repo>-<agente>-<n>` y rama `kuntur/<agente>-<n>` libres.
  Paleta: "Agentes: abrir X en un worktree nuevo" → `git worktree add -b …` en segundo plano, sesión
  nueva en esa carpeta y el agente abierto ahí; la acoplada de esa sesión arranca en el worktree.
  Prueba real con un repositorio temporal.

### 6. Línea de estado de Claude
- `kuntur-hook statusline` reenvía los límites y escribe una línea corta (modelo · contexto %).
  El anillo de Claude Code en "instalados" usa el último dato (5 h; tooltip con semanal).
- "Claude Code: instalar ganchos de Kuntur" (paleta): ventana que muestra exactamente lo que se
  agrega a `~/.claude/settings.json`, dónde queda la copia de seguridad y solo escribe al pulsar
  **Instalar**. `KUNTUR_CLAUDE_SETTINGS` lo redirige (pruebas de humo: archivo temporal).

### 7. Humo
- `scripts/smoke.ps1 -AgentEvents <archivo>`: eventos sintéticos (líneas JSON con `"pane":"#1"`) que la
  app manda **por el named pipe real**; `-HooksInstall` abre la ventana del instalador contra un
  `settings.json` temporal. Nunca se abre `claude` real.

### Fuera de F5b
- Auditoría desde los hooks (comandos de cualquier agente con hook): queda para después si no entra.
