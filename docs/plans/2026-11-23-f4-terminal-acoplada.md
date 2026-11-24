# F4 · Terminal acoplada — Plan de implementación

> **Para agentes:** ejecutar tarea por tarea (método de F1–F3). Casillas `- [ ]`.

**Objetivo:** cada sesión puede abrir un **segundo panel de terminal debajo de la principal** (como VS Code), sin IA, con su propio PTY y el mismo shell de la sesión. Criterio de aceptación de [`PLAN.md`](../PLAN.md) §6: *"Abrir/cerrar sin perder historial; sigue la ruta de la sesión"*. Diseño de referencia: [`PLAN.md`](../PLAN.md) §5.3 y la simulación (`.dock`: alto inicial ~250 px, mínimo 110 px, borde superior arrastrable, encabezado con "Terminal", el shell y "Ocultar (Ctrl+`)").

**Qué entra en F4:**
1. **Ctrl+`** (y "Terminal acoplada" en la paleta y un botón de ícono en la barra) abre / enfoca / oculta el panel acoplado de la sesión activa.
2. **Alto ajustable** arrastrando el separador; la proporción se guarda **por sesión** (en memoria y en `config.toml`, campo opcional `dock_ratio` de cada sesión).
3. El panel arranca en la **carpeta actual de la sesión** (`Pane::cwd()` por OSC 9;9); si no se conoce o ya no existe, en la carpeta personal.
4. Ocultar **no destruye**: el PTY y el historial siguen vivos. Se destruye solo si su shell termina (`exit`) o se cierra la sesión.
5. Foco: Ctrl+` alterna como VS Code (cerrado → abre y enfoca; abierto sin foco → lo enfoca; abierto con foco → oculta y vuelve a la principal). Clic en un panel lo enfoca; cambiar de pestaña recuerda qué panel tenía el foco. Los atajos Ctrl+Shift siguen funcionando desde los dos paneles.

**Fuera de F4 (pendiente):** abrir la acoplada en el **worktree de un agente** (selector "en:" y botón "↧ Terminal aquí") depende de F5 (worktree por agente). Canal SSH acoplado: F6. "Enviar a terminal": F5.

**Teclado en español:** GPUI informa la tecla según la distribución. En la de EE. UU. la tecla a la izquierda del 1 es "`"; en las distribuciones en español esa posición física (VK_OEM_3) es la **Ñ**, igual que hace VS Code. Se aceptan **Ctrl+`** y **Ctrl+Ñ**.

---

## Archivos

| Archivo | Acción | Responsabilidad |
|---|---|---|
| `crates/kuntur-app/src/dock.rs` | Crear | Lógica pura (TDD): `DockState` (abierto, proporción, panel con foco), `toggle`, límites del separador, carpeta inicial, reconocimiento de Ctrl+` / Ctrl+Ñ |
| `crates/kuntur-app/src/sessions.rs` | Modificar | `get` / `get_mut` por id (con pruebas) |
| `crates/kuntur-config/src/settings.rs` | Modificar | `SessionConfig::dock_ratio: Option<f32>` (opcional; archivos viejos siguen cargando) |
| `crates/kuntur-app/src/command_palette.rs` | Modificar | Acción `ToggleDock` ("Terminal acoplada: mostrar/ocultar", Ctrl+`) |
| `crates/kuntur-app/src/terminal_view.rs` | Modificar | `is_focused(window)` |
| `crates/kuntur-app/src/workspace.rs` + `workspace/dock_view.rs` | Modificar / Crear | Panel por sesión, separador arrastrable (`on_drag` + `on_drag_move`), encabezado, foco, persistencia, gancho de humo |
| `crates/kuntur-app/src/workspace/title_bar.rs` | Modificar | Botón de ícono "Terminal acoplada (Ctrl+`)" |
| `scripts/smoke.ps1` | Modificar | `-Dock open|reopen` y `-DockCommands` (`KUNTUR_SMOKE_DOCK`, `KUNTUR_SMOKE_DOCK_INPUT`) |
| `docs/plans/f4-resultados.md` | Crear | Aceptación |

## Tareas

- [ ] **Task 1 — `dock.rs` (TDD).** Pruebas primero: estado inicial cerrado con foco en la principal; `toggle` en sus tres casos; el panel terminado vuelve al estado cerrado sin perder la proporción; la proporción por puntero respeta mínimos (110 px el acoplado, 140 px la principal) y los límites 0,15–0,85; valores rotos (NaN, fuera de rango) del archivo se corrigen; carpeta inicial: la de la sesión si existe, si no la personal; Ctrl+` y Ctrl+Ñ sí, con Shift/Alt no.
- [ ] **Task 2 — Configuración y sesiones.** `dock_ratio` opcional (prueba de ida y vuelta y de archivo viejo sin el campo); `SessionList::get/get_mut`.
- [ ] **Task 3 — Paleta.** `ToggleDock` con su prueba de filtro ("acoplada").
- [ ] **Task 4 — Workspace.** `Session { view, dock: Option<Entity<TerminalView>>, dock_state, … }`; crear la vista acoplada con el shell de la sesión y la carpeta de Task 1; `Exited` del acoplado → se suelta y el foco vuelve a la principal; tema y fuente se aplican también al acoplado; la línea inferior muestra la carpeta del panel con foco.
- [ ] **Task 5 — Dibujo.** Área central: principal (`flex_1`) + separador de 5 px (cursor ↕, acento al pasar) + encabezado de 24 px (ícono, "Terminal", shell y carpeta en tenue, botón ×) + terminal acoplada con `h(relative(ratio))`. Clic en cualquier panel actualiza el foco guardado. La proporción se guarda al soltar (`on_drop`).
- [ ] **Task 6 — Humo.** `KUNTUR_SMOKE_DOCK=open|reopen`: abre el acoplado después de los comandos de la principal (para que herede su carpeta); `reopen` además lo oculta y lo vuelve a abrir para comprobar que el historial sigue. `smoke.ps1 -Dock` (conservar BOM UTF-8).
- [ ] **Task 7 — Verificación.** `cargo fmt --all`, `clippy -D warnings`, `cargo test --workspace`; capturas en Blue/Dark/White con RAM (< 100 MB, presupuesto de F4); `f4-resultados.md`, `ESTADO.md`, `README.md`.

## Pruebas manuales que quedan para el usuario
Arrastrar el separador con el mouse real, Ctrl+` con teclado físico (EE. UU. y español), `exit` en el acoplado, cerrar la sesión con el acoplado abierto, reabrir Kuntur y comprobar que el alto se recuerda.
