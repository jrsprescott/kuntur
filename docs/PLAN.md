# Kuntur — Plan de construcción (v4)

> Terminal propia en Rust, liviana y orientada a trabajar con agentes de IA (Claude Code, Codex).
> Nombre provisional. v1, v2 y **v3: 2026-11-23** (cambios de v2 con 🆕, de v3 con 🆕³).

## Cambios de la v4 — diseño minimalista
Referencias: **Zed** (una sola barra, paneles que se ocultan), **Ghostty** (la terminal manda, nada decorativo), **Linear** (jerarquía por peso y color, no por cajas) y **Warp 2** (bloques sin bordes).

| Antes (v3) | Ahora (v4) |
|---|---|
| Barra de título con logo, buscador grande y 3 círculos de tema, **más** una segunda fila con nombre, chips de ruta/rama/MCP y 3 botones | **Una sola barra de 40 px**: a la izquierda el ícono de sesiones; al centro "nombre · ruta · ⎇ rama" (clic = Ctrl+K); a la derecha 3 íconos (terminal acoplada, acciones, panel derecho) y los botones de ventana en gris, que toman color al pasar el mouse. Los temas pasan a Ctrl+K → Apariencia |
| Barra de estado inferior (shell, UTF-8, ConPTY, ruta, agentes, GPU, RAM, hora) | **Eliminada**: la ruta ya está arriba y los agentes a la derecha |
| Cada comando dentro de una caja con cabecera (ruta, rama, tiempo, ✓) | **Sin cajas**: `❯ comando` y su salida, separados por una línea fina. Tiempo y botones a la derecha, y los botones solo aparecen al pasar el mouse. El ✓ solo se muestra cuando hay error (✗) |
| Línea de arranque "PowerShell 7 · ConPTY · OSC 7/133…" | Eliminada (va en la tarjeta de la sesión) |
| Entrada con línea de prompt + fila de 6 atajos | Una sola línea `❯ Escribe un comando · Tab completa · Ctrl+K acciones`. El aviso de pegado aparece solo cuando hace falta |
| Íconos de sesión con degradados de colores | **Íconos de un solo color**; la activa con fondo sutil y barra de acento; punto de estado de 7 px. El `+` queda al pie del riel (clic derecho: elegir shell o SSH) |
| Pestañas del panel en MAYÚSCULAS, contadores, herramientas siempre visibles | "Agentes 8 · Archivos", con un punto ámbar si alguien espera. "Esta/Todas" y ⚡8 solo al pasar el mouse. Los botones de archivos, igual |
| Panel de uso de ~11 líneas siempre abierto | **Una línea**: `Opus 5.5 · $0.29 · ctx 12% · 5 h 38% · sem 61%`; clic para ver el detalle |
| Bordes y fondos en todos los paneles | Solo líneas de 1 px (`--line`), fondos casi iguales; el contraste lo da el texto |
| Panel derecho siempre visible | Se oculta con Alt+J (Ctrl+J en la app real) para modo enfoque |

Reglas para la app real:
1. Máximo **una** barra horizontal fija arriba; nada abajo salvo la terminal acoplada si está abierta.
2. Todo lo secundario aparece **al pasar el mouse** o en **Ctrl+K**.
3. Jerarquía con **peso y color de texto**, no con cajas ni chips.
4. Color solo para **estado** (trabajando, esperando, error) y para el acento; los íconos van en un solo color.
5. Tamaños: UI 12.5 px, código 13 px, filas de 24 px, separadores de 1 px.

## Cambios de la v3
- 🆕³ **Panel "Archivos"** junto a "Agentes": lista la carpeta de la sesión en **Windows** y, en sesiones SSH, también la del **servidor Linux**. Permite arrastrar entre ambos, desde y hacia el Explorador de Windows, copiar/pegar archivos (Ctrl+C / Ctrl+V), subir, descargar, abrir y crear carpetas. Todo por la misma conexión SSH (SFTP), sin pedir clave de nuevo.
- 🆕³ **Soltar archivos sobre la sesión**: por SSH se **suben** a la carpeta actual del servidor; en local se **inserta la ruta** en la entrada. Sobre la terminal acoplada pasa lo mismo.
- 🆕³ **Uso minimalista**: sin tarjetas. Una línea por dato, barras de 3 px, tokens/s con mini-gráfico en línea. El origen de los datos va en un ⓘ.
- 🆕³ **Vista de íconos minimalista**: solo el ícono (riel de 60 px), punto de estado e insignia. Al pasar el mouse aparece una **tarjeta** con el nombre, de qué trata la sesión, la ruta y el estado de sus agentes.

## Cambios de la v2
- 🆕 **Agentes como lista compacta** (una fila por agente o subagente, con la acción que hace en este momento). Entran 8 o más en paralelo; se reemplazaron las tarjetas.
- 🆕 **Copiar y pegar sin fallas**: la salida que se está seleccionando no se redibuja; botones de copiar por bloque; menú con clic derecho; pegado de varias líneas seguro.
- 🆕 **Barra de sesiones con 2 vistas**: completa y solo íconos (nunca se oculta del todo; corregido tras la v4).
- 🆕 **Terminal acoplada sin IA** abajo de cada sesión, en la misma ruta (o en el worktree de un agente), **también por SSH** en el mismo servidor; botón **"↧ Enviar a terminal"** en los comandos que sugiere el agente.

## 1. Metas medibles

| Meta | Valor objetivo | Referencia medida en este equipo |
|---|---|---|
| RAM con 5 sesiones, 3 agentes y 8 subagentes | **< 120 MB** | Warp: **~903 MB** (2 procesos) |
| RAM de la ventana vacía | < 80 MB | Ventana GPUI + gpui-kit de prueba: **70 MB** |
| Arranque en frío | < 300 ms | — |
| Latencia de tecla a pantalla | < 10 ms | — |
| CPU en reposo | ~0 % (sin redibujar si no hay cambios) | — |
| Plataforma | Windows 11 (ConPTY) primero | RTX 4060 → DirectX 12 |

## 2. Lo mejor de cada terminal (qué tomamos)

| De | Tomamos | Cómo lo hacemos nosotros |
|---|---|---|
| **Warp** | Pestañas verticales con `+` y `×`; salida en **bloques**; editor de entrada moderno | Bloques vía integración de shell (OSC 133); editor propio cuando el shell está en el prompt |
| **VS Code** 🆕 | **Terminal integrada que se acopla abajo** (Ctrl+`), redimensionable | Panel acoplado por sesión, con su propio PTY o canal SSH |
| **Zellij 0.46** | **Bandeja de atención** (qué agente espera tu respuesta); vista previa de sesiones | OSC 7501 + hooks → punto/insignia en la sesión y notificación |
| **Alacritty** | Núcleo VT probado y rápido; selección por celdas | `alacritty_terminal` 0.26 (incluye selección normal, por palabra, por línea y en bloque) |
| **WezTerm** | PTY en Windows (ConPTY); dominios SSH con varias pestañas por conexión | `portable-pty` 0.9; `russh` 0.65 para SSH con canales |
| **Zed** | GPUI + `alacritty_terminal` (la misma arquitectura) | `gpui-kit` 0.11 (GPUI + dock, pestañas, temas, listas virtuales) |
| **Windows Terminal** | Perfiles por shell; Ctrl+C copia si hay selección; aviso al pegar varias líneas | Menú `▾` junto al `+`; mismas reglas de copiar/pegar |
| **Ghostty / Kitty** | Notificaciones de escritorio; OSC 52 (copiar desde apps remotas) | Notificaciones nativas de Windows; OSC 52 con permiso |
| **claude-squad / agent-deck / agentgc** | Un **git worktree por agente**; estados | Panel "Agentes" + lanzador con worktree automático |
| **fish / PSReadLine / zoxide** | Sugerencia en gris, frecuencia × recencia, `z nombre` | SQLite de visitas + `nucleo` (búsqueda difusa) |

## 3. Requisitos → solución

| # | Pedido | Solución |
|---|---|---|
| 1 | Pestañas a la izquierda con `+` y eliminar | Barra lateral con ícono por sesión (tipo de proyecto: Rust, web, CAD, shell, SSH), punto de estado, insignia de "espera", nombre (doble clic para renombrar), **texto que resume la sesión**, carpeta, `×`; `+` y `▾` (PowerShell, Git Bash, WSL, **SSH**); cerrar se puede deshacer |
| 1b 🆕³ | Vista de íconos minimalista (**la barra de sesiones nunca se oculta**) | **2 vistas**: completa (232 px) e **íconos** (riel de 60 px: solo ícono + punto de estado + insignia; barra de acento en la activa). El mínimo es el riel de íconos: las sesiones siempre están a la vista. En la vista de íconos, al pasar el mouse sale una **tarjeta** con nombre, resumen de la sesión, ruta, rama, estado de agentes y atajo. Ctrl+B alterna; se recuerda la elección |
| 2 | Autocompletado de rutas y memoria de carpetas | Editor propio en el prompt: rutas de Windows (corrige mayúsculas), carpetas visitadas (frecuencia × recencia), historial, comandos; `Tab`, `→`, `z texto`. **En SSH** completa rutas remotas |
| 3 | Estilo de la imagen, botones a la derecha, Blue / Dark / White | Barra de título propia con los botones a la derecha; 3 temas con su pareja de fuentes (`themes/*.toml`) |
| 4 | Orientada a IA | Agentes como ciudadanos de primera: lanzador, estados, permisos desde el panel, MCP por pestaña, uso y límites siempre a la vista |
| 5 🆕 | Panel de agentes que aguante muchos en paralelo | **Lista compacta tipo árbol**: fila del agente (ícono, nombre, `activos/total` subagentes, acción actual, tiempo) y debajo una fila por subagente (`Explore`, `Plan`, `general-purpose`…) con su acción (`Read …`, `Bash …`, `Grep …`). Plegable. Contadores trabajando / esperando / listos. Vista **Pestaña / Todas**. Botón **✓** en la propia fila para permitir. El detalle (PID, carpeta, worktree, MCP) va en una franja fija debajo y solo del elemento seleccionado. Lista virtual: 50+ filas sin costo |
| 6 🆕³ | Panel de uso (minimalista) | ~11 líneas sin tarjetas: modelo · esfuerzo · thinking · costo; mini-gráfico de tok/s + duración; ↑ entrada · ↓ salida · caché; contexto, Claude 5 h, Claude semanal, Codex 5 h, Codex semanal, cada uno con barra de 3 px + % + tiempo para reiniciar; total del día. Fuentes de datos en un ⓘ |
| 7 | MCP on/off y comandos por pestaña | Hoja Ctrl+K: interruptores MCP por pestaña + perfiles; modelo, esfuerzo, permisos, thinking; comandos con atajo y ámbito pestaña/global, que se pueden ejecutar **en la sesión o en la terminal acoplada** |
| 8 | Colores del código | El programa marca *qué* es cada cosa y la terminal decide *el tono* con su paleta ANSI (§7) |
| 9 🆕 | Copiar y pegar sin problemas | §5 "Copiar y pegar": selección que no se pierde, Ctrl+C inteligente, Ctrl+Shift+C/V, clic derecho, botones ⧉ por bloque, copia limpia (sin números de línea ni ANSI), pegado entre corchetes, OSC 52 |
| 10 🆕 | Terminal dentro de la sesión, sin IA, en la misma ruta (también por SSH) | §5 "Terminal acoplada": Ctrl+` o botón **⌄ Terminal**; abre en la ruta de la sesión o en el worktree del agente elegido; en SSH abre **otro canal en la misma conexión** y en la misma carpeta remota; muestra el prompt real del shell |
| 12 🆕³ | Pasar archivos Windows ↔ Linux de forma transparente | Pestaña **Archivos** en el panel derecho (§5.6): lista Windows y Linux de la ruta actual, arrastrar/copiar/pegar entre ambos y con el Explorador, subir, descargar, abrir |
| 13 🆕³ | Arrastrar un archivo a la sesión | Por SSH se sube a la carpeta actual; en local se inserta la ruta entre comillas (§5.7) |
| 11 🆕 | Ejecutar lo que la IA pide sin abrir otra sesión | Los comandos que el agente sugiere aparecen con **⧉ Copiar** y **↧ Enviar a terminal**: el comando queda escrito en la terminal acoplada **sin ejecutarse**, para revisarlo y presionar Enter. También: seleccionar texto → clic derecho → "Enviar a la terminal acoplada" |

## 4. Arquitectura

```
┌──────────────────────────────────── kuntur-app (GPUI + gpui-kit) ─────────────────────────────────────┐
│ Barra de título · Sesiones (completa/íconos) · [Sesión: bloques + editor] · Agentes · Uso          │
│                                                     [Terminal acoplada: PTY o canal SSH propio]     │
└──────▲───────────────────────▲──────────────────────▲────────────────────────▲──────────────────────┘
       │ celdas, selección     │ sugerencias          │ árbol de agentes       │ métricas
┌──────┴─────────────┐ ┌───────┴────────┐ ┌───────────┴──────────┐ ┌───────────┴────────┐
│ kuntur-term          │ │ kuntur-complete  │ │ kuntur-agents          │ │ kuntur-usage         │
│ portable-pty       │ │ rutas locales  │ │ hooks (pipe), OSC    │ │ statusLine, OTel   │
│ russh (canales)    │ │ y remotas,     │ │ 7501, subagentes,    │ │ local, rollout     │
│ alacritty_terminal │ │ frecencia,     │ │ git worktree,        │ │ Codex, precios     │
│ OSC 7/52/133/7501  │ │ historial      │ │ comandos sugeridos   │ │                    │
└────────▲───────────┘ └────────────────┘ └──────────▲───────────┘ └─────────▲──────────┘
         │ ConPTY / SSH                              │ \\.\pipe\kuntur      │
   pwsh · bash · wsl · ssh ──► claude / codex ──► kuntur-hook.exe (hooks + statusLine)
          kuntur-config: config.toml · themes/*.toml · keybindings.toml · mcp-profiles.toml · sesiones guardadas
```

### Crates (workspace)
| Crate | Responsabilidad |
|---|---|
| `kuntur-app` | Binario. Ventana, layout, elemento GPUI propio que dibuja la grilla de la terminal, paneles, hoja Ctrl+K, menú contextual |
| `kuntur-term` | **Varios paneles por sesión** (principal + acoplada). Cada panel: un PTY local o un **canal SSH**. Alimenta `alacritty_terminal`; intercepta OSC 7, 52, 133, 7501 y BEL; selección y extracción de texto |
| `kuntur-agents` | Árbol agente → subagentes por sesión; eventos de hooks por named pipe; worktrees; detección por árbol de procesos; extracción de **comandos sugeridos** |
| `kuntur-usage` | statusLine de Claude, receptor OTLP local, rollouts JSONL de Codex |
| `kuntur-complete` | Autocompletado local y remoto, frecuencia × recencia, historial |
| `kuntur-files` 🆕³ | Listados local/SFTP/WSL, cola de transferencias, portapapeles de archivos (CF_HDROP), arrastrar hacia el Explorador (OLE) |
| `kuntur-config` | TOML con recarga en caliente; guarda y restaura sesiones (vista de la barra, alto de la terminal acoplada) |
| `kuntur-hook` | Binario mínimo para hooks y `statusLine`: reenvía el JSON al pipe con `KUNTUR_PANE` y termina en < 5 ms |

## 5. Detalle de las funciones nuevas 🆕

### 5.1 Lista de agentes (árbol compacto)
| Columna | Fuente |
|---|---|
| Agente / subagente y tipo | Hooks `SubagentStart` / `SubagentStop` (`agent_id`, `agent_type`) |
| **Acción actual** (`Read ruta`, `Bash comando`, `Grep "patrón"`, `WebFetch url`) | Hook `PreToolUse` (`tool_name` + resumen de `tool_input`); se limpia con `PostToolUse` |
| Estado | `UserPromptSubmit`/`PreToolUse` → trabajando · `Notification` (`permission_prompt`, `idle_prompt`, `elicitation_dialog`) o `PermissionRequest` → esperando · `Stop`/`SubagentStop` → listo · OSC 7501 como respaldo (y como **única** fuente en sesiones remotas) |
| Tiempo | Desde el último cambio de estado |

- Una fila de ~25 px: con 8 subagentes la lista entera ocupa ~230 px.
- Se usa la lista virtual de gpui-kit: solo se dibujan las filas visibles.
- Si un subagente espera permiso, la fila se resalta y su sesión muestra la insignia; ✓ en la fila responde sin buscar la salida.

### 5.2 Copiar y pegar
- **Selección con el ratón** sobre la grilla (`alacritty_terminal::Selection`): arrastrar, doble clic (palabra), triple clic (línea), Alt+arrastrar (bloque rectangular).
- **La salida que llega no borra la selección**: la selección se guarda en coordenadas de la grilla y se ajusta con el scroll. En la simulación el bloque seleccionado no se redibuja (probado: la selección sigue intacta tras 4,5 s de actualizaciones en vivo).
- **Ctrl+C**: copia si hay selección; si no, manda la interrupción al programa (como Windows Terminal). **Ctrl+Shift+C / Ctrl+Shift+V** siempre copian/pegan. Clic derecho: menú (copiar selección, copiar bloque, enviar a la terminal acoplada, pegar en la sesión, pegar en la terminal acoplada).
- **Copia limpia**: sin códigos ANSI, sin números de línea ni botones, y uniendo las líneas que solo se cortaron por el ancho de la ventana.
- **Pegado seguro**: modo *bracketed paste* (DEC 2004), así Claude Code o el shell reciben el texto como un solo bloque. Al pegar varias líneas en el prompt se avisa ("Pegaste N líneas") y nada se ejecuta hasta presionar Enter.
- **OSC 52**: cuando un programa remoto (Claude Code por SSH, nvim) quiere copiar, el texto llega a tu portapapeles de Windows (con permiso configurable). Portapapeles del sistema con `arboard` 3.7.

### 5.3 Terminal acoplada (sin IA)
- Ctrl+` o botón **⌄ Terminal**. Se acopla abajo, se puede cambiar de alto arrastrando, maximizar o limpiar, y es independiente por sesión.
- **Ruta**: por defecto la de la sesión (OSC 7 la mantiene al día). Un selector permite abrirla en el **worktree de un agente**. El botón **"↧ Terminal aquí"** de cada agente hace lo mismo.
- **Local**: un PTY nuevo con el mismo shell de la sesión.
- **SSH**: la sesión usa `russh` 0.65. La terminal acoplada abre un **segundo canal en la misma conexión**: sin volver a pedir clave y en el mismo servidor. Al abrir el canal se envía un arranque corto que entra a la misma carpeta y activa OSC 7/133 **solo en esa sesión**, sin tocar archivos del servidor (como hace Warp con SSH). El prompt que se ve es el real (`admin@srv-web:/var/www/acme$`).
- Los comandos personalizados pueden apuntar a la terminal acoplada. Ejemplo: `Ctrl+Alt+U` → `sudo apt update && sudo apt upgrade -y` en la pestaña del servidor.

### 5.4 "Enviar a terminal" (lo que la IA pide ejecutar)
- **Local**: el hook `Stop` entrega `last_assistant_message`; Kuntur extrae los bloques de código de shell (```bash, ```powershell, ```sh) y los muestra bajo la respuesta con **⧉ Copiar** y **↧ Enviar a terminal**.
- **Remoto / respaldo**: seleccionar texto → clic derecho → "Enviar a la terminal acoplada".
- **Nunca se ejecuta solo**: el comando queda escrito en la terminal acoplada, tú lo revisas y presionas Enter. Esto importa sobre todo con `sudo` en servidores.

### 5.6 Panel "Archivos" (Windows ↔ Linux) 🆕³
- Vive en el panel derecho como pestaña **Agentes | Archivos**.
- **Sesión local**: una lista de la carpeta de Windows. **Sesión SSH**: dos listas, arriba el **servidor** (`admin@srv-web · Linux`) y abajo **este equipo** (`Windows`, por defecto Descargas).
- **Sigue la carpeta de la sesión** (🔗, por OSC 7). Si navegas en el panel, deja de seguirla hasta que vuelvas a activarlo.
- Cada fila muestra ícono por tipo, nombre, tamaño y fecha. Ruta en migas de pan clicables. Botones: subir un nivel, nueva carpeta, subir/copiar archivos, seguir sesión, abrir terminal acoplada aquí.

| Acción | Cómo | Implementación |
|---|---|---|
| Servidor → este equipo (descargar) | Arrastrar de la lista de arriba a la de abajo, clic derecho "Descargar" o Ctrl+C / Ctrl+V | SFTP por la **misma conexión SSH** (`russh-sftp` 4.0) |
| Este equipo → servidor (subir) | Arrastrar hacia arriba, "Subir al servidor", Ctrl+C / Ctrl+V | SFTP |
| Explorador de Windows → panel o sesión | Soltar archivos/carpetas sobre una lista o sobre la sesión | Evento de soltar archivos externos de GPUI (`ExternalPaths`) |
| Panel → Explorador de Windows | Arrastrar una fila al escritorio o a una carpeta | OLE `DoDragDrop` con el crate `windows` 0.62 (los remotos se descargan primero a una carpeta temporal) |
| Copiar en el Explorador → pegar en el panel | Ctrl+C en el Explorador, Ctrl+V en la lista | Leer `CF_HDROP` del portapapeles (`clipboard-win` 5.4) |
| Copiar en el panel → pegar en el Explorador | Ctrl+C en la lista, Ctrl+V en el Explorador | Escribir `CF_HDROP`; si los archivos son remotos, se descargan antes |
| Abrir un archivo | Doble clic | Local: app predeterminada de Windows. Remoto: se descarga a temporal y se abre; si se modifica, Kuntur ofrece **volver a subirlo** |
| Insertar ruta | Arrastrar una fila a la sesión o a la terminal acoplada, o "Insertar ruta en la terminal" | Ruta entre comillas si tiene espacios |
| Eliminar | Supr o clic derecho | Local: a la Papelera. Remoto: pide confirmación (no se puede deshacer) |

- Las **transferencias** se muestran al pie del panel: flecha ↑/↓, nombre, % y velocidad, con barra de 2 px. Si el nombre ya existe se crea `nombre (2).ext` (nunca se sobrescribe sin preguntar).
- Para Linux local también sirve con **WSL**: la lista de "Linux" apunta a `\\wsl$\<distro>`, sin SFTP.

### 5.7 Soltar archivos en la sesión 🆕³
- **Sesión SSH**: soltar archivos de Windows sobre la sesión los **sube** a la carpeta actual del servidor. Queda una línea en la salida ("↑ Subiendo 3 archivos a admin@srv-web:/var/www/acme") y el progreso en el panel Archivos.
- **Sesión local**: soltar inserta la **ruta completa** en la entrada (como Windows Terminal).
- **Terminal acoplada**: igual, usando su propia carpeta.
- Con un agente abierto (Claude Code), soltar inserta la ruta para que el agente la lea.

### 5.8 Compatibilidad con cualquier CLI y perfiles de agente
- **Ejecutar**: Kuntur es una terminal completa (ConPTY + `alacritty_terminal`), así que **cualquier** CLI o TUI corre igual que en Windows Terminal: `claude`, `codex`, `opencode`, `qwen`, `gemini`, `aider`, `nvim`, `htop` por SSH, etc. Usa el PATH y las variables del usuario; los comandos instalados con npm (`.ps1`/`.cmd`) funcionan porque corren dentro del shell.
- **Aplicaciones de escritorio** (por ejemplo Antigravity, VS Code o Cursor) se pueden lanzar desde la terminal, pero se abren en su propia ventana; no son agentes de terminal.
- **Reconocimiento en el panel Agentes**, por niveles:
  1. *Completo* (Claude Code): hooks + statusLine + OSC 7501 → estado, acción actual, subagentes, costos y límites.
  2. *Medio* (Codex): proceso + rollout JSONL → estado, tokens y límites.
  3. *Genérico* (OpenCode, Qwen, Gemini, Aider…): nombre del proceso + OSC 7501/9/777, BEL, cambios de título y la regla "sin salida durante N s y aparece su prompt" → trabajando / esperando / listo aproximado.
- **Perfiles de agente** en `agents.toml` (se agrega un CLI sin recompilar):

```toml
[agent.opencode]
exe = ["opencode", "opencode.exe"]
label = "OpenCode"
waiting_regex = '(?m)^\s*>\s*$'    # prompt del agente = esperando
idle_secs = 4
usage = "none"                      # o "jsonl:<ruta>" si guarda registros

[agent.qwen]
exe = ["qwen"]
label = "Qwen Code"
waiting_regex = '(?m)^\s*>\s'
```

- Instalados hoy en este equipo: `claude`, `codex`, `opencode`, `qwen` (además de `node`, `python`, `wsl`, `ssh`). No encontrados: `gemini`, `aider`, `antigravity`.

### 5.5 Barra de sesiones: vistas e ícono + resumen
- Ícono por tipo de sesión, detectado por la carpeta (`Cargo.toml` → Rust, `package.json`/`astro.config` → web, `.dwg`/mcp autocad → CAD, SSH → servidor, si no → shell). Íconos Lucide (vienen con gpui-kit).
- **Texto que resume la sesión**: el `session_name` de Claude Code (título generado por la IA o `/rename`), o la tarea con la que se lanzó el agente; en sesiones sin IA, el último comando relevante. Se puede editar.
- La vista se guarda en `config.toml` (`sidebar = "full" | "icons"`; no existe modo oculto).

## 6. Fases

| Fase | Entrega | Criterio de "listo" |
|---|---|---|
| **F0 — Investigación** ✅ | Toolchain, simulación v2, prueba GPUI | Rust 1.100 + MSVC 14.51 + SDK 10.0.26100; ventana GPUI = 70 MB |
| **F1 — Núcleo de terminal** | 1 ventana, 1 panel: PTY + VT + render GPUI, scrollback, **selección, copiar/pegar, bracketed paste, OSC 52**, resize, truecolor | `claude`, `codex`, `nvim` y `git log` sin defectos; copiar en medio de una salida en vivo no falla; RAM < 80 MB |
| **F2 — Sesiones y temas** | Barra lateral con **2 vistas** (completa / íconos, nunca oculta), ícono + resumen, barra de título propia, 3 temas + fuentes, guardar/restaurar | Crear/cerrar/renombrar/reordenar; cambiar vista y tema en caliente; **alternar la vista de sesiones y mostrar/ocultar el panel derecho en cualquier orden y cantidad de veces sin que el área de la sesión cambie de lugar** (prueba automática de las 4 combinaciones) |
| **F3 — Terminal acoplada** 🆕 | Varios paneles por sesión, Ctrl+`, alto ajustable, ruta de la sesión o del worktree | Abrir/cerrar sin perder historial; sigue la ruta de la sesión |
| **F4 — Agentes** | `kuntur-hook`, pipe, OSC 7501, **lista compacta con subagentes**, permisos desde la fila, worktrees, **Enviar a terminal** | 8 subagentes en paralelo visibles sin desplazar; estados correctos |
| **F5 — Uso y costos** | statusLine, OTLP local, rollout de Codex, gráfico de tok/s | Números iguales a `/status` y `/usage` |
| **F6 — SSH** 🆕 | `russh`: autenticación con ssh-agent de Windows / `~/.ssh`, `known_hosts`, canal principal + canal acoplado en la misma conexión, arranque OSC 7/133 sin tocar el servidor, autocompletado remoto, agentes remotos por OSC 7501 | Abrir la terminal acoplada en `/var/www/...` sin volver a autenticar; `sudo apt upgrade` funciona |
| **F7 — Archivos** 🆕³ | Panel Archivos (local + SFTP + WSL), arrastrar entre listas, desde/hacia el Explorador, portapapeles de archivos (CF_HDROP), soltar sobre sesión/terminal, cola de transferencias | Bajar y subir un archivo de 1 GB con progreso; pegar en el Explorador lo copiado desde el servidor |
| **F8 — Shell y autocompletado** | Integración de shell, bloques, editor propio, motor de sugerencias | `cd e:\pro` + Tab; `z astro`; bloques con código de salida |
| **F9 — MCP, opciones y comandos** | Hoja Ctrl+K, perfiles MCP, atajos por pestaña con destino sesión o terminal acoplada | Una pestaña lanza agentes con su propio set de MCP |
| **F10 — Pulido** | Instalador MSI local, notificaciones, accesibilidad, medición final | Metas de §1 cumplidas |

## 7. Colores del código
- El **programa** (Claude Code, `bat`, `delta`, `nvim`, `git`) decide *qué* es cada cosa y emite códigos ANSI o truecolor.
- La **terminal** decide el tono de los **16 colores ANSI** con su paleta: el mismo estilo en cualquier lenguaje.
- Kuntur colorea por sí mismo la línea de comandos mientras escribes y sus paneles (diffs con tree-sitter de gpui-kit).

## 8. Riesgos

| Riesgo | Mitigación |
|---|---|
| GPUI aún no llega a 1.0 | `gpui-kit` fija la versión; UI aislada en `kuntur-app` |
| Dibujar la grilla en GPUI es trabajo fino | Estudiar `zed/crates/terminal_view` como referencia (GPL-3: ideas, no copiar código si se comparte) |
| OSC 7501: formato no confirmado; campos de `rate_limits` de Codex no confirmados | Verificar en F4/F5 con sesiones reales; respaldo por hooks/statusLine |
| Hooks globales afectan todas las sesiones de Claude | `kuntur-hook` no hace nada si falta `KUNTUR_PANE` |
| **SSH**: hooks y statusLine corren en el servidor y no llegan al pipe local 🆕 | Agentes remotos se siguen por OSC 7501 (viaja dentro del mismo canal); opcional más adelante: reenviar el pipe por el túnel SSH |
| **SSH**: arranque para OSC 7/133 depende del shell remoto (bash/zsh) 🆕 | Si falla, la terminal acoplada abre igual con `cd <ruta>`, solo sin bloques |
| `sudo` y comandos destructivos enviados desde la IA 🆕 | "Enviar a terminal" nunca ejecuta; resalta `sudo`, `rm -rf` y similares |
| ConPTY: detalles de resize/ratón | `portable-pty` los resuelve |
| **Arrastrar desde Kuntur hacia el Explorador**: GPUI recibe archivos soltados, pero no se confirmó que pueda iniciar un arrastre hacia afuera 🆕³ | Implementar `DoDragDrop` de OLE con el crate `windows`; respaldo: Ctrl+C → Ctrl+V (CF_HDROP) o "Descargar" |
| Archivos grandes o muchos archivos por SFTP 🆕³ | Cola de transferencias con concurrencia limitada (3), reanudación y cancelación; no bloquea la UI |
| Borrar en el servidor es irreversible 🆕³ | Siempre confirma; local va a la Papelera |

## 9. Decisiones pendientes
1. Nombre definitivo (Kuntur es provisional).
2. Shell por defecto: PowerShell 7 / Git Bash / WSL.
3. ¿Sumar otros agentes (Gemini CLI, OpenCode, Aider)?
4. ¿Uso solo personal o se compartirá? (afecta licencias).
5. 🆕 Servidores SSH: ¿se toman de `~/.ssh/config` o se definen en Kuntur? ¿Clave con ssh-agent de Windows?
6. 🆕 OSC 52 (copiar desde el servidor a tu portapapeles): ¿siempre permitido, preguntar cada vez o desactivado?
