# Estado del proyecto — nota de traspaso

> Última actualización: 2026-11-24. Léela primero al retomar en otra PC (o pídele a Claude Code que la lea).

## Dónde está cada cosa
- **Rama `integracion/f10-f11` (pendiente de revisión para avanzar `main`): Fases 1 a 11 integradas** (2026-11-24, merge de `fase/f10-mcp` y luego `fase/f11-publicacion` sobre `main`). Pruebas, clippy y fmt en verde; capturas revisadas; MSI generado desde esta rama (no instalado). Conflictos, corrección de CPU en reposo (antes/después), capturas y pendientes en `docs/plans/integracion-f10-f11.md`. Cuando el usuario avance `main` a esta rama, **`main` tendrá F1–F11**.
- Rama **`main`**: Fases 1 a 9 (hasta que se avance a `integracion/f10-f11`). Integración anterior: `docs/plans/integracion-f5-f9.md`.
- Rama **`fase/f10-mcp`**: Fase 10 · MCP y comandos (`docs/plans/f10-resultados.md`): crate `kuntur-mcp`, gestor de MCP por pestaña (botón, Ctrl+Shift+M, paleta), perfiles con modelo/esfuerzo/permisos, configuración MCP generada por CLI al lanzar (Claude Code, Codex, OpenCode; nunca la global), «Probar», capturas → vista previa y comandos con atajo. Faltan las pruebas manuales ⏳ con MCP y agentes reales.
- Rama **`fase/f11-publicacion`**: Fase 11 · Pulido y publicación (`docs/plans/f11-resultados.md`): licencia MIT OR Apache-2.0, ícono, instalador MSI (`scripts\msi.ps1 -Check`, WiX v3 + cargo-wix; AUMID `Kuntur.Kuntur`), contraste WCAG (Blue corregido), teclado en Uso/Auditoría/Conexiones, medición final, README en inglés + `README.es.md`, CONTRIBUTING, SECURITY, CHANGELOG y revisión de privacidad del historial. El repositorio es **público** desde el 2026-11-24 (revisión de privacidad del historial y del instalador 0.1.0, que se regeneró sin rutas de la PC). Falta instalar el MSI en una PC de prueba ⏳.
- Las ramas `integracion/*` y `fase/*` quedan como historia de cada fase (no hace falta trabajar en ellas).
- Rama **`fase/f3-v5`**: Fase 3 (diseño v5) **terminada** (`docs/plans/f3-resultados.md`).
- Rama **`fase/f4-acoplada`** (sale de `fase/f3-v5`): Fase 4 · Terminal acoplada **terminada** (`docs/plans/f4-resultados.md`); faltan las pruebas manuales ⏳ del usuario.
- Rama de trabajo **`fase/f5-agentes`** (sale de `fase/f4-acoplada`): **Fase 5 terminada** (`docs/plans/f5-resultados.md`, con el detalle en `f5a-resultados.md` y `f5b-resultados.md`). F5a: catálogo configurable, abrir agentes con un clic, agentes de la acoplada, alertas y auditoría. F5b: `kuntur-hook.exe` + named pipe, OSC 7501, estado exacto (trabajando / pide permiso / espera respuesta / terminó), subagentes con su acción, Permitir / Rechazar desde la fila, worktree por agente (la acoplada arranca ahí), "Enviar a terminal", anillo de Claude por su línea de estado e instalador de ganchos con confirmación. Faltan las pruebas manuales ⏳ del usuario (sobre todo **instalar los ganchos** y probar con `claude` real). Pendiente: auditoría desde los hooks.
- Rama **`fase/f6-ssh`** (sale de `fase/f4-acoplada`): Fase 6 · SSH y conexiones **terminada** (`docs/plans/f6-resultados.md`); faltan las pruebas manuales ⏳ contra un servidor real. Se trabajó en paralelo a F5: al integrar, revisar los ganchos en `workspace.rs` (lista en `f6-resultados.md`).
- Rama **`fase/f8-vista-previa`** (sale de `fase/f4-acoplada`, en paralelo con otras fases): Fase 8 · Vista previa y diff **terminada para archivos locales** (`docs/plans/f8-resultados.md`; spike del visor web en `docs/plans/f8-spike-wry.md`); faltan las pruebas manuales ⏳ y los archivos remotos (dependen de F6/F7).
- Rama **`fase/f9-autocompletado`** (sale de `fase/f4-acoplada`, en paralelo con otras fases): Fase 9 · Shell y autocompletado **terminada** (`docs/plans/f9-resultados.md`); faltan las pruebas manuales ⏳ del usuario (escribir rutas y aceptar con Tab).
- Planes: `docs/plans/README.md` (hoja de ruta), `docs/plans/2026-11-23-f3-v5-agentes-y-uso.md` y `docs/plans/2026-11-23-f4-terminal-acoplada.md`.
- Diseño de referencia: `docs/PLAN.md` ("Cambios de la v4") y la simulación `docs/simulacion/index.html`.
- **Especificación del usuario** (adoptada el 2026-11-23): `docs/ESPECIFICACION.md`, con las decisiones tomadas (se mantiene GPUI, costo estimado, spike de `wry`…). La hoja de ruta pasó a 11 fases (nueva F8 · Vista previa y diff).
- Rama **`fase/f7-archivos`** (sale de `fase/f6-ssh`): Fase 7 · Archivos remotos y transferencias **terminada** (`docs/plans/f7-resultados.md`); faltan las pruebas manuales ⏳ contra un servidor real.
- **Decisión pendiente (sigue abierta tras F7):** panel izquierdo con agentes arriba + árbol abajo a la vez (especificación §3) o el botón "Archivos" que alterna (actual, se mantuvo en F7 sin cambios).

## Ajustes v0.2 · actualizador, firma y Linux/Mac (rama `ajustes/v0.2-update`)
| Parte | Estado |
|---|---|
| Versión del workspace **0.2.0**; CHANGELOG 0.2.0 (faltan títulos de pestaña e imágenes) | ✅ |
| `kuntur-update`: versión más nueva (incluidas preliminares), MSI + SHA-256, descarga verificada, firma (WinVerifyTrust, mismo firmante si el exe en uso está firmado) | ✅ con pruebas; prueba manual contra GitHub real ✅ |
| Aviso en la línea inferior, Configuración → Actualizaciones, instalar con `msiexec /passive` y reabrir Kuntur | ✅ compila y con pruebas; de punta a punta ⏳ (hace falta una 0.2.x publicada) |
| README/SECURITY: la única conexión de red propia es buscar actualizaciones (desactivable) | ✅ |
| Firma: `msi.ps1 -Sign`, SignPath opcional en la CI, `docs/FIRMA.md` | ✅; certificado ⏳ (usuario) |
| Linux/Mac: carpetas por sistema (`kuntur_config::dirs`), `$SHELL`, trabajos `linux`/`macos` en la CI | ✅ (crates de Rust puro verificados con `cargo check --target`); la app completa ⏳ en la CI |

## Ajustes v0.2 · paso 2: árbol de archivos y menú tipo VS Code (rama `ajustes/v0.2-arbol`)
Plan: `docs/plans/2026-11-24-ajustes-v0.2.md` (secciones 5 y 6).
| Parte | Estado |
|---|---|
| `kuntur-files`: `tree.rs` (raíz, desplegadas, filas aplanadas con profundidad, carga perezosa, relectura de lo visible, fila madre), `path.rs` (`is_within`, `relative`), `share.rs` (enlace de GitHub, `usuario@host:ruta`) | ✅ con pruebas (`MemoryFs`) |
| Árbol en el panel: chevron que gira, sangría con guías, ícono por tipo, clic despliega, ←/→/↑/↓/Enter, Esc vuelve a la terminal, plegar todo | ✅ compila; sin captura (esta PC tiene un solo monitor) |
| Menú (`file_menu.rs`, es/en, sin "Diff This"): Nuevo archivo/carpeta y Cambiar nombre en línea, Eliminar (local a la Papelera con `trash`; remoto "ADVERTENCIA: Eliminar esto"), Copiar ruta / relativa, Cortar/Copiar/Pegar (mismo sistema = renombrar; local↔remoto por la cola), Mostrar en el gestor de archivos, Abrir en la terminal integrada, Agregar al chat / Claude Code: Agregar al chat, Descargar/Subir esto, Compartir ▸ (GitHub / `usuario@host:ruta`), Abrir en vista previa de imágenes (hasta 12 imágenes en la vista previa), Agregar como proyecto Python | ✅ con pruebas de la lógica |
| Buscar en la carpeta…, Abrir línea de tiempo | ⏳ en gris "(próximamente)" |
| Portapapeles del sistema (copiar en Kuntur y pegar en el Explorador, y al revés), selección múltiple, arrastrar dentro del árbol | ⏳ pendiente |
| Pruebas manuales | ⏳ lista en el plan (§ Pruebas manuales) |

## Fase 10: MCP y comandos (rama `fase/f10-mcp`)
| Parte | Estado |
|---|---|
| `kuntur-config`: `[[mcp]]`, `[[mcp_perfiles]]`, `[[comandos]]`, lista MCP y perfil por sesión; secretos en el Administrador de credenciales («Kuntur MCP») | ✅ con pruebas |
| `kuntur-mcp`: catálogo, configuración por CLI (Claude `--mcp-config`, Codex `-c mcp_servers.…`, OpenCode `OPENCODE_CONFIG`), opciones, línea por shell, carpeta temporal por sesión, «Probar» stdio/http, capturas, atajos | ✅ con pruebas e integración contra servidores MCP **falsos** |
| Gestor de MCP (`workspace/mcp_view.rs`), ganchos al lanzar agentes (clic, paleta, worktree), capturas → vista previa, comandos con atajo | ✅ capturas con servidores sintéticos |
| Alta/edición de servidores desde la UI, comandos en la paleta, proxy MCP (estado en vivo) | ⏳ pendiente |

Pruebas de humo: `.\scripts\smoke.ps1 -Sessions 2 -Mcp probar` (servidores **sintéticos**: `kuntur-mcp-falso`, uno que no existe, uno http a un puerto cerrado; secretos en memoria) y `-Mcp linea -Commands 'cls'` (pega **sin Enter** la línea de Claude Code generada; nunca la ejecuta).

## Fase 11: pulido y publicación
| Parte | Estado |
|---|---|
| Licencia, ícono (`assets/kuntur.ico`), recursos de `kuntur.exe` (`build.rs`) | ✅ |
| MSI: `crates/kuntur-app/wix/main.wxs`, `scripts\msi.ps1 -Check` (WiX 3.14.1 en `%LOCALAPPDATA%\Programs\WiX`, variable `WIX`, `cargo-wix`) | ✅ generado y revisado con `msiexec /a`; instalarlo ⏳ |
| Notificaciones con AUMID `Kuntur.Kuntur` cuando está instalado | ✅ código y prueba; verlo instalado ⏳ |
| Contraste WCAG (prueba) y teclado en los paneles | ✅ |
| Medición: `.\scripts\smoke.ps1 -Sessions 5 -Panel full -AgentEvents scripts\smoke-agentes.jsonl -MaxRamMB 120 -Measure` | ✅ ver `f11-resultados.md` |
| Documentación pública y capturas `docs/images/` | ✅ |

## Fase 7: archivos remotos y transferencias
| Parte | Estado |
|---|---|
| `kuntur-files`: trait `FileSystem` (`LocalFs`, `MemoryFs`), rutas POSIX/Windows, nombres de conflicto, carpeta del agente, cola de transferencias y trabajador | ✅ con pruebas |
| `kuntur-ssh`: `SftpFs` (`russh-sftp` 4) sobre la misma conexión (`RemoteSession::sftp`) | ✅ integración con el servidor `russh` en proceso + subsistema SFTP sobre una carpeta temporal |
| Panel "Archivos" local o remoto según la pestaña (nunca mezclados), etiqueta y ruta visibles, soltar desde el Explorador, clic derecho (Descargar…, Copiar ruta, Borrar…), cola con progreso/cancelar/conflictos, zona "para el agente" | ✅ capturas con datos sintéticos |
| WSL `\\wsl$\…` como local | ✅; traducir el `cwd` Linux de `wsl.exe` queda ⏳ |
| Servidor real, archivos grandes, arrastre real desde el Explorador | ⏳ lista manual en `f7-resultados.md` |

Pruebas de humo: `.\scripts\smoke.ps1 -Sessions 1 -Connections remote -Files remote` (árbol y cola **sintéticos**, nunca un servidor real).

## Fase 5b: agentes exactos
Pruebas de humo: `.\scripts\smoke.ps1 -Sessions 3 -Panel full -AgentEvents scripts\smoke-agentes.jsonl` (eventos **sintéticos** por el named pipe real: subagentes, Permitir/Rechazar, comando sugerido, anillo de Claude), `-SendSuggestion 3` ("Enviar a terminal" a la acoplada, sin ejecutar) y `-HooksInstall` (instalador contra `target\smoke-claude-settings.json`; nunca `~/.claude`). `kuntur-hook.exe` se compila junto a `kuntur.exe` (`cargo build -p kuntur-app -p kuntur-hook --release`); los ganchos instalados apuntan al que está junto a `kuntur.exe`.

## Fase 5a: agentes configurables, alertas y auditoría
Pruebas de humo: `.\scripts\smoke.ps1 -Sessions 2 -Audit -AuditQuery 'git' -Badges` (auditoría con datos **sintéticos** e insignias de prueba) y `-Config target\smoke-agentes.toml -Launch 'extra:ping'` (agente extra de prueba con `ping`; el archivo vive en `target\`, ver `f5a-resultados.md`). Las pruebas nunca muestran notificaciones de Windows.

## Fase 6: SSH y conexiones
| Parte | Estado |
|---|---|
| `kuntur-term`: trait `Transport` (PTY local o remoto) y `Pane::with_transport` | ✅ con pruebas |
| `kuntur-ssh` (`russh` 0.65 con `ring`): known_hosts, ssh-agent de Windows → llaves → contraseña, secretos en el Administrador de credenciales (solo con "recordar"), diálogos en la terminal, segundo canal, reconexión, agentes del servidor | ✅ con pruebas de integración contra un servidor `russh` en proceso |
| Gestor de conexiones (botón de ícono, Ctrl+Shift+O, paleta), pestañas remotas, acoplada remota, franja "Desconectado · Reconectar" (Ctrl+Shift+R), restaurar sesiones remotas | ✅ capturas con conexiones sintéticas |
| Guía de OSC 7 para bash/zsh/fish/PowerShell | ✅ `docs/osc7-remoto.md` |
| Conexión a un servidor real, ssh-agent real, `sudo apt upgrade` | ⏳ lista manual en `f6-resultados.md` |

Pruebas de humo: `.\scripts\smoke.ps1 -Sessions 1 -Connections list|form|remote` (conexiones **sintéticas**; nunca `~/.ssh` ni credenciales reales).

## Fase 8: vista previa y diff (rama `fase/f8-vista-previa`)
| Parte | Estado |
|---|---|
| Enlaces en la salida: lógica pura `crates/kuntur-term/src/links.rs` (URLs, rutas absolutas/relativas/entre comillas, `:línea`, solo si existen) | ✅ con pruebas |
| Ctrl+mouse subraya, Ctrl+clic abre, clic derecho → menú (`terminal_view/links.rs`) | ✅ captura con `-Links` |
| Crate `kuntur-preview` (tipo, lectura con límites, diff unificado, `git diff`, `notify`, LibreOffice) | ✅ con pruebas |
| Panel `workspace/preview_view.rs`: división terminal \| vista previa, pestañas, separador, Markdown, imágenes/SVG, texto con números, diff a colores, recarga automática | ✅ capturas Dark/Blue |
| Spike `wry` (WebView2 hijo de GPUI) | ✅ funciona: integrado para PDF/HTML (RAM aparte: ~141 MB HTML, ~256 MB PDF, 0 al cerrar) |
| Office → PDF con LibreOffice | ✅ código y detección; sin probar con un documento real (LibreOffice no está instalado aquí) |
| Archivos remotos por SFTP | ⏳ F6/F7 |

Pruebas de humo: `.\scripts\smoke.ps1 -Sessions 1 -Preview 'crates\kuntur-preview\tests\datos\ejemplo.md'` (`-PreviewDiff`, `-PreviewEdit`, `-Links`); el spike: `.\spikes\wry-gpui\medir.ps1 -Step menu`.

## Fase 9: shell y autocompletado (rama `fase/f9-autocompletado`)
| Parte | Estado |
|---|---|
| OSC 133 (`A`/`B`/`D;código`) en el script de arranque de PowerShell; Kuntur pone `C` al enviar Enter | ✅ probado con ConPTY real |
| Lo escrito en el prompt (`Pane::prompt_input`), solo fuera de programas a pantalla completa | ✅ |
| Motor `kuntur-complete`: rutas, historial por carpeta, memoria de carpetas (frecuencia × recencia, `nucleo-matcher`) | ✅ con pruebas |
| Texto fantasma gris en la terminal: Tab/→ aceptan, Esc oculta (`terminal_view/ghost.rs`) | ✅ capturas |
| Bloques: raya en el margen (tenue = bien, roja = error) (`terminal_view/blocks.rs`) | ✅ capturas |
| `autocompletado = true` en `config.toml`; historial en `%APPDATA%\Kuntur\historial\` | ✅ |
| Completar comandos/parámetros sin historial, completado remoto (F6), plegar/copiar bloques | ⏳ pendiente |

Pruebas de humo: `.\scripts\smoke.ps1 -Sessions 1 -Commands 'cls' -Type 'cd C:\Win'` (texto sin Enter) y `-History 'git status','git log --oneline -5' -Type 'git l'` (historial **sintético** en memoria).

## Fase 4: terminal acoplada
| Parte | Estado |
|---|---|
| Lógica pura `crates/kuntur-app/src/dock.rs` (estado por sesión, Ctrl+` como VS Code, límites del separador, carpeta inicial, Ctrl+`/Ctrl+Ñ) | ✅ con pruebas |
| Vista: `crates/kuntur-app/src/workspace/dock_view.rs` (panel debajo de la principal, separador arrastrable, encabezado "Terminal · shell · carpeta" con ×) | ✅ capturas en Blue/Dark/White |
| Ocultar sin perder historial (el PTY sigue vivo; se suelta solo si su shell termina o se cierra la sesión) | ✅ comprobado con `-Dock reopen` |
| Arranca en la carpeta de la sesión (OSC 9;9) o en la personal | ✅ |
| Alto por sesión guardado en `config.toml` (`dock_ratio` de cada sesión, opcional) | ✅ |
| Botón de ícono en la barra, acción en la paleta, cursor hueco en el panel sin teclado | ✅ |
| Abrir en el **worktree de un agente** | ⏳ pendiente para F5 (depende de los worktrees por agente) |

Prueba de humo de la acoplada: `.\scripts\smoke.ps1 -Sessions 2 -Commands 'cd C:\Windows\Media' -Dock reopen -DockCommands 'Get-Location'` (`-Dock open|reopen`).

## Fase 3 (v5): qué está hecho y qué falta
| Parte | Estado |
|---|---|
| Bloque A: lectores de uso (Claude Code, Codex zstd, OpenCode SQLite), estadísticas por período, catálogo/detección de agentes, carpeta actual por OSC 9;9 | ✅ hecho y revisado (90 pruebas) |
| Task 7: sesiones como **pestañas arriba** | ✅ hecho; capturas revisadas en Blue/Dark/White |
| Task 8: **panel izquierdo de agentes** (buscar, Todos/Esperando/Trabajando, instalados con anillo %, "Archivos") | ✅ hecho; capturas del panel completo, compacto y "Archivos" (falta ver un agente real en la lista: va en la lista manual de `f3-resultados.md`) |
| Task 10 (parte): **RAM** abajo a la derecha | ✅ hecho (64–80 MB en las pruebas de humo, objetivo < 90 MB) |
| Filtros **Hoy / 7 días / 30 días / 1 año / Todo** en `kuntur-usage` (especificación §10) | ✅ hecho, con pruebas |
| Task 9: vista **Uso de agentes** (selector de períodos, tarjetas, tokens por hora/día/semana/mes, mapa de horas, modelos, agentes, tareas top, CSV) + botones de ícono Tema y Estadísticas | ✅ hecho; capturas con datos sintéticos en Blue/Dark/White y "Hoy"; probado contra el historial real (~5 s de lectura en segundo plano) |
| Task 9b: **costo estimado** (`pricing.toml` editable) y **almacén SQLite incremental** | ✅ hecho: precios verificados el 2026-11-23 en las páginas oficiales (Anthropic, OpenAI, DeepSeek, Z.ai, MiniMax); sin precio publicado: MiniMax-M2.7, qwen3.8-max, Kimi, glm-5.3-flash-free. Importación real: 0,66 s la primera vez, 0,04 s las siguientes |
| Task 10 (resto): humo con datos sintéticos de uso (`-Usage`), `f3-resultados.md` | ✅ hecho; CI sin comprobar (falta `gh auth login`) |

Pruebas de humo del panel: `.\scripts\smoke.ps1 -Sessions 1 -Panel files -Commands 'cd C:\Windows\Media'` (`-Panel full|compact|files`).
Vista de uso: `.\scripts\smoke.ps1 -Sessions 1 -Usage 30 -Theme dark` (`-Usage hoy|7|30|ano|todo`): usa **datos sintéticos** generados dentro de la app (`kuntur_usage::synthetic`), nunca el historial real.
Comparación con el período anterior: solo si el historial llega hasta el inicio de ese período (Claude Code borra lo de más de 30 días por defecto); si no, la tarjeta dice "sin datos para comparar".
Las capturas se guardan en `target\` (no se suben): muestran rutas de la PC, no publicarlas.

Decisión pendiente ya tomada: la tarjeta "Tokens de entrada" **incluye la caché** (lectura + escritura) y muestra debajo "NN % desde caché".

## Preparar la PC (una sola vez)
1. **Git** y **GitHub CLI** (`winget install GitHub.cli`), y `gh auth login` con la cuenta personal.
2. **Rust**: `winget install Rustlang.Rustup`, luego en una consola nueva `rustup default stable`.
3. **Visual Studio Build Tools** con "Desarrollo de escritorio con C++" (compilador MSVC + Windows SDK): `winget install Microsoft.VisualStudio.BuildTools` y marcar esa carga de trabajo en el instalador.
4. Fuente **Cascadia Mono** (viene con Windows 11 / Windows Terminal).

## Clonar y compilar
```powershell
git clone https://github.com/jrsprescott/kuntur.git
cd kuntur
git switch fase/f4-acoplada
cargo test --workspace            # ~220 pruebas
cargo build -p kuntur-app --release
.\target\release\kuntur.exe       # o doble clic
.\scripts\smoke.ps1               # prueba de humo: captura target\smoke.png y mide RAM
```
La primera compilación tarda varios minutos (GPUI + SQLite).

## Reglas del proyecto (no romperlas)
- **Privacidad:** nada de rutas personales, nombres de instituciones ni datos reales de historiales en el repo. Las pruebas usan datos sintéticos. La prueba `crates/kuntur-usage/tests/datos_reales.rs` está `#[ignore]` y solo imprime totales.
- **Antes de cada commit:** `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- **Pruebas visuales:** solo con `scripts/smoke.ps1` (entrada por `KUNTUR_SMOKE_INPUT`, captura con PrintWindow). **Nunca** simular teclado del sistema (SendKeys): las teclas pueden caer en otra ventana.
- **Diseño:** minimalista (una barra arriba, jerarquía por texto, color solo para estado), atajos con Ctrl+Shift (excepción heredada de VS Code: Ctrl+` para la terminal acoplada; en teclados en español, Ctrl+Ñ).
- Si en la PC el `PATH` del usuario contiene la entrada `%PATH%` (se infla y Windows lo descarta), Kuntur no encontrará `claude`/`npm`: quitar esa entrada.

## Sincronizar entre PCs
- Al terminar en una PC: `git add -A`, `git commit`, `git push`.
- Al llegar a la otra: `git status` (que esté limpio) y `git pull` en la rama de trabajo (hoy `fase/f4-acoplada`).
- El ejecutable compilado de cada avance se publica como *pre-release* privada en GitHub (Releases), para probar sin compilar.
