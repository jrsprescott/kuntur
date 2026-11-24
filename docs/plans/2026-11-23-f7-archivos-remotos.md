# F7 · Archivos remotos y transferencias — plan

> Rama `fase/f7-archivos` (sale de `fase/f6-ssh`). Especificación: §6 y §7 de
> [`ESPECIFICACION.md`](../ESPECIFICACION.md). Criterio de "listo": el panel "Archivos" de una pestaña
> remota muestra el servidor por SFTP (nunca mezclado con lo local), soltar archivos del Explorador
> los sube, clic derecho descarga, y la cola muestra progreso, cancelar y conflictos.

## Decisiones
| Tema | Decisión |
|---|---|
| Biblioteca SFTP | `russh-sftp` 4.0 (cliente y servidor; no depende de `russh`, va sobre `channel.into_stream()`; sus pruebas usan `russh` 0.65). Un canal `subsystem sftp` sobre **la misma conexión** de la pestaña: no se vuelve a autenticar. |
| Abstracción | Trait `FileSystem` en `kuntur-files`, **síncrono** (listar, stat, leer, escribir, crear carpeta, renombrar, borrar) con rutas `String` y estilo de ruta (Windows / POSIX). Implementaciones: `LocalFs` (también `\\wsl$\…`), `MemoryFs` (pruebas y humo) y `SftpFs` en `kuntur-ssh` (bloquea sobre el runtime de `tokio`; la app lo llama fuera del hilo de la UI). `kuntur-ssh` depende de `kuntur-files`, no al revés. |
| Nunca mezclar | El panel resuelve su fuente por la pestaña activa: local → `LocalFs`; remota → `SftpFs` de esa conexión. Al cambiar de pestaña se reinicia todo el estado del árbol. Encabezado con la ruta completa y la etiqueta **"local"** o **"remoto · usuario@host"**. |
| Seguir la carpeta | Local: OSC 7 / OSC 9;9 como hoy. Remota: OSC 7 que ya entrega `RemoteTerminal::cwd()`; sin OSC 7, la carpeta personal (`realpath .`). |
| Transferencias | `kuntur_files::transfer`: cola pura (`Queue`: estados, conflictos, cancelar, resumen) + un hilo trabajador (`Transfers`) que copia por bloques de 32 KB entre dos `FileSystem` cualesquiera (subir, descargar, copiar local). Carpetas: recursivo. Cancelar borra el archivo a medio copiar. |
| Conflictos | Si el destino existe, el trabajo queda en "¿Sobrescribir / Renombrar / Omitir?" sin bloquear la cola; "Renombrar" usa `nombre (1).ext`. |
| Soltar | `on_drop::<ExternalPaths>` en cada carpeta del árbol (destino = esa carpeta) y en la lista (destino = carpeta mostrada). Remoto = subir por SFTP; local = copiar. |
| Clic derecho | Menú: "Descargar…" (remoto; `prompt_for_paths` para elegir carpeta), "Abrir" (local), "Copiar ruta", "Borrar…" (con confirmación). |
| Carpeta para el agente (§7) | Zona "Soltar para el agente" al pie del árbol remoto: sube a `/tmp/kuntur-<sesión>/` y, al terminar, escribe las rutas (entre comillas) en la terminal y las copia al portapapeles. |
| WSL | `\\wsl$\…` y `\\wsl.localhost\…` son rutas locales: funcionan con `LocalFs`. Traducir el `cwd` Linux de un shell `wsl.exe` a `\\wsl$\<distro>\…` queda pendiente. |
| Panel izquierdo | **Sin cambios**: se mantiene el botón "Archivos" que alterna. La decisión (agentes arriba + árbol abajo) sigue pendiente del usuario. |

## Pasos
1. `kuntur-files` (TDD): rutas POSIX/Windows, nombre de conflicto, carpeta del agente, comillas POSIX, etiqueta local/remoto; trait `FileSystem` + `LocalFs` + `MemoryFs`; cola de transferencias y trabajador (subir, conflicto, renombrar, omitir, cancelar).
2. `kuntur-ssh`: `SftpFs` y `RemoteSession::sftp()`. El servidor `russh` de prueba suma un subsistema SFTP sobre una carpeta temporal; pruebas de listar, subir, descargar, conflicto y cancelar.
3. App: `workspace/files_panel.rs` con fuente local/remota, encabezado con etiqueta, soltar, menú contextual, borrar con confirmación, cola de transferencias (`workspace/transfers_view.rs`) y zona del agente. Ganchos mínimos en `workspace.rs` (humo).
4. Humo: `-Files remote` con un árbol **sintético** (`MemoryFs`) y una cola sintética.
5. Documentación: `f7-resultados.md`, `ESTADO.md`, fila F7.
