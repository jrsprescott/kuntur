# Kuntur — Hoja de ruta de desarrollo (Rust · Windows)

> Producto y diseño: [`docs/PLAN.md`](../PLAN.md) (v4). Simulación: [`docs/simulacion/index.html`](../simulacion/index.html).
> Este archivo ordena **cómo** se construye. Cada fase tiene su propio plan detallado (paso a paso, con código y pruebas),
> que se escribe al empezar la fase para usar las APIs reales de ese momento. Cada fase termina en software que funciona.

## Principios
1. **Cada fase entrega algo usable** y medible (RAM, pruebas, captura).
2. **TDD** en la lógica (`kuntur-*` sin UI): prueba que falla → código mínimo → prueba que pasa → commit.
3. **La UI solo lee "fotos"** (`Screen`) del estado; la lógica vive en crates sin GPUI, probables sin ventana.
4. **Antes de cada commit**: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
   y buscar que no se cuelen datos personales o de terceros (ver la regla en `README.md`).
5. Presupuesto de RAM por fase (medido con `scripts/smoke.ps1`): F1 < 80 MB · F4 < 100 MB · F10 < 120 MB.

## Fases

| Fase | Qué entrega | Crates que toca | Depende de | Plan detallado |
|---|---|---|---|---|
| **F1 · Núcleo de terminal** | Ventana GPUI con PowerShell real: colores, cursor, scroll, selección, copiar/pegar (Ctrl+C inteligente, Ctrl+V, clic derecho, bracketed paste, OSC 52), redimensionar | `kuntur-term`, `kuntur-app` | — | [`2026-11-23-f1-nucleo-terminal.md`](2026-11-23-f1-nucleo-terminal.md) ✅ |
| **F2 · Sesiones y temas** | Varias sesiones; riel lateral (completo ↔ íconos, nunca oculto) con tarjeta al pasar el mouse; barra de título propia minimalista; temas Blue/Dark/White desde `themes/*.toml`; Ctrl+K (apariencia/atajos); guardar y restaurar sesiones | `kuntur-config`, `kuntur-app` | F1 | Se escribe al iniciar F2 |
| **F3 · Terminal acoplada** | Segundo panel por sesión (Ctrl+`), alto ajustable, en la ruta de la sesión o del worktree de un agente | `kuntur-term`, `kuntur-app` | F2 | Se escribe al iniciar F3 |
| **F4 · Agentes** | `kuntur-hook.exe` + named pipe; OSC 7501; árbol compacto agente → subagentes con acción actual; permitir desde la fila; worktree por agente; "Enviar a terminal"; perfiles `agents.toml` (OpenCode, Qwen…) | `kuntur-agents`, `kuntur-term`, `kuntur-app` | F3 | Se escribe al iniciar F4 |
| **F5 · Uso y costos** | Línea compacta + detalle: statusLine de Claude, rollouts de Codex, tok/s, límites 5 h / semanal | `kuntur-usage`, `kuntur-app` | F4 | Se escribe al iniciar F5 |
| **F6 · SSH** | `russh`: sesión remota + canal acoplado en la misma conexión, `known_hosts`, ssh-agent de Windows, OSC 7/133 sin tocar el servidor | `kuntur-term`, `kuntur-app` | F3 | Se escribe al iniciar F6 |
| **F7 · Archivos** | Panel Windows ↔ Linux (local, SFTP, WSL), arrastrar/soltar, CF_HDROP, cola de transferencias | `kuntur-files`, `kuntur-app` | F6 | Se escribe al iniciar F7 |
| **F8 · Shell y autocompletado** | Integración de shell (OSC 133/7), bloques, editor de entrada, rutas + memoria de carpetas + historial | `kuntur-complete`, `kuntur-term`, `kuntur-app` | F2 | Se escribe al iniciar F8 |
| **F9 · MCP y comandos** | MCP por sesión/perfil, modelo/esfuerzo/permisos, comandos con atajo por sesión o globales | `kuntur-config`, `kuntur-agents`, `kuntur-app` | F4 | Se escribe al iniciar F9 |
| **F10 · Pulido y publicación** | Instalador MSI, notificaciones de Windows, accesibilidad, medición final, licencia, README en inglés, CONTRIBUTING, repositorio público | todos | F1–F9 | Se escribe al iniciar F10 |

Orden recomendado: **F1 → F2 → F3 → F4 → F5 → F6 → F7 → F8 → F9 → F10**. F8 puede adelantarse si se prefiere el autocompletado antes que SSH.

## Hechos verificados en este equipo (prototipo del 2026-11-23)
Se compiló y ejecutó un prototipo con exactamente el código del plan F1:
- **25 pruebas** de `kuntur-term` pasan, incluidas 2 con **ConPTY real** (`cmd.exe` y escritura de teclas); `clippy -D warnings` limpio.
- Ventana GPUI con **PowerShell 7 real**: colores ANSI/256/truecolor, negrita/cursiva/subrayado, `ñ`, acentos, CJK y `✓` correctos.
- **RAM: 57–71 MB** (objetivo F1: < 80 MB). Binario release: 19 MB.
- ConPTY envía `ESC[6n` al arrancar y **espera la respuesta**: el emulador debe contestar los `PtyWrite` (resuelto en `Pane`).
- Si el entorno tiene `NO_COLOR=1`, PowerShell quita los colores: el script de humo lo limpia antes de lanzar la app.
- APIs de GPUI 0.3.14 (vía gpui-kit 0.11) que difieren de ejemplos viejos: `window.focus(&h, cx)`; `ShapedLine::paint(origin, line_h, TextAlign::Left, None, window, cx)`; `ScrollEvent` y `.on_scroll(...)`.
- Fuente por defecto: **Cascadia Mono** (viene con Windows 11).
