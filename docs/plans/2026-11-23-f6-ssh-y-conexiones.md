# F6 · SSH y conexiones — plan

> Rama `fase/f6-ssh` (sale de `fase/f4-acoplada`). Especificación: §5, §6, §8 y §15 de
> [`ESPECIFICACION.md`](../ESPECIFICACION.md); criterio de "listo" de `PLAN.md` (§6, fila F6):
> *abrir la terminal acoplada en `/var/www/...` sin volver a autenticar; `sudo apt upgrade` funciona*.
> F7 (árbol SFTP y transferencias) queda fuera.

## Decisiones
| Tema | Decisión |
|---|---|
| Biblioteca SSH | `russh` 0.65 con `ring` (sin `aws-lc-rs`: evita CMake/NASM en Windows). Corre en un runtime `tokio` propio de 2 hilos, creado la primera vez que se abre una sesión remota (las sesiones locales no lo pagan). |
| Transporte en `kuntur-term` | Trait `Transport` (escribir, redimensionar, PID, ¿terminó?, cerrar) + un lector `Read`. `Pane::spawn` (ConPTY) sigue igual por fuera: por dentro usa `LocalPty`. `Pane::with_transport` alimenta el mismo emulador desde cualquier transporte. `TerminalView` no cambia: una pestaña remota es la misma vista. |
| Diálogos de conexión | **Dentro de la propia terminal**, como `ssh`: "Conectando…", huella nueva (s/n), passphrase y contraseña sin eco, errores en rojo. Así el historial se conserva al reconectar y no hace falta UI modal. Además, una franja "Desconectado · Reconectar" sobre la terminal y Ctrl+Shift+R. |
| Autenticación | Orden: ssh-agent de Windows (`\\.\pipe\openssh-ssh-agent`) → llave del gestor (o `~/.ssh/id_ed25519`, `id_ecdsa`, `id_rsa` si no hay) → contraseña. La passphrase se pide en la terminal. |
| Secretos | Administrador de credenciales de Windows (`keyring-core` + `windows-native-keyring-store`), **solo** si la conexión tiene "Recordar" marcado. Nunca en `config.toml`. Si una contraseña guardada falla, se borra y se vuelve a pedir. |
| known_hosts | Se lee `~/.ssh/known_hosts` (solo lectura, compatible con OpenSSH, incluye entradas con hash) y `%APPDATA%\Kuntur\known_hosts`; Kuntur **solo escribe en el suyo**. Huella nueva → se pide confirmación mostrando `SHA256:…`; huella distinta → se bloquea con un aviso claro (y la línea del archivo). |
| Acoplada remota | Segundo canal (`exec` con PTY) sobre la **misma conexión**: `cd -- '<carpeta>' && exec "$SHELL" -l`. Sin volver a autenticar. |
| Reconexión | Canal cerrado con `exit-status` = la sesión terminó (la pestaña se cierra como una local). Sin `exit-status` = conexión perdida: aviso en la terminal, franja y Enter / Ctrl+Shift+R reconectan en la última carpeta conocida (OSC 7). |
| Agentes en el servidor | Al conectar, un canal `exec` con `sh -lc 'for a in claude codex …; do command -v …'`; se muestran en el gestor y en el panel de agentes de la pestaña remota. |
| Persistencia | `[[connections]]` en `config.toml` (nombre, host, puerto, usuario, llave, recordar). Cada `[[sessions]]` suma `connection` (vacío = local) y `cwd`. Al restaurar, las remotas se reconectan solas (si piden algo, lo piden en su terminal). |

## Pasos
1. `kuntur-term`: `Transport`, `LocalPty`, `Pane::with_transport` (las pruebas de ConPTY siguen pasando).
2. `kuntur-config`: `ConnectionConfig` (validación, `usuario@host:puerto`), `connections` y `SessionConfig.connection/cwd`. TDD.
3. `kuntur-ssh` lógica pura con TDD: editor de línea para los diálogos, plan de autenticación, known_hosts, comando de la acoplada, parseo de agentes, nombres de credenciales.
4. `kuntur-ssh` red: cliente `russh`, `RemoteSession` (conexión compartida) y `RemoteTerminal` (transporte). Pruebas de integración con un **servidor `russh` en proceso** (localhost, puerto efímero, llaves generadas en la prueba): contraseña, llave, huella nueva/cambiada, shell con eco, redimensionar, segundo canal sin reautenticar, `exec` de agentes, caída y reconexión.
5. App: gestor de conexiones (`workspace/connections_view.rs` + lógica en `connections.rs`), botón de ícono, paleta, pestañas remotas, acoplada remota, franja de reconexión, restaurar sesiones. Ganchos mínimos en `workspace.rs`.
6. Humo: `-Connections list|form|remote` con conexiones **sintéticas** (dominios `.test`, `127.0.0.1`).
7. Documentación: `docs/osc7-remoto.md` (bash, zsh, PowerShell), `f6-resultados.md`, `ESTADO.md`, fila F6.
