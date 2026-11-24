# F6 · SSH y conexiones — Resultados

Rama `fase/f6-ssh` (sale de `fase/f4-acoplada`). Plan: [`2026-11-23-f6-ssh-y-conexiones.md`](2026-11-23-f6-ssh-y-conexiones.md). Criterio de [`PLAN.md`](../PLAN.md) §6: *"Abrir la terminal acoplada en `/var/www/...` sin volver a autenticar; `sudo apt upgrade` funciona"*.

## Verificación automática ✅

| Verificación | Resultado |
|---|---|
| `cargo test --workspace` | ✅ 196 pruebas (+2 ignoradas a propósito que leen el historial real) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ sin advertencias |
| `cargo fmt --all` | ✅ |
| CI de GitHub | ⏳ sin comprobar desde esta PC |

Lo nuevo con pruebas (~45):
- `kuntur-term`: un `Transport` falso alimenta el emulador (texto, OSC 7 remoto, respuestas del emulador por el transporte, redimensionar, cerrar). Las pruebas de ConPTY siguen pasando.
- `kuntur-config`: validación de conexiones, `usuario@host:puerto` (también IPv6), puerto del formulario, TOML mínimo, sesiones remotas guardadas (y que nunca aparezca una contraseña en el archivo).
- `kuntur-ssh` (lógica pura): editor de línea de los diálogos (eco, modo secreto, cancelar, ignora flechas y respuestas del emulador), plan de autenticación, known_hosts (nueva / conocida / cambiada, lee el de OpenSSH sin escribirlo, entradas con hash), comando de la acoplada y de agentes, consola.
- `kuntur-ssh` **integración con un servidor `russh` en proceso** (127.0.0.1, puerto efímero, llaves generadas en la prueba, home temporal, secretos en memoria):
  - huella nueva → se muestra `SHA256:…` y se confirma; contraseña sin eco; shell con eco; OSC 7 remoto; redimensionar (`window-change`); la contraseña queda recordada; **segundo canal en la carpeta sin volver a autenticar** (1 sola autenticación); agentes del servidor por `exec`; `exit` cierra cada terminal por separado;
  - llave cifrada: pide la passphrase, avisa si es incorrecta, la recuerda y la segunda sesión no pregunta;
  - huella cambiada: aviso en rojo, no se autentica, la pestaña queda abierta;
  - rechazar la huella nueva: no conecta ni guarda nada;
  - conexión caída: aviso, el historial sigue, **Enter reconecta en la última carpeta** sin preguntar;
  - servidor apagado: error claro ("el servidor rechazó la conexión") y reintentar no se cuelga.
- `kuntur-app`: formulario (Tab, solo dígitos en el puerto, pegar `usuario@host:puerto`, errores en palabras, editar sin chocar consigo misma), `~` en la llave, agentes con y sin insignia, conexiones sintéticas válidas, acciones de la paleta.

### Pruebas de humo (`scripts/smoke.ps1`, conexiones **sintéticas**)

| Comando | RAM | Qué muestra la captura |
|---|---|---|
| `-Sessions 1 -Connections list` (Dark) | 75 MB | Panel "Conexiones" bajo su botón (arriba a la derecha, resaltado): nombre, `usuario@host:puerto`, llave o "agente o ~/.ssh", "recuerda la contraseña"; "+ Nueva"; pie con la regla de las contraseñas |
| `-Sessions 1 -Connections remote -Theme blue` | 64 MB | Pestaña remota con ícono de globo; "Conectando a demo@127.0.0.1:1…", error en rojo y la franja **"● Desconectado · … · Reconectar"**; el panel izquierdo no muestra los agentes locales en la pestaña remota |
| `-Sessions 1 -Connections form -Theme white` | 63 MB | "Editar conexión": campos con foco y guía tenue, puerto angosto, casilla "Recordar contraseña o passphrase" marcada, Cancelar / Guardar |

Presupuesto: < 100 MB ✅. El runtime de `tokio` (2 hilos) solo se crea al abrir la primera sesión remota.

## Pruebas manuales ⏳ (para el usuario, con un servidor real)

- [ ] ⏳ Agregar una conexión real en el gestor (botón de servidor o Ctrl+Shift+O → "+ Nueva"), clic → pestaña remota conectada.
- [ ] ⏳ Primera vez: la huella SHA256 coincide con `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` del servidor; después de confirmar, no vuelve a preguntar. Un servidor ya usado con `ssh` (en `~/.ssh/known_hosts`) no pregunta.
- [ ] ⏳ **ssh-agent de Windows** (servicio "OpenSSH Authentication Agent" con la llave cargada con `ssh-add`): conecta sin preguntar nada.
- [ ] ⏳ Llave con passphrase sin agente: la pide en la terminal; con "Recordar" marcado aparece en el Administrador de credenciales ("Kuntur SSH") y la próxima vez no pregunta.
- [ ] ⏳ Contraseña (servidor con `PasswordAuthentication` o `keyboard-interactive`).
- [ ] ⏳ `sudo apt upgrade` (o `sudo -v`) funciona: la contraseña de sudo se escribe en la terminal como siempre.
- [ ] ⏳ `cd /var/www/...` y Ctrl+` → la acoplada abre **en esa carpeta y sin volver a autenticar** (requiere OSC 7 en el servidor: [`docs/osc7-remoto.md`](../osc7-remoto.md)).
- [ ] ⏳ `htop`, `vim`, `claude` remotos: colores, mouse y redimensionar la ventana.
- [ ] ⏳ Cortar la red (Wi-Fi) → en ≤ 45 s aparece "Conexión perdida" y la franja; Enter o Ctrl+Shift+R reconecta en la última carpeta.
- [ ] ⏳ Cerrar Kuntur con una pestaña remota abierta y volver a abrir: se reconecta sola.
- [ ] ⏳ El panel izquierdo de una pestaña remota muestra "Agentes en el servidor".
- [ ] ⏳ Borrar una conexión con "Recordar" borra su contraseña del Administrador de credenciales.

## Notas y decisiones

- **Diálogos dentro de la terminal** (como `ssh`): huella, passphrase y contraseña se piden en la propia pestaña, sin modales; así la reconexión conserva el historial. La passphrase/contraseña no tiene eco y Ctrl+C / Esc cancelan.
- **known_hosts:** Kuntur lee `~/.ssh/known_hosts` (solo lectura) y escribe solo en `%APPDATA%\Kuntur\known_hosts`. Una huella distinta en cualquiera de los dos bloquea la conexión e indica archivo y línea.
- **Secretos:** `keyring-core` + `windows-native-keyring-store` (sin la función de búsqueda, que trae `regex`). Servicio "Kuntur SSH"; cuentas `contraseña:usuario@host:puerto` y `passphrase:<ruta de la llave>`. Solo se guardan con "Recordar"; una contraseña recordada que falla se borra.
- **`russh` con `ring`** (sin `aws-lc-rs`): compila en Windows sin CMake/NASM.
- **Caída vs. salida normal:** si el canal termina con `exit-status` (o `exit-signal`), la pestaña se cierra como una local; si no y la conexión también se cerró, es una caída. Keepalive cada 15 s (3 sin respuesta = caída).
- **Acoplada remota:** `exec` con PTY `cd -- '<carpeta>' 2>/dev/null || cd; exec "$SHELL" -l` sobre la misma conexión. Asume un shell POSIX en el servidor (con un servidor OpenSSH de Windows no aplica).
- **Agentes del servidor:** `"$SHELL" -lc 'for a in claude codex …; do command -v …'` con una marca por línea (el resto de la salida se ignora). Solo ve el `PATH` de `~/.profile` (documentado).
- **Panel "Archivos"** en una pestaña remota queda vacío a propósito (nunca mezcla local con remoto); el árbol por SFTP es F7.
- Atajos nuevos: **Ctrl+Shift+O** (conexiones), **Ctrl+Shift+R** (reconectar); en el formulario, Ctrl+R alterna "recordar" y Ctrl+N (en la lista) abre uno nuevo.
- Sin hacer: certificados de host OpenSSH (se rechazan con un aviso), `ProxyJump`, leer `~/.ssh/config`, reenvío de agente, OSC 7501 de agentes remotos (va con F5), una entrada "Conectar: <nombre>" por conexión en la paleta.

## Archivos compartidos tocados (para integrar con otras fases)
`crates/kuntur-term/src/{pane.rs,lib.rs}` (+`transport.rs`), `crates/kuntur-config/src/{settings.rs,lib.rs}` (+`connections.rs`), `crates/kuntur-app/src/{workspace.rs,terminal_view.rs,command_palette.rs,main.rs}`, `crates/kuntur-app/src/workspace/{title_bar.rs,dock_view.rs,side_panel.rs}`, `crates/kuntur-app/Cargo.toml`, `scripts/smoke.ps1`, `Cargo.lock`, `docs/plans/{README.md,ESTADO.md}`.
