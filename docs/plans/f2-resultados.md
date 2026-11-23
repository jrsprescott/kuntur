# F2 · Sesiones y temas — Resultados

Rama `fase/f2-sesiones`. Tasks 1–9 del plan [`2026-11-23-f2-sesiones-y-temas.md`](2026-11-23-f2-sesiones-y-temas.md).

## Verificación automática ✅

| Verificación | Resultado |
|---|---|
| `cargo test --workspace` | ✅ 57 pruebas (kuntur-app 16 · kuntur-config 8 · kuntur-term 33) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ sin advertencias |
| `cargo fmt --all -- --check` | ✅ |
| `cargo build -p kuntur-app --release` | ✅ sin advertencias |

Pruebas nuevas de la F2 (Tasks 8–9):
- `command_palette`: filtro sin mayúsculas ni tildes (`"fuente"` → "Fuente: más grande", `"tema blu"` → "Tema: Blue", `"iconos"` → "Riel: solo íconos"), navegación ↑/↓ que da la vuelta, renombrar en el mismo campo (el nombre propuesto se reemplaza al escribir; Backspace lo vacía de una vez; tope de 40 caracteres; solo la primera línea al pegar).
- `mouse`: movimiento sin botón (modo 1003) → `ESC[<35;x;yM` en SGR y su equivalente X10.

### Pruebas de humo (`scripts/smoke.ps1`, configuración aislada en `target\smoke-config.toml`)

| Comando | RAM | Qué muestra la captura |
|---|---|---|
| `-Sessions 3 -Theme blue` | 60 MB | Degradado azul, riel de íconos con 3 sesiones, colores ANSI legibles |
| `-Sessions 3 -Theme dark` | 61 MB | Tema oscuro, mismo diseño |
| `-Sessions 3 -Theme white` | 60 MB | Tema claro, controles de ventana visibles |
| `-Sessions 2 -Theme dark -Palette` | 65 MB | Paleta centrada arriba con las 13 acciones, atajos a la derecha y ✓ en "Tema: Dark" y "Riel: solo íconos" |
| `-Sessions 2 -Theme white -PaletteQuery tema` | 61 MB | Filtro "tema" → 3 resultados, ✓ en "Tema: White" |
| `-Sessions 3 -Theme blue -PaletteQuery fuente` | 61 MB | Filtro "fuente" → 3 resultados con Ctrl+= / Ctrl+- / Ctrl+0 |
| `-Sessions 2 -PaletteRename` (Dark y White) | 61 MB | Campo con el nombre actual resaltado y la guía "Enter guarda · Esc cancela" |
| Persistencia: `smoke-config.toml` con tema `white`, riel completo, fuente 15 y sesiones "web" (shell por defecto) y "servidor" (`cmd.exe`), sin `-Sessions` | 60 MB | Se restauran tema blanco, riel completo, letra 15 y las 2 sesiones con su nombre; "servidor" muestra `cmd` |
| `-Commands opencode` con esa configuración | 60 MB | OpenCode abre dentro de la sesión "web"; la app no falla |
| Configuración con una sesión cuyo shell no existe | 60 MB | La sesión "roto" abre con el shell predeterminado en vez de cerrar la app |

Con `-Sessions` o `-Theme` la app no guarda nada (se comprobó que `smoke-config.toml` no se crea).

## Pruebas manuales ⏳ (para el usuario)

Abrir `target\release\kuntur.exe` normal (usa `%APPDATA%\Kuntur\config.toml`).

**Sesiones y atajos**
- [ ] ⏳ Ctrl+Shift+T crea una sesión; Ctrl+Shift+W cierra la activa; cerrar la última cierra la ventana.
- [ ] ⏳ Ctrl+Tab / Ctrl+Shift+Tab rotan; Ctrl+1…9 van directo; clic en el riel cambia de sesión y el teclado sigue en la terminal.
- [ ] ⏳ Ctrl+Shift+B alterna riel completo ↔ íconos (nunca se oculta).
- [ ] ⏳ En modo íconos, al dejar el mouse sobre una sesión aparece la tarjeta con nombre, programa y `Ctrl+N`.
- [ ] ⏳ En modo completo, la `×` aparece al pasar el mouse por una fila inactiva y cierra esa sesión.
- [ ] ⏳ Un clic en la barra de título o en el espacio vacío del riel no le quita el teclado a la terminal.

**Paleta de comandos**
- [ ] ⏳ Ctrl+Shift+P y el botón ⌘ de la barra abren la paleta; Esc, un clic fuera o Ctrl+Shift+P la cierran.
- [ ] ⏳ Escribir filtra sin importar mayúsculas ni tildes ("sesion", "tema blu", "iconos"); ↑/↓ (o Tab) mueven; Enter ejecuta; clic en una fila también.
- [ ] ⏳ Mientras la paleta está abierta, nada de lo escrito llega al shell.
- [ ] ⏳ "Renombrar sesión": el campo muestra el nombre actual resaltado; al escribir se reemplaza; Enter guarda (se ve en el riel y en la barra); Esc cancela.
- [ ] ⏳ Temas Blue / Dark / White desde la paleta cambian todo en vivo.
- [ ] ⏳ Ctrl+= (o Ctrl++), Ctrl+- y Ctrl+0 cambian el tamaño de letra en todas las sesiones.

**Persistencia**
- [ ] ⏳ Cambiar tema, riel, letra, renombrar y crear/cerrar sesiones; cerrar Kuntur con la ✕ y volver a abrir: todo se restaura (sesiones con su nombre y shell, en el mismo orden).

**Mouse dentro de programas**
- [ ] ⏳ En `opencode`: clic en elementos, rueda para desplazar mensajes y arrastrar funcionan dentro del programa.
- [ ] ⏳ En `opencode`: Shift + arrastrar selecciona texto de Kuntur; Ctrl+Shift+C lo copia; Shift + clic derecho copia/pega como siempre.
- [ ] ⏳ En PowerShell (sin modo mouse): arrastrar selecciona, doble/triple clic seleccionan palabra/línea, la rueda recorre el historial, clic derecho copia/pega — igual que en F1.

## Notas

- La paleta dibuja su propio campo (sin componente de entrada de gpui-kit): sin mover el cursor con ←/→; Backspace borra, Ctrl+Backspace limpia, Ctrl+V pega la primera línea.
- Rueda con un programa en modo mouse: hasta 10 avisos por evento; con Shift se desplaza el historial de Kuntur.
- Si una sesión guardada apunta a un shell que ya no existe, se abre el shell predeterminado (sin aviso visible por ahora).
