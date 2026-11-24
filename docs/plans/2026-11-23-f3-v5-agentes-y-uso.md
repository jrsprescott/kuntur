# F3 · Diseño v5: pestañas, panel de agentes y uso — Plan de implementación

> **Para agentes:** ejecutar tarea por tarea con revisión entre bloques (método de F1/F2). Casillas `- [ ]`.

**Objetivo (pedido del usuario tras F2):**
1. Las **sesiones pasan a pestañas arriba** (en la barra de título), con `+` para nueva sesión.
2. A la izquierda, un **panel de agentes**: buscador, filtros **Todos / Esperando / Trabajando**, lista de agentes corriendo en las sesiones (clic = ir a esa sesión) y, si no hay, "No hay agentes corriendo".
3. Abajo a la izquierda, la **fila de agentes instalados** (Claude Code, Codex, OpenCode, Qwen, Gemini…) como íconos con un **anillo de porcentaje** de uso de su límite cuando hay dato real; y debajo **"Archivos"**, que al hacer clic muestra los archivos de la carpeta de la sesión activa.
4. Arriba a la derecha, botón **Uso de agentes**: vista con períodos **Hoy / 7 días / 30 días / 1 año / Todo** (cambiado de 7/30/90/Todo al adoptar [`ESPECIFICACION.md`](../ESPECIFICACION.md) §10), tarjetas (tokens de entrada, de salida, turnos, tiempo de agente), **tokens por día** (barras entrada/salida), **cuándo trabajan** (mapa día × hora), **modelos** (participación por proveedor + tabla entrada/salida/turnos), **agentes** (participación) y **tareas que más tokens usaron** (título, carpeta, modelo).
5. Abajo a la derecha, la **RAM que usa Kuntur**.

**Reglas que se mantienen:** diseño minimalista v4 (una sola barra arriba, jerarquía por texto, color solo para estado), atajos Ctrl+Shift, temas Blue/Dark/White en vivo, nada de datos personales en el repo (las pruebas usan datos sintéticos).

**Arquitectura:**
- `kuntur-usage` (nuevo contenido, sin UI): modelo `UsageRecord`, lectores **Claude Code** (JSONL), **Codex** (JSONL comprimido zstd) y **OpenCode** (SQLite solo lectura), y agregaciones por período. Corre en un hilo aparte; la UI solo recibe resultados.
- `kuntur-agents` (nuevo contenido, sin UI): catálogo de agentes conocidos (`AgentKind`), detección de instalados (PATH) y de **agentes corriendo** dentro de cada sesión (árbol de procesos con `sysinfo`) con estado por actividad.
- `kuntur-term`: **carpeta actual** de la sesión por OSC 7 / OSC 9;9 (escáner propio antes del parser VT) + `Pane::pid()` y tiempo de la última salida.
- `kuntur-app`: pestañas en la barra, panel izquierdo (agentes / archivos), vista "Uso de agentes", indicador de RAM.

**Fuentes de datos verificadas en este equipo (solo estructura, 2026-11-23):**
- Claude Code: `~/.claude/projects/*/*.jsonl`. Líneas `type:"assistant"` con `message.{id, model, usage{input_tokens, output_tokens, cache_creation_input_tokens, cache_read_input_tokens}}`, `requestId`, `timestamp` (ISO 8601), `cwd`, `sessionId`, `gitBranch`. Títulos: líneas `type:"ai-title"` (`aiTitle`, `sessionId`) y `type:"custom-title"`. Una respuesta puede repetirse en varias líneas: **contar una sola vez por (`message.id`, `requestId`)**.
- Codex: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl.zst` (**zstd**). Eventos `payload.type:"token_count"` con `info` (uso por turno y acumulado) y `rate_limits` (primario/secundario con porcentaje usado). Confirmar nombres exactos de campos con un archivo real **imprimiendo solo claves**, nunca contenido.
- OpenCode: `~/.local/share/opencode/opencode.db` (SQLite). Tablas `session` y `session_v2`: `id, title, directory, model, agent, cost, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write, time_created, time_updated`. **Abrir en solo lectura** (`?mode=ro`). **Nunca leer** las tablas `credential`, `account`, `control_account`.
- Qwen / Gemini: tienen carpetas (`~/.qwen`, `~/.gemini`), lectores en una fase posterior (se muestran instalados, sin datos de uso).

**Dependencias nuevas (workspace):** `serde_json = "1"`, `zstd = "0.13"`, `rusqlite = { version = "0.43", features = ["bundled"] }`, `jiff = "1"` (fechas/zonas; ya está en el árbol de gpui-kit), `sysinfo = "0.40"`. Verificar versiones con `cargo search` al empezar.

---

## Bloque A — Datos y lógica (TDD, sin UI)

### Task 1: Modelo de uso y agregaciones (`kuntur-usage`)
**Archivos:** `crates/kuntur-usage/src/{lib.rs,model.rs,aggregate.rs}`

`model.rs`:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AgentKind { ClaudeCode, Codex, OpenCode, Qwen, Gemini, Other }

/// Un turno (o sesión, si la fuente solo da totales) con sus tokens.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageRecord {
    pub agent: AgentKind,
    pub model: String,
    /// Segundos Unix (UTC).
    pub ts: i64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub session_id: String,
    pub title: Option<String>,
    /// Carpeta del proyecto (solo el último segmento se muestra en la UI).
    pub project: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period { Today, Days7, Days30, Year, All }   // 1 / 7 / 30 / 365 días locales / todo
```
(Originalmente `Days7, Days30, Days90, All`; cambiado el 2026-11-23 por la especificación §10, con `Period::ALL` y `label()` para el selector.)
`aggregate.rs` — `pub fn summarize(records: &[UsageRecord], period: Period, now: i64, tz: &jiff::tz::TimeZone) -> Summary` con:
- `totals { input, output, turns, sessions, agent_time_secs }` y `previous` (mismo largo, período anterior) para el % de cambio (en `All`, `previous = None`).
- `by_day: Vec<(jiff::civil::Date, u64 input, u64 output)>` **con días vacíos rellenados** dentro del período (en `All`: desde el primer registro).
- `heatmap: [[u64; 24]; 7]` (lunes=0) en la **zona local** `tz`.
- `models: Vec<ModelRow { model, provider, input, output, turns }>` ordenado por total desc; `provider` = "Anthropic" (`claude-*`), "OpenAI" (`gpt-*`, `o*`, `codex*`), "Google" (`gemini-*`), "DeepSeek", "Qwen", o "Otros".
- `agents: Vec<(AgentKind, u64 total)>` desc.
- `top_tasks: Vec<TaskRow { title, project, model, agent, total }>` (agrupar por `session_id`; título = el del registro o "Sin título"; 10 primeras).
- `agent_time_secs`: por sesión, suma de intervalos entre turnos consecutivos **≤ 5 min** (pausas largas no cuentan).

**Pruebas (escribirlas primero, con registros sintéticos):** total por período excluye lo de fuera; días vacíos rellenados; heatmap ubica en hora local (usar `TimeZone::fixed(-5h)`); proveedor por prefijo de modelo; top tareas agrupa por sesión y ordena; tiempo de agente ignora pausas > 5 min; % de cambio contra el período anterior.

### Task 2: Lector de Claude Code
**Archivo:** `crates/kuntur-usage/src/claude.rs` — `pub fn read_dir(root: &Path) -> Vec<UsageRecord>` y `pub fn parse_lines(text: &str, project_hint: Option<&str>) -> Vec<UsageRecord>`.
- Solo líneas `type:"assistant"` con `message.usage`; deduplicar por (`message.id`, `requestId`); ignorar líneas inválidas sin fallar.
- Título por `sessionId` desde `ai-title` / `custom-title` (gana `custom-title`).
- `project` = `cwd`.
- **Pruebas** con un JSONL sintético en el propio test (2 respuestas, una repetida, una línea rota, un `ai-title`): 2 registros, tokens correctos, título asignado.

### Task 3: Lector de Codex (zstd)
**Archivo:** `crates/kuntur-usage/src/codex.rs` — `read_dir(root)` recorre `**/*.jsonl` y `**/*.jsonl.zst` (descomprime con `zstd`).
- Primero, con una prueba ignorada (`#[ignore]`) o un binario de diagnóstico **local** (fuera del repo), listar **solo las claves** de `token_count` e `info` de un archivo real para confirmar nombres; documentarlas en un comentario.
- Un registro por evento `token_count` con uso **del turno** (no el acumulado); modelo del último `turn_context`; carpeta de `session_meta`/`turn_context` si existe.
- Además `pub fn latest_limits(root) -> Option<Limits { primary_pct, secondary_pct, primary_resets_at, secondary_resets_at }>` del evento más reciente (para el anillo).
- **Pruebas** con JSONL sintético comprimido en memoria con `zstd::encode_all`.

### Task 4: Lector de OpenCode (SQLite, solo lectura)
**Archivo:** `crates/kuntur-usage/src/opencode.rs` — `read_db(path) -> Vec<UsageRecord>`: `SELECT id, title, directory, model, tokens_input, tokens_output, tokens_cache_read, tokens_cache_write, time_created FROM session` (y `session_v2` si existe, sin duplicar `id`). Abrir con `OpenFlags::SQLITE_OPEN_READ_ONLY`. `time_created` puede venir en milisegundos: normalizar a segundos. **No consultar** `credential`/`account`.
- **Pruebas:** crear una BD temporal con ese esquema mínimo, 2 sesiones → 2 registros; BD inexistente → vacío sin error.

### Task 5: Catálogo y detección de agentes (`kuntur-agents`)
**Archivos:** `crates/kuntur-agents/src/{lib.rs,catalog.rs,running.rs}`
- `catalog.rs`: `AgentInfo { kind, name, exe_names: &[&str], color: u32, letter: char }` para Claude Code (`claude.exe`), Codex (`codex`, `codex.exe`, proceso `node` con `codex` en la línea de comando), OpenCode (`opencode.exe`), Qwen (`qwen`), Gemini (`gemini`), y `installed() -> Vec<AgentKind>` buscando en el PATH (`.exe`, `.cmd`, `.ps1`).
- `running.rs`: `pub fn agents_under(pid: u32, sys: &sysinfo::System) -> Vec<RunningAgent { kind, pid, started_at }>` recorriendo **descendientes** del shell de la sesión. Estado: `Working` si la sesión tuvo salida en los últimos 3 s, si no `Waiting` (heurística v1; en una fase posterior se reemplaza por hooks / OSC 7501).
- **Pruebas** de la función pura de coincidencia `match_process(name, cmdline) -> Option<AgentKind>` (incluye `node … codex`).

### Task 6: Carpeta actual (OSC 7 / OSC 9;9) y datos del panel
**Archivos:** `crates/kuntur-term/src/cwd.rs`, `pane.rs`, `PaneOptions::default_shell`
- `CwdScanner` con estado (las secuencias pueden partirse entre lecturas): detecta `ESC ] 7 ; file://host/C:/ruta BEL|ST` y `ESC ] 9 ; 9 ; "C:\ruta" BEL|ST`; devuelve la última ruta. Se ejecuta en el hilo lector **antes** de `emulator.feed` (no altera los bytes).
- PowerShell por defecto: `-NoLogo -NoExit -Command <script>` que envuelve el `prompt` existente para emitir `ESC]9;9;"<ruta>"ESC\` (como Windows Terminal) sin cambiar lo que el usuario ve.
- `Pane::{pid(), cwd(), last_output_age()}`.
- **Pruebas:** escáner con ambas secuencias, terminador BEL y ST, y secuencia partida en dos trozos; integración ConPTY: `cmd /c` que imprime un OSC 9;9 → `pane.cwd()` lo refleja.

---

## Bloque B — Interfaz v5 (especificación; probar SIEMPRE con `scripts/smoke.ps1`)

### Task 7: Pestañas de sesiones en la barra de título
- La barra (34 px) contiene: botón panel izquierdo · **pestañas** (ícono `SquareTerminal`, nombre, `×` al pasar el mouse; activa con fondo `ui.surface_2` y texto pleno; ancho máximo 200 px con elipsis; se desplazan si no caben) · botón `+` · espacio de arrastre · botones derechos (**Uso de agentes**, paleta ⌘) · controles de Windows.
- Doble clic en una pestaña = renombrar (reusa el campo de la paleta). Clic medio = cerrar.
- El riel de sesiones de F2 desaparece; los atajos de sesiones siguen iguales.

### Task 8: Panel izquierdo de agentes y "Archivos"
- Ancho 240 px (completo) ↔ 56 px (compacto, solo íconos); Ctrl+Shift+B; nunca oculto.
- Arriba: buscador ("Buscar agentes"), filtros **Todos / Esperando / Trabajando** (texto, el activo subrayado con acento).
- Lista: una fila por agente corriendo (ícono con color del agente, nombre, pestaña/sesión en tenue, punto de estado, tiempo); clic → activa esa sesión. Vacío: "No hay agentes corriendo" y en tenue "Abre claude, codex, opencode… en una sesión".
- Abajo: fila de **agentes instalados** (íconos 28 px con la letra/color del catálogo) con **anillo de progreso** alrededor cuando hay porcentaje real (Codex: `latest_limits().primary_pct`; Claude: cuando exista el puente de línea de estado, fase posterior) y tooltip "Codex · 5 h: 12 % · semanal: 44 %"; sin dato → sin anillo y tooltip "sin datos de límite".
- Debajo: **"Archivos"** (ícono `Folder`): alterna el contenido del panel a un explorador de la carpeta actual de la sesión activa (carpetas primero, tamaño y fecha en tenue, ↑ subir, clic en carpeta entra, doble clic en archivo lo abre con la app de Windows, clic derecho copia la ruta). Sin carpeta conocida: "Carpeta desconocida (abre un shell PowerShell)".

### Task 9: Vista "Uso de agentes"
- Botón de ícono arriba a la derecha (`ChartColumn` o similar de Lucide): **abre la vista y la cierra al volver a pulsarlo** (§4); acción en la paleta; Esc vuelve a la terminal. Ocupa el área central.
- Botón de ícono **Tema** al lado (§4): menú Blue / Dark / White con el actual marcado; guarda la preferencia (ya existe `set_theme` + `persist`).
- Cabecera "Uso de agentes" + rango de fechas en tenue + selector **Hoy / 7 días / 30 días / 1 año / Todo** (`Period::ALL`, `label()`; ✅ modelo y pruebas hechos).
- 4 tarjetas: Tokens de entrada (incluye caché lectura + escritura, con "NN % desde caché" debajo), Tokens de salida, Turnos, Tiempo de agente — valor grande, % de cambio vs. período anterior en verde/rojo (en "Hoy", contra ayer).
- Línea de resumen bajo las tarjetas (§10): agente más usado · agentes usados · sesiones · **costo estimado**.
- **Tokens por día**: barras apiladas (entrada / salida) dibujadas con `canvas`; eje de fechas mínimo; tooltip al pasar. Con muchos días se agrupa: **1 año** → por semana; **Todo** con más de 120 días → por mes (función pura en `kuntur-usage`, con pruebas). En **Hoy** las barras son por hora.
- **Cuándo trabajan**: mapa 7 × 24 con intensidad por cuantiles.
- **Modelos**: barra de participación por proveedor + tabla (modelo, entrada, salida, turnos).
- **Agentes**: participación por agente con barra fina.
- **Tareas que más tokens usaron**: título, carpeta (último segmento), modelo, total.
- Carga en hilo aparte con indicador "Leyendo historial…"; se cachea y solo relee archivos cambiados (por fecha de modificación) al reabrir.
- **Exportar CSV** (§10): botón en la cabecera; escribe los registros del período (fecha local, agente, modelo, entrada, salida, caché, sesión, carpeta) donde el usuario elija.

### Task 9b: Costo estimado y almacén incremental (§10)
- **Costo estimado**: `kuntur-usage/src/pricing.rs` — tabla de precios por modelo (prefijo → USD por millón de tokens de entrada, salida, lectura y escritura de caché) cargada de `pricing.toml` **editable por el usuario** en `%APPDATA%\Kuntur\`. Los valores iniciales se verifican en las páginas oficiales al implementar (no se inventan). Modelo sin precio → "sin precio" y no suma. OpenCode trae costo real (`cost`): se usa ese y se marca como real. La UI dice siempre "estimado". Reemplaza "cuentas que más facturan", que los historiales locales no registran.
- **Almacén SQLite** (`%APPDATA%\Kuntur\usage.db`, `rusqlite`): tabla de registros + tabla de archivos importados (ruta, tamaño, fecha de modificación, posición leída). Al abrir la vista solo se importa lo nuevo (JSONL: desde la última posición; zstd y OpenCode: si cambió la fecha). Deduplicación con la misma clave de cada lector. Reemplaza la caché en memoria de la Task 9.
- **Pruebas:** importar dos veces el mismo archivo no duplica; agregar líneas a un JSONL importa solo las nuevas; costo con tabla sintética (incluye caché y modelo sin precio).

### Task 10: Indicador de RAM y aceptación
- Abajo a la derecha, texto tenue "Kuntur · 62 MB" (memoria de trabajo del proceso, `sysinfo`, cada 2 s). Sin barra de estado completa: solo una línea de 22 px con la carpeta actual a la izquierda y la RAM a la derecha.
- `scripts/smoke.ps1`: `-Usage` (abre la vista de uso) y variable `KUNTUR_USAGE_ROOT` para apuntar los lectores a datos **sintéticos** de prueba en `target\smoke-usage\` (generados por el propio script), para no mostrar datos reales en capturas.
- Capturas a revisar: pestañas con 3 sesiones (Blue/Dark/White), panel de agentes vacío, "Archivos", vista de uso con datos sintéticos.
- `docs/plans/f3-resultados.md` con la lista manual (incluye abrir `claude`/`codex`/`opencode` en sesiones y ver la lista de agentes y los filtros).
- CI verde y push de la rama `fase/f3-v5`.

## Fuera de alcance
Estado exacto esperando/trabajando por hooks y OSC 7501 (heurística por ahora) · puente de línea de estado de Claude Code para el anillo de 5 h/semanal (requiere tu permiso para tocar `~/.claude/settings.json`) · lectores de Qwen, Gemini, Kimi y DeepSeek · registro propio de la app como fuente de uso · terminal acoplada, alertas, auditoría, SSH, vista previa, MCP (fases siguientes, ver [`README.md`](README.md)).
