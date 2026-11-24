# F5 · Agentes exactos, alertas y auditoría — Resumen de la fase

Rama `fase/f5-agentes` (sale de `fase/f4-acoplada`). Plan: [`2026-11-23-f5-agentes-alertas-auditoria.md`](2026-11-23-f5-agentes-alertas-auditoria.md). Detalle de cada mitad: [`f5a-resultados.md`](f5a-resultados.md) y [`f5b-resultados.md`](f5b-resultados.md).

## Qué quedó hecho ✅

| Parte | Mitad | Estado |
|---|---|---|
| Catálogo configurable (Kimi, DeepSeek, Antigravity, `[[agentes]]` en `config.toml`), instalados bajo demanda | F5a | ✅ |
| Clic en un agente instalado = abrirlo (sesión nueva si la terminal está ocupada) | F5a | ✅ |
| Agentes de la terminal acoplada | F5a | ✅ |
| Alertas: insignia en la pestaña + notificación de Windows | F5a (heurística) → F5b (exactas, "pide permiso") | ✅ |
| Auditoría de comandos (Claude Code, Codex): búsqueda, filtros, CSV | F5a | ✅ |
| `kuntur-hook.exe` + named pipe `\\.\pipe\kuntur-<pid>` | F5b | ✅ |
| OSC 7501 (respaldo y agentes sin hooks) | F5b | ✅ |
| Estado exacto: trabajando / pide permiso / espera respuesta / terminó | F5b | ✅ |
| Subagentes con su acción actual en la lista compacta | F5b | ✅ |
| Permitir / Rechazar desde la fila (solo por clic) | F5b | ✅ |
| Worktree por agente y acoplada en el worktree (pendiente de F4) | F5b | ✅ |
| "Enviar a terminal" (comando sugerido y selección, sin ejecutar) | F5b | ✅ |
| Puente de la línea de estado de Claude (anillo 5 h / semanal) e instalador de ganchos con confirmación | F5b | ✅ (instalación real: ⏳ usuario) |
| Auditoría también desde los hooks | — | ⏳ queda pendiente |

## Números

- `cargo test --workspace`: **222 pruebas** (186 al cerrar F5a), clippy y fmt sin advertencias.
- RAM en las pruebas de humo de la fase: **64–67 MB** (objetivo < 90 MB).
- Crates nuevos: `kuntur-audit` (F5a) y `kuntur-hook` (F5b).

## Lo que debe probar / aprobar el usuario

Las listas ⏳ de [`f5a-resultados.md`](f5a-resultados.md) y [`f5b-resultados.md`](f5b-resultados.md). Lo principal: **instalar los ganchos de Claude Code** desde la paleta (Kuntur muestra exactamente qué agrega a `~/.claude/settings.json`, hace copia y solo escribe al pulsar Instalar) y comprobar con un `claude` real el estado exacto, los subagentes, Permitir/Rechazar y el anillo.
