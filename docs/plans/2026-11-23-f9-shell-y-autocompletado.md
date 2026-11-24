# F9 · Shell y autocompletado — Plan corto

Rama `fase/f9-autocompletado` (sale de `fase/f4-acoplada`). Criterio de [`PLAN.md`](../PLAN.md): *"`cd e:\pro` + Tab; `z astro`; bloques con código de salida"*. Pedido del usuario: autocompletado **como Warp/fish**: el resto sugerido aparece **en gris** después del cursor y **Tab** (o **→**) lo acepta.

## Decisiones
- **No se reemplaza el editor del shell** (PSReadLine sigue escribiendo la línea): Kuntur solo *mira* la línea de entrada y dibuja encima el texto fantasma. Aceptar = escribir en el PTY lo que falta, como si el usuario lo tecleara. Si no hay sugerencia, Tab llega al shell (su completado de siempre sigue).
- **Integración de shell (OSC 133)** en el mismo script de arranque de PowerShell que ya emite OSC 9;9: `133;D;<código>` (fin del comando anterior), `133;A` (empieza el prompt) y `133;B` (termina el prompt = empieza la entrada). El `133;C` (empieza el comando) lo pone **Kuntur** al enviar Enter estando en el prompt: así no se toca la tecla Enter de PSReadLine.
- Con la marca B (su posición en la cuadrícula, en coordenadas absolutas que no cambian al desplazarse el historial) Kuntur lee **lo escrito** = celdas desde B hasta el cursor. Solo hay sugerencia si: fase "editando" (B recibido y sin Enter), sin pantalla alterna, sin scroll, cursor al final de la línea (nada escrito a su derecha) y entrada de una sola línea lógica. Así no aparece dentro de `claude`/`codex`/`vim` (fase "ejecutando" o modo alterno).
- **Motor puro** en `kuntur-complete` (sin UI): rutas (sin distinguir mayúsculas, carpetas con `\`, comillas simples de PowerShell si hay espacios), historial propio por carpeta y memoria de carpetas por frecuencia × recencia (estilo zoxide; búsqueda difusa con `nucleo-matcher` sobre el nombre de la carpeta para `cd astro`). Orden: historial (preferido de la misma carpeta) → ruta del último argumento → memoria de carpetas para `cd`.
- **Persistencia simple** en texto (`%APPDATA%\Kuntur\historial\comandos.tsv` y `carpetas.tsv`), sin SQLite: se agrega una línea por comando; se compacta al cargar. No se lee el historial de PSReadLine. Comandos que empiezan con espacio no se guardan (convención de bash/zsh).
- Configurable: `autocompletado = true` en `config.toml` (se lee en `main.rs`; no toca `workspace.rs`).

## Tareas
1. `kuntur-term`: escáner de OSC general (`osc.rs`) que entrega carpeta (9;9 / 7) y marcas 133 con su posición en el flujo; `Pane` alimenta el emulador por tramos y guarda la posición del cursor en cada marca (`shell.rs`: fase y marca B). `Emulator`: texto entre dos puntos y cursor absoluto. Prueba ConPTY real con PowerShell 5.1.
2. `kuntur-complete`: `tokens`, `paths`, `history`, `folders`, `suggest` con pruebas (carpetas temporales, datos sintéticos).
3. `kuntur-app`: `terminal_view/ghost.rs` (estado del fantasma, cálculo, aceptar con Tab/→, Esc lo oculta, dibujo en gris). En `terminal_view.rs` solo ganchos de pocas líneas.
4. Humo: `-Type 'cd C:\Win'` (texto sin Enter) y `-History` (historial sintético en memoria). Capturas en el monitor secundario.
5. Bloques (opcional): si no da el tiempo, queda documentado como pendiente.
6. `f9-resultados.md`, `ESTADO.md`, fila F9 de `README.md`.
