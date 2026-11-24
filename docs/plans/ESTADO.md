# Estado del proyecto — nota de traspaso

> Última actualización: 2026-11-23. Léela primero al retomar en otra PC (o pídele a Claude Code que la lea).

## Dónde está cada cosa
- Rama principal `main`: **Fases 1 y 2** (terminal real + sesiones, temas, paleta, persistencia). Estable.
- Rama **`fase/f3-v5`**: Fase 3 (diseño v5) **terminada** (`docs/plans/f3-resultados.md`).
- Rama de trabajo **`fase/f4-acoplada`** (sale de `fase/f3-v5`): Fase 4 · Terminal acoplada **terminada** (`docs/plans/f4-resultados.md`); faltan las pruebas manuales ⏳ del usuario. La **siguiente fase es F5** (agentes exactos, alertas y auditoría), que se empieza desde esta rama.
- Rama **`fase/f6-ssh`** (sale de `fase/f4-acoplada`): Fase 6 · SSH y conexiones **terminada** (`docs/plans/f6-resultados.md`); faltan las pruebas manuales ⏳ contra un servidor real. Se trabajó en paralelo a F5: al integrar, revisar los ganchos en `workspace.rs` (lista en `f6-resultados.md`).
- Planes: `docs/plans/README.md` (hoja de ruta), `docs/plans/2026-11-23-f3-v5-agentes-y-uso.md` y `docs/plans/2026-11-23-f4-terminal-acoplada.md`.
- Diseño de referencia: `docs/PLAN.md` ("Cambios de la v4") y la simulación `docs/simulacion/index.html`.
- **Especificación del usuario** (adoptada el 2026-11-23): `docs/ESPECIFICACION.md`, con las decisiones tomadas (se mantiene GPUI, costo estimado, spike de `wry`…). La hoja de ruta pasó a 11 fases (nueva F8 · Vista previa y diff).
- Rama **`fase/f7-archivos`** (sale de `fase/f6-ssh`): Fase 7 · Archivos remotos y transferencias **terminada** (`docs/plans/f7-resultados.md`); faltan las pruebas manuales ⏳ contra un servidor real.
- **Decisión pendiente (sigue abierta tras F7):** panel izquierdo con agentes arriba + árbol abajo a la vez (especificación §3) o el botón "Archivos" que alterna (actual, se mantuvo en F7 sin cambios).

## Fase 7: archivos remotos y transferencias
| Parte | Estado |
|---|---|
| `kuntur-files`: trait `FileSystem` (`LocalFs`, `MemoryFs`), rutas POSIX/Windows, nombres de conflicto, carpeta del agente, cola de transferencias y trabajador | ✅ con pruebas |
| `kuntur-ssh`: `SftpFs` (`russh-sftp` 4) sobre la misma conexión (`RemoteSession::sftp`) | ✅ integración con el servidor `russh` en proceso + subsistema SFTP sobre una carpeta temporal |
| Panel "Archivos" local o remoto según la pestaña (nunca mezclados), etiqueta y ruta visibles, soltar desde el Explorador, clic derecho (Descargar…, Copiar ruta, Borrar…), cola con progreso/cancelar/conflictos, zona "para el agente" | ✅ capturas con datos sintéticos |
| WSL `\\wsl$\…` como local | ✅; traducir el `cwd` Linux de `wsl.exe` queda ⏳ |
| Servidor real, archivos grandes, arrastre real desde el Explorador | ⏳ lista manual en `f7-resultados.md` |

Pruebas de humo: `.\scripts\smoke.ps1 -Sessions 1 -Connections remote -Files remote` (árbol y cola **sintéticos**, nunca un servidor real).

## Fase 6: SSH y conexiones
| Parte | Estado |
|---|---|
| `kuntur-term`: trait `Transport` (PTY local o remoto) y `Pane::with_transport` | ✅ con pruebas |
| `kuntur-ssh` (`russh` 0.65 con `ring`): known_hosts, ssh-agent de Windows → llaves → contraseña, secretos en el Administrador de credenciales (solo con "recordar"), diálogos en la terminal, segundo canal, reconexión, agentes del servidor | ✅ con pruebas de integración contra un servidor `russh` en proceso |
| Gestor de conexiones (botón de ícono, Ctrl+Shift+O, paleta), pestañas remotas, acoplada remota, franja "Desconectado · Reconectar" (Ctrl+Shift+R), restaurar sesiones remotas | ✅ capturas con conexiones sintéticas |
| Guía de OSC 7 para bash/zsh/fish/PowerShell | ✅ `docs/osc7-remoto.md` |
| Conexión a un servidor real, ssh-agent real, `sudo apt upgrade` | ⏳ lista manual en `f6-resultados.md` |

Pruebas de humo: `.\scripts\smoke.ps1 -Sessions 1 -Connections list|form|remote` (conexiones **sintéticas**; nunca `~/.ssh` ni credenciales reales).

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
cargo test --workspace            # ~150 pruebas
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
