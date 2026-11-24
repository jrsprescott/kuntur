# F7 · Archivos remotos y transferencias — Resultados

Rama `fase/f7-archivos` (sale de `fase/f6-ssh`). Plan: [`2026-11-23-f7-archivos-remotos.md`](2026-11-23-f7-archivos-remotos.md). Especificación §6 y §7: árbol de la sesión activa local / SFTP **sin mezclar nunca**, ruta y "local/remoto" siempre visibles, arrastrar desde el Explorador = subir, clic derecho = descargar, progreso, cancelar, sobrescribir/renombrar, carpeta temporal "para el agente".

## Verificación automática ✅

| Verificación | Resultado |
|---|---|
| `cargo test --workspace` | ✅ 214 pruebas (+2 ignoradas a propósito que leen el historial real) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ sin advertencias |
| `cargo fmt --all` | ✅ |
| CI de GitHub | ⏳ sin comprobar desde esta PC |

Lo nuevo con pruebas (18):
- `kuntur-files` (lógica pura y disco/memoria):
  - rutas POSIX y Windows (unir, carpeta superior, nombre; `\\wsl$\…` se trata como local);
  - nombre en conflicto: `informe (1).pdf`, `datos (1).tar.gz`, `.bashrc (1)`, salta los ocupados;
  - carpeta del agente `/tmp/kuntur-<sesión>` (sin acentos, sin `..`, largo acotado); comillas POSIX para escribir las rutas; etiqueta "local" / "remoto · usuario@host";
  - trait `FileSystem` con `LocalFs` (crear, leer, renombrar, borrar recursivo) y `MemoryFs`;
  - cola: estados, resumen de bytes, conflicto que pregunta **sin frenar la cola**, "para todos", cancelar antes y durante;
  - trabajador: sube archivo y carpeta (recursivo), renombrar / sobrescribir / omitir, descarga, **cancelar borra lo copiado a medias**, copiar local en la misma carpeta no se pisa, una carpeta dentro de sí misma da un error claro.
- `kuntur-ssh` **integración SFTP con el servidor `russh` en proceso** (127.0.0.1, puerto efímero; el servidor de prueba suma un subsistema SFTP —`russh-sftp` lado servidor— sobre una carpeta temporal que hace de `/`):
  - sin conexión: error claro ("sin conexión con el servidor");
  - carpeta personal (`realpath .`), listar (carpetas primero, sin `.`/`..`, tamaño y fecha), stat, leer, escribir, crear carpeta, renombrar, borrar recursivo;
  - el SFTP se reutiliza y va por la **misma conexión** (1 sola autenticación);
  - subir 300 KB (varios bloques) a una carpeta, conflicto → "Renombrar" sube `datos (1).bin`, subir a la carpeta del agente (se crea sola), descargar una carpeta remota completa;
  - cancelar una subida a mitad de camino: queda "cancelado" y no queda el archivo a medias en el servidor.

### Pruebas de humo (`scripts/smoke.ps1`, datos **sintéticos**)

| Comando | RAM | Qué muestra la captura |
|---|---|---|
| `-Sessions 1 -Connections remote -Files remote -Theme dark` | 80 MB | Pestaña remota (conexión sintética a un puerto sin servidor) con el panel "Archivos" sobre un árbol **en memoria**: encabezado con "remoto · demo@127.0.0.1:1" y la ruta, botón del agente, zona "Soltar aquí para el agente · /tmp/kuntur-local-apagado/", cola con una subida al 43 % (barra), un conflicto "Ya existe en el destino: Sobrescribir / Renombrar / Omitir" y una descarga "listo" |
| `-Sessions 1 -Connections remote -Files remote -Theme blue` | 69 MB | Igual, dentro de `proyecto` (carpetas primero, tamaños y fechas) |
| `-Sessions 1 -Files local -Commands 'cd C:\Windows\Media' -Theme white` | 64 MB | Pestaña local: etiqueta "local", ruta, sin zona del agente |

Presupuesto: < 100 MB ✅. El SFTP solo se abre al mostrar "Archivos" en una pestaña remota.

## Pruebas manuales ⏳ (para el usuario, con un servidor real)

- [ ] ⏳ Pestaña remota conectada + Ctrl+Shift+E: el árbol muestra la carpeta del shell (con OSC 7, [`docs/osc7-remoto.md`](../osc7-remoto.md)) o la carpeta personal; la etiqueta dice "remoto · usuario@host".
- [ ] ⏳ `cd /var/www` en la terminal → el árbol lo sigue. Cambiar a una pestaña local → el árbol pasa a local (nunca quedan archivos del servidor).
- [ ] ⏳ Arrastrar archivos y una carpeta desde el Explorador sobre una carpeta del árbol → se suben ahí; soltar sobre la lista → a la carpeta mostrada. En una pestaña local, soltar copia.
- [ ] ⏳ Subir otra vez lo mismo → "Ya existe": probar Sobrescribir, Renombrar (`nombre (1).ext`) y Omitir; con varios a la vez, la respuesta vale para todos.
- [ ] ⏳ Un archivo grande (cientos de MB): progreso, velocidad aceptable, × cancela y no deja el archivo a medias.
- [ ] ⏳ Clic derecho en un archivo y en una carpeta remota → "Descargar…" → elegir carpeta → se descarga; "Copiar ruta" copia la ruta remota.
- [ ] ⏳ Clic derecho → "Borrar…" pide confirmación y borra en el servidor (también carpetas).
- [ ] ⏳ Soltar archivos sobre "Soltar aquí para el agente" con `claude` abierto en la pestaña: se suben a `/tmp/kuntur-<sesión>/` y las rutas aparecen escritas en el prompt (y en el portapapeles).
- [ ] ⏳ Cortar la red con el árbol abierto: "Sin conexión con el servidor"; al reconectar vuelve el árbol.
- [ ] ⏳ WSL: una sesión local con `cd \\wsl$\Ubuntu\home\…` en PowerShell muestra el árbol (como local).

## Notas y decisiones

- **Panel izquierdo:** sin cambios. Se mantiene el botón "Archivos" que alterna con los agentes. **La decisión (agentes arriba + árbol abajo a la vez, especificación §3) sigue pendiente del usuario.**
- **`russh-sftp` 4.0** (`cargo search`, 2026-11-23): no depende de `russh` (va sobre `channel.into_stream()`) y sus pruebas usan `russh` 0.65. Canal `subsystem sftp` sobre la conexión de la pestaña; si la conexión se rehace, se abre uno nuevo. Mientras la terminal está preguntando (contraseña, huella) el árbol dice "conectando…": nunca pregunta por su cuenta.
- **Interfaz síncrona:** `FileSystem` bloquea sobre el runtime de SSH; la app lo usa desde el ejecutor de fondo y el hilo de transferencias, nunca desde el hilo de la UI.
- **Cola única** para todas las pestañas (cada fila dice qué hace y a dónde). Un trabajo a la vez, bloques de 64 KB. Lo terminado queda hasta "Limpiar".
- **Copiar local:** soltar un archivo en su misma carpeta pregunta; "Sobrescribir" consigo mismo no hace nada (no se trunca el original).
- **Borrar** no usa la papelera (local) y en el servidor es definitivo: por eso la confirmación lo dice.
- **Para el agente:** `/tmp/kuntur-<nombre de la sesión>`; al terminar se escriben las rutas entre comillas simples con *bracketed paste* (no se envía Enter) y se copian al portapapeles. La carpeta no se limpia sola (queda en `/tmp` del servidor).
- **Enlaces simbólicos** remotos: se resuelven (hasta 200 por carpeta) para poder abrir los que apuntan a carpetas.
- **WSL:** las rutas `\\wsl$\…` / `\\wsl.localhost\…` son locales para Windows y funcionan con el árbol local. **Pendiente:** traducir la carpeta Linux que informa un shell `wsl.exe` (`/home/…`) a `\\wsl$\<distro>\…`.
- Sin hacer (fuera de alcance o para después): crear carpeta y renombrar desde la UI (la API y sus pruebas ya están), arrastrar del árbol hacia el Explorador, abrir archivos remotos (vista previa, F8), reintentar una transferencia fallida, transferencias en paralelo.

## Archivos compartidos tocados (para integrar con otras fases)
`crates/kuntur-app/src/workspace.rs` (+`mod transfers_view;` y la llamada a `smoke_files`), `crates/kuntur-app/src/terminal_view.rs` (+`paste_text`), `crates/kuntur-app/src/main.rs` (ícono `Download` en `ExtraIcons`), `crates/kuntur-ssh/src/{client.rs,remote.rs,lib.rs}`, `crates/kuntur-ssh/tests/servidor.rs`, `crates/kuntur-ssh/Cargo.toml`, `scripts/smoke.ps1` (`-Files`), `Cargo.lock`, `docs/plans/{README.md,ESTADO.md}`. Reescrito: `crates/kuntur-app/src/workspace/files_panel.rs` (`FileEntry.path` pasó a `String`). Nuevos: `workspace/transfers_view.rs`, `kuntur-files/src/{fs,memory,path,transfer}.rs`, `kuntur-ssh/src/sftp.rs`, `kuntur-ssh/tests/sftp_servidor/mod.rs`. No se tocaron `command_palette.rs`, `title_bar.rs` ni `side_panel.rs`.
