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

## F5b · (siguiente) Estado exacto
Esqueleto, se detalla al empezarla:
- `kuntur-hook.exe` + named pipe; OSC 7501; estado exacto esperando/trabajando (reemplaza la heurística por actividad).
- Permitir desde la fila; subagentes con su acción actual.
- Worktree por agente (y la acoplada en el worktree: pendiente de F4); "Enviar a terminal".
- Puente de la línea de estado de Claude (anillo 5 h / semanal, con permiso del usuario).
- Auditoría también desde los hooks (comandos de cualquier agente con hook).
