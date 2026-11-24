# F3 · Diseño v5: pestañas, agentes y uso — Resultados

Rama `fase/f3-v5`. Tasks 1–10 del plan [`2026-11-23-f3-v5-agentes-y-uso.md`](2026-11-23-f3-v5-agentes-y-uso.md), con los cambios de la [especificación del usuario](../ESPECIFICACION.md) (filtros Hoy/7/30/1 año/Todo, costo estimado, CSV, almacén SQLite).

## Verificación automática ✅

| Verificación | Resultado |
|---|---|
| `cargo test --workspace` | ✅ 139 pruebas (+2 ignoradas a propósito que leen el historial real y solo imprimen totales) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ sin advertencias |
| `cargo fmt --all -- --check` | ✅ |
| CI de GitHub | ⏳ sin comprobar desde esta PC (falta `gh auth login`; el repositorio es privado) |

Lo nuevo con pruebas: lectores de Claude Code / Codex (zstd) / OpenCode (SQLite solo lectura), agregaciones por período (con comparación solo si el historial cubre el período anterior), barras por hora/día/semana/mes, CSV seguro para Excel, datos sintéticos, tabla de precios (coincidencia estricta de modelos), almacén incremental, filtros y anillo del panel de agentes, formatos de la vista de uso.

### Pruebas de humo (`scripts/smoke.ps1`)

| Comando | RAM | Qué muestra la captura |
|---|---|---|
| `-Sessions 3 -Theme blue/dark/white` | 64–80 MB | Pestañas arriba (activa resaltada, `×`, `+`), panel compacto con agentes instalados y anillo de Codex, línea inferior con carpeta y RAM |
| `-Sessions 1 -Theme white -Panel full` | 67 MB | Panel completo: buscador, Todos/Esperando/Trabajando, "No hay agentes corriendo", instalados, "Archivos" |
| `-Sessions 1 -Panel files -Commands 'cd C:\Windows\Media'` | 66 MB | Explorador: carpeta primero, tamaño y fecha en columnas alineadas, sigue al `cd` de la sesión |
| `-Usage 30 / ano / hoy` (Dark, White, Blue) | 72–76 MB | Vista de uso con datos sintéticos: 5 tarjetas (con costo), gráfico por día / semana / hora, mapa día × hora, agentes, modelos con costo, tareas top |

Historial real (prueba ignorada, solo números): 3 895 registros; importación incremental 0,66 s la primera vez y 0,04 s después.

## Pruebas manuales ⏳ (para el usuario)

Abrir `target\release\kuntur.exe` normal.

**Pestañas**
- [ ] ⏳ Doble clic en una pestaña renombra; clic medio la cierra; `+` crea una sesión; muchas pestañas se desplazan.
- [ ] ⏳ Una pestaña con un agente trabajando o esperando muestra su punto de color.

**Panel de agentes**
- [ ] ⏳ Abrir `claude`, `codex` y `opencode` en sesiones distintas: aparecen en la lista con su sesión, estado y tiempo; clic en la fila va a esa sesión.
- [ ] ⏳ Filtros Todos / Esperando / Trabajando y el buscador ("codex", nombre de la sesión) filtran bien.
- [ ] ⏳ El anillo de Codex muestra su tooltip con 5 h / semanal.
- [ ] ⏳ Ctrl+Shift+B alterna completo ↔ compacto; Ctrl+Shift+E abre "Archivos".

**Archivos**
- [ ] ⏳ Sigue al `cd` de la sesión; clic en carpeta entra; ↑ sube; doble clic en un archivo lo abre con su programa; clic derecho copia la ruta.

**Uso de agentes**
- [ ] ⏳ Ctrl+Shift+U, el botón de gráfico o la paleta la abren; Esc o una pestaña vuelven a la terminal.
- [ ] ⏳ Los 5 períodos cambian todo; los tooltips de las barras y del mapa muestran los números.
- [ ] ⏳ "Exportar CSV" guarda un archivo que Excel abre con tildes correctas.
- [ ] ⏳ "Uso de agentes: editar precios" abre `%APPDATA%\Kuntur\pricing.toml`; un precio agregado se ve al volver a abrir la vista.
- [ ] ⏳ El botón de paleta (Tema) abre los 3 temas; elegir uno lo aplica y se guarda.

## Notas

- El costo es **precio de API** (no lo que cobra una suscripción). Modelos sin precio publicado al 2026-11-23 (MiniMax-M2.7, qwen3.8-max, Kimi, glm-5.3-flash-free) no suman y se cuentan como "turnos sin precio".
- El % de cambio solo aparece si el historial cubre el período anterior completo (Claude Code borra lo de más de 30 días por defecto).
- Estado esperando/trabajando: heurística por actividad (exacto en F5).
- Autocompletado en gris estilo Warp: F9.
