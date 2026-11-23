# F1 · Resultados de aceptación (2026-11-23)

Leyenda: ✅ verificado · ⏳ pendiente de prueba manual · ❌ falla (se convierte en tarea nueva del plan)

## Verificación automática
Con `scripts/smoke.ps1` (entrada desde dentro de la app con `KUNTUR_SMOKE_INPUT`, captura solo de la ventana de Kuntur con PrintWindow, sin simular teclado).

| # | Prueba | Esperado | Resultado |
|---|---|---|---|
| A1 | PowerShell 7 + `Write-Host` con colores + `Get-ChildItem` | Colores, tabla alineada, cursor | ✅ 73 MB |
| A2 | `claude` | Se dibuja (pantalla de confianza de carpeta, colores, negrita, selector) | ✅ 57 MB (tras arreglar el PATH, Task 12) |
| A3 | `codex` | Se dibuja (cuadros, enlaces, caja de entrada) | ✅ 68 MB |
| A4 | `opencode` | Interfaz a pantalla completa (logo, fondos, diseño centrado) | ✅ 58 MB |
| A5 | `qwen` | Logo truecolor con degradado, bordes, menús | ✅ 62 MB |
| A6 | Al cerrar Kuntur | No quedan procesos de los agentes abiertos | ✅ |
| A7 | Pruebas unitarias e integración ConPTY (`cargo test --workspace`) | Todas pasan | ✅ 26 |
| A8 | `clippy -D warnings` y `cargo fmt --check` | Limpios | ✅ |

## Pendiente de prueba manual (necesita teclado y mouse reales)
Abrir con `cargo run -p kuntur-app --release` y marcar cada fila.

| # | Prueba | Esperado | Resultado |
|---|---|---|---|
| 1 | Escribir dentro de `claude` y salir con `/exit` | Responde al teclado | ⏳ |
| 2 | `git log` (paginador) | Flechas y `q` funcionan | ⏳ |
| 3 | Seleccionar con el mouse mientras corre `ping -t 127.0.0.1` | La selección no se pierde | ⏳ |
| 4 | Ctrl+C con selección / sin selección | Copia / interrumpe el ping | ⏳ |
| 5 | Ctrl+Shift+C, Ctrl+V, Shift+Insert, clic derecho | Copian y pegan | ⏳ |
| 6 | Pegar 3 líneas dentro de `claude` | Llegan como un solo bloque (no se envían solas) | ⏳ |
| 7 | Doble clic / triple clic / Alt+arrastrar | Palabra / línea / bloque | ⏳ |
| 8 | Rueda del mouse y Shift+RePág | Desplaza el historial; al escribir vuelve abajo | ⏳ |
| 9 | Redimensionar la ventana | `$Host.UI.RawUI.WindowSize` cambia y el texto se reacomoda | ⏳ |
| 10 | `ñ á é ¿ ¡ @ # \` con teclado latinoamericano | Se escriben bien | ⏳ |
| 11 | RAM en el Administrador de tareas con `claude` abierto | < 80 MB (sin contar procesos hijos) | ⏳ |

## Hallazgos de la aceptación
- **Task 12 (corregido):** `portable-pty` duplicaba el PATH (registro del sistema + del usuario) y con PATH grandes Windows no podía iniciar programas desde el shell. Se usa el entorno del proceso tal cual; prueba de regresión incluida.
- **Detalle cosmético para F2:** si el alto de la ventana no es múltiplo del alto de línea, queda una franja de unos píxeles con el fondo del tema en lugar del fondo del programa (visible con OpenCode, que usa su propio fondo).
