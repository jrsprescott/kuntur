# F5b · Agentes exactos — Resultados

Rama `fase/f5-agentes` (después de F5a). Plan: [`2026-11-23-f5-agentes-alertas-auditoria.md`](2026-11-23-f5-agentes-alertas-auditoria.md) (sección F5b). Especificación §3, §5 y §12; diseño `docs/PLAN.md` §5.1, §5.3 y §5.4.

## Verificación automática ✅

| Verificación | Resultado |
|---|---|
| `cargo test --workspace` | ✅ 222 pruebas (+36 respecto de F5a; +2 ignoradas a propósito, las del historial real) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ sin advertencias |
| `cargo fmt --all -- --check` | ✅ |
| CI de GitHub | ⏳ sin comprobar desde esta PC |

Lo nuevo con pruebas:
- **`kuntur-hook`** (crate nuevo: biblioteca + `kuntur-hook.exe`, 17): conversión de la entrada de los hooks de Claude Code (resúmenes por herramienta, subagentes, `Notification`, `Stop`, eventos ignorados, recortes sin partir caracteres), línea de estado (`rate_limits`), codificación por líneas; instalador sobre texto (archivo nuevo, conserva lo del usuario y el orden de sus claves, no duplica, comillas con espacios, archivos inválidos, **BOM**, escritura con copia de seguridad en carpeta temporal). **Named pipe real en proceso** (servidor y cliente en la prueba): mensajes en orden recuperable por el sello `ts`, 24 clientes a la vez sin perder ninguno, sin Kuntur vuelve en < 0,5 s, el nombre no se puede robar y se libera al cerrar, y el **binario real** reenvía y no escribe nada (ni en stdout ni en stderr; código 0 con basura o sin Kuntur).
- **`kuntur-term`**: escáner OSC genérico (`osc.rs`, lo usa también OSC 9;9), OSC 7501 (`agent_signal.rs`: forma completa y corta, alias en español, inválidos, partido entre lecturas, recortes) y prueba real con ConPTY (variables `env` y señal 7501 que no se ve en pantalla).
- **`kuntur-agents`**: `exact.rs` (ciclo de un turno, permisos y notificaciones, subagentes con su acción, eventos viejos que no pisan nuevos, límite de subagentes, señales 7501), `suggest.rs` (bloques de shell, consola con prompts, repetidos, límite, peligrosos, varias líneas en una sin ejecutar), `permission.rs`, `worktree.rs` (nombres libres y **worktree real** en un repositorio temporal) y alertas con estado exacto (avisan en la primera lectura; `AlertKind::Permission`).
- **`kuntur-app`**: textos de la fila con estado exacto y claves de fila; paleta (worktree, ganchos, enviar selección).

Rendimiento del gancho: `kuntur-hook.exe` (285 KB) tarda ~9 ms en promedio, arranque del proceso incluido, sin Kuntur escuchando.

### Pruebas de humo (`scripts/smoke.ps1`)

| Comando | RAM | Qué muestra la captura |
|---|---|---|
| `-Sessions 3 -Panel full -Theme dark -AgentEvents scripts\smoke-agentes.jsonl` | 65 MB | Eventos **sintéticos** mandados por el named pipe real: sesión 1 trabajando con "Agent · Revisar…" y 4 subagentes (Explore `Grep · "fn render"`, Plan `Read · workspace.rs`, general-purpose `Bash · cargo t…`, Explore terminado en gris; contador 3/4); sesión 2 "pide permiso" con `Bash · rm -rf dist` y **Permitir / Rechazar**; sesión 3 "terminó" con `$ npm run build` y **Enviar a terminal / Copiar**; anillo de Claude en "instalados" con el 42 % de la línea de estado; puntos de las pestañas exactos |
| Igual en Blue con `-SendSuggestion 3` | 66 MB | "Clic" en Enviar a terminal: se abrió la acoplada de la sesión 3 y quedó escrito `npm run build` **sin ejecutarse** |
| `-Sessions 2 -Panel full -Theme white -HooksInstall -Commands '… OSC 7501 …'` | 64 MB | OSC 7501 escrito por la propia terminal: fila "Codex · pide permiso · Bash · cargo build --release" con botones (agente conocido solo por la señal) |
| `-Sessions 1 -Panel full -Theme white -HooksInstall` | 67 MB | Instalador contra `target\smoke-claude-settings.json`: ruta, fragmento JSON exacto que se agrega, "11 eventos; no se borra ni cambia nada", línea de estado, copia de seguridad; botones Cancelar / Instalar (la prueba no pulsa Instalar; el archivo quedó igual) |

`scripts/smoke-agentes.jsonl` son eventos sintéticos (sin datos reales). Ninguna prueba abre `claude` ni toca `~/.claude`; ninguna muestra notificaciones de Windows.

## Pruebas manuales ⏳ (para el usuario)

- [ ] ⏳ **Instalar ganchos** (paleta → "Claude Code: instalar ganchos de Kuntur"): revisar el fragmento, pulsar **Instalar** y comprobar que quedó `~/.claude/settings.json.kuntur-<fecha>.bak` y que el archivo conserva lo anterior. Kuntur debe correr desde la carpeta donde quedará (los ganchos apuntan al `kuntur-hook.exe` que está junto a `kuntur.exe`).
- [ ] ⏳ Abrir `claude` en Kuntur después de instalar: la fila pasa a "trabajando" con la acción ("Bash · npm test"), los subagentes aparecen debajo, y al pedir permiso dice "pide permiso" con Permitir / Rechazar.
- [ ] ⏳ **Permitir** responde `1` (primera opción, "Sí") y **Rechazar** Esc. Confirmar que en la versión instalada de Claude Code `1` elige y confirma; en Codex, `y` / Esc.
- [ ] ⏳ Al terminar un turno con un bloque ```bash/```powershell, "Enviar a terminal" lo escribe en la acoplada sin Enter; "Terminal: enviar selección a la acoplada" hace lo mismo con la selección.
- [ ] ⏳ Anillo de Claude Code en "instalados" (5 h; tooltip con semanal) después de la primera respuesta (solo cuentas Pro/Max: con clave de API no hay `rate_limits`). Si ya tenías otra línea de estado, el instalador no la toca y el anillo queda sin datos.
- [ ] ⏳ "Agentes: abrir Claude Code en un worktree nuevo" con la sesión en un repositorio: crea `<repo>-claude-1` (rama `kuntur/claude-1`), abre la sesión ahí y lanza el agente; Ctrl+` abre la acoplada en el worktree.
- [ ] ⏳ Alertas con estado exacto: en otra pestaña, "pide permiso" avisa enseguida ("Claude Code pide permiso").

## Notas y decisiones

- **Gancho**: un solo binario para todos los eventos (el nombre viene en `hook_event_name`). Sin `KUNTUR_PIPE`/`KUNTUR_SESSION` (Claude fuera de Kuntur) no hace nada; siempre sale con 0, sin stderr, y como gancho no escribe en stdout (en `UserPromptSubmit` se agregaría al contexto). Timeout de 5 s en `settings.json` (tarda milisegundos). No viaja el texto del usuario ni el contenido de los archivos: solo herramienta + resumen corto y, en `Stop`, el último mensaje (16 KB como mucho) para los comandos sugeridos.
- **Named pipe** `\\.\pipe\kuntur-<pid>`: primera instancia exclusiva, sin clientes remotos, 4 instancias escuchando (subagentes en paralelo); el cliente reintenta ~1 s si todas están ocupadas. Como varias instancias pueden entregar fuera de orden, cada evento lleva `ts` (µs) y el estado descarta lo más viejo que lo último aplicado.
- **Identidad**: cada terminal (principal o acoplada) recibe su `KUNTUR_SESSION` (`k1`, `k2`…). Si los eventos llegan de una terminal donde no se reconoce el proceso (p. ej. solo OSC 7501), la fila aparece igual; si el proceso del agente se vio y desaparece (se cerró sin `SessionEnd`), el estado exacto se olvida.
- **Estados**: trabajando (`UserPromptSubmit`/`PreToolUse`), pide permiso (`PermissionRequest` o `Notification` `permission_prompt`), espera respuesta (`idle_prompt`, `elicitation_dialog`), terminó (`Stop`). Para filtros, pestañas y alertas los tres últimos cuentan como "esperando". La heurística por actividad queda solo para agentes sin eventos.
- **OSC 7501** (formato propio, no hay estándar): `ESC ] 7501 ; state=<working|permission|waiting|done|end> [; agent=<id>] [; action=<texto>] BEL`; también `ESC ] 7501 ; working BEL` y alias en español. Se lee cada 2 s (respaldo).
- **Permitir/Rechazar** solo para Claude Code (`1` / Esc) y Codex (`y` / Esc); otros agentes muestran "pide permiso" sin botones. Solo por clic.
- **Enviar a terminal**: nunca ejecuta; un comando de varias líneas se une en una (`;`, `&` en cmd; las continuaciones `\`, `` ` ``, `^` se respetan) porque sin bracketed paste cada salto sería un Enter. Si la acoplada tiene otro programa corriendo no se le escribe. Los comandos peligrosos (`sudo`, `rm -rf`, `git push --force`…) se ven en ámbar con aviso.
- **Worktree**: carpeta hermana `<repo>-<agente>-<n>` y rama `kuntur/<agente>-<n>` (el primer `n` libre). La sesión no recuerda el worktree al reiniciar Kuntur (las sesiones guardadas no tienen carpeta).
- **Instalador**: agrega un gancho por evento sin tocar los del usuario, conserva el orden de las claves (`serde_json` con `preserve_order`), acepta BOM, escribe por un temporal y deja copia `.kuntur-<fecha>.bak`. La línea de estado solo se agrega si no hay otra. En pruebas de humo sin `KUNTUR_CLAUDE_SETTINGS` el instalador no tiene ruta: nunca puede tocar el real.
- **Archivos compartidos** (otras fases): solo ganchos agregados — `workspace.rs` (módulo, campo `exact`, arranque, `poll_exact`, filas con estado exacto, alertas `Permission`, carpeta de sesión nueva, 4 acciones de paleta, overlay), `command_palette.rs` (4 acciones), `title_bar.rs` (estado de la pestaña), `terminal_view.rs` (4 métodos). Lo demás en módulos nuevos (`workspace/agents_exact.rs`).

## Pendiente / riesgos

- **Auditoría desde los hooks** (comandos de cualquier agente con hook) no entró: la auditoría sigue leyendo los registros de Claude Code y Codex.
- La tecla `1` para "Sí" en Claude Code está tomada de su menú numerado; si una versión futura cambia el diálogo, Permitir no haría nada (falla segura) — revisar en la prueba manual.
- Si `kuntur.exe` corre desde `target\release`, los ganchos instalados apuntan ahí; al mover Kuntur (MSI de F11) hay que reinstalarlos.
- Los worktrees no se borran desde Kuntur (`git worktree remove` a mano).
- Agentes remotos (F6) dependerán solo de OSC 7501.
