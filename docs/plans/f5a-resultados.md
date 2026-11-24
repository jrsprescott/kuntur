# F5a · Agentes configurables, alertas y auditoría — Resultados

Rama `fase/f5-agentes` (sale de `fase/f4-acoplada`). Plan: [`2026-11-23-f5-agentes-alertas-auditoria.md`](2026-11-23-f5-agentes-alertas-auditoria.md). Especificación §3, §5, §12 y §13.

## Verificación automática ✅

| Verificación | Resultado |
|---|---|
| `cargo test --workspace` | ✅ 186 pruebas (+2 ignoradas a propósito que leen el historial real y solo imprimen totales) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ sin advertencias |
| `cargo fmt --all -- --check` | ✅ |
| CI de GitHub | ⏳ sin comprobar desde esta PC |

Lo nuevo con pruebas (35):
- `kuntur-agents`: catálogo con Kimi/DeepSeek/Antigravity (IDE, fuera del árbol de procesos), agentes extra (nuevo, que ajusta uno conocido, con ruta, inválidos y repetidos), colores, instalados en el PATH (incluye `.bat` y rutas completas); `launch.rs` (comillas para PowerShell con `&`, cmd y sh); `alerts.rs` (trabajando → esperando confirmado en 2 lecturas, pausa corta, pestaña visible, primera lectura, agente que termina, sesión cerrada, acoplada).
- `kuntur-config`: `alertas` (por defecto `true`) y `[[agentes]]` (`nombre`, `ejecutable`, `argumentos`, `letra`, `color`, `registros`).
- `kuntur-audit` (crate nuevo, 16): lector de Claude Code (`tool_use` Bash/PowerShell, ignora `api-request-blob`), lector de Codex (`shell_command`, `shell` con lista, `exec_command`, `local_shell_call`, `.zst`), repetidos, caché incremental, filtros (texto sin tildes, agente, período en hora local), CSV a prueba de fórmulas, datos sintéticos.
- `kuntur-app`: terminales ocupadas, filas de la acoplada, paleta con "Agentes: abrir …", toasts apagados en humo/configuración, comandos de varias líneas.

### Pruebas de humo (`scripts/smoke.ps1`)

| Comando | RAM | Qué muestra la captura |
|---|---|---|
| `-Sessions 2 -Audit -Badges -Theme dark` | 66 MB | Vista Auditoría con datos sintéticos: 181 comandos en 30 días, columnas Fecha · agente · Comando · Carpeta · Sesión, selector de período, filtro de agente, botón resaltado; insignia "1" en la pestaña no activa |
| `-Sessions 2 -Theme white -Audit -AuditQuery 'git' -Badges` | 66 MB | Búsqueda "git": 47 comandos, filas a todo el ancho |
| `-Sessions 2 -Panel full -Config target\smoke-agentes.toml -Launch 'extra:ping' -Dock open -DockCommands 'ping -t 127.0.0.1'` | 66 MB | Agente extra "Ping de prueba" (insignia P violeta) en instalados; el "clic" escribió `ping -t 127.0.0.1` en la sesión activa; la lista muestra **dos** agentes: el de la principal y el de la acoplada |
| Igual en Blue sin `-Launch` | 64 MB | Fila "PowerShell · acoplada" |

`target\smoke-agentes.toml` es una configuración de prueba (no se sube): define un agente extra con `ping` para probar sin abrir agentes reales.
**Ninguna prueba muestra toasts:** cualquier variable `KUNTUR_SMOKE_*` los apaga.

## Pruebas manuales ⏳ (para el usuario)

- [ ] ⏳ Clic en un agente instalado: con la sesión libre escribe su comando y Enter; si en esa sesión ya corre otro programa (otro agente), abre una sesión nueva y lo lanza ahí. Antigravity abre el IDE en la carpeta (`antigravity .`).
- [ ] ⏳ Paleta: "Agentes: abrir Claude Code" (y los demás instalados); "Agentes: actualizar instalados" relee `config.toml`.
- [ ] ⏳ Agregar un agente en `%APPDATA%\Kuntur\config.toml`, p. ej. `[[agentes]]` `nombre = "Aider"` `ejecutable = "aider"`, y actualizar: aparece en instalados.
- [ ] ⏳ Un agente en la acoplada aparece como "sesión · acoplada".
- [ ] ⏳ Alertas: con `claude` trabajando en una pestaña, cambiar a otra; al quedar esperando (≈ 4–6 s sin salida) la pestaña muestra la insignia y llega una notificación de Windows ("Windows PowerShell" como origen). Al verla se limpia. `alertas = false` apaga la notificación (la insignia sigue).
- [ ] ⏳ Auditoría (Ctrl+Shift+A o el botón): lista los comandos reales de Claude Code y Codex; búsqueda, filtros, Esc vuelve, "Exportar CSV" abre en Excel con tildes y sin fórmulas.

## Notas y decisiones

- **Identidad de agentes:** el catálogo pasó de un enum a ids (`claude`, `codex`… y `extra:<comando>`). `AgentKind` sigue solo para los historiales de uso/auditoría (`AgentDef.usage`).
- **Kimi / DeepSeek** no tienen lector de uso ni de auditoría (formatos no verificados). DeepSeek no publica un CLI oficial: se detecta el comando `deepseek` de terceros. `registros` del agente extra queda reservado.
- **Antigravity es un IDE:** se lanza con `.`, no cuenta como "agente corriendo" (sus procesos no se reconocen en el árbol).
- **No escribir sobre otro programa:** "ocupada" = el shell tiene algún proceso hijo (último muestreo, cada 2 s). Mejor una sesión nueva que mandar `claude⏎` como mensaje a un agente.
- **"Pestaña visible"** = activa, sin la vista de Uso/Auditoría encima y con la ventana de Kuntur enfocada (detrás de un juego, nada es visible). La insignia se limpia al activar la pestaña o al volver a la ventana.
- **Alerta "esperando"** sigue la heurística por actividad (3 s sin salida) más 2 lecturas seguidas para no avisar por pausas cortas; F5b la hace exacta con hooks. "Terminó" = el proceso del agente desaparece (cerrar la sesión no avisa).
- **Toast** con `tauri-winrt-notification` 0.8.1 y el AppUserModelID de PowerShell (sin instalador no hay uno propio registrado; F11 lo registra con el MSI).
- **Fila de la acoplada:** en 240 px no cabe "sesión · acoplada · trabajando": la palabra "acoplada" reemplaza a "trabajando" (el estado sigue en el punto de color); si espera, "acoplada, esperando" en ámbar.
- **Auditoría:** solo comandos de shell (Bash/PowerShell de Claude Code; `shell_command`/`shell`/`exec_command`/`local_shell_call` de Codex). Host = `local` hasta F6. Caché incremental **en memoria** por archivo (tamaño + fecha), no SQLite: reabrir la vista solo relee lo que cambió. El CSV exporta lo que se ve (búsqueda + filtros).
- **GPUI exporta su propio `test`:** en módulos con `use super::*` dentro de `workspace` hay que importar solo lo necesario en `mod tests`.
