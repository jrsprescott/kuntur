# F8 · Spike del visor web (`wry` / WebView2)

> 2026-11-23 · Proyecto: [`spikes/wry-gpui`](../../spikes/wry-gpui) (fuera del workspace) · Medición: `spikes/wry-gpui/medir.ps1`
> (abre la ventana en la pantalla secundaria y sin foco, como `scripts/smoke.ps1`; captura con PrintWindow y suma la RAM del proceso y de sus `msedgewebview2.exe`).

## Pregunta
¿Se puede incrustar un WebView2 como **ventana hija** de la ventana GPUI (`gpui-kit` 0.11) usando `HasWindowHandle`, para ver PDF y HTML en la vista previa? ¿Se posiciona bien, sigue al redimensionar, qué pasa con los menús de Kuntur que caen encima, y cuánta RAM cuesta?

## Montaje
- `wry` 0.57.3 (WebView2 del sistema, ya incluido en Windows 11), `gpui-kit` 0.11.
- `WebViewBuilder::new().with_url(file:///…).with_bounds(rect).build_as_child(window)`: `gpui::Window` implementa `HasWindowHandle` (raw-window-handle 0.6, la misma versión que usa `wry`). Compila sin adaptadores.
- El rectángulo del visor sale de un `canvas` de GPUI (en su `prepaint`), en píxeles lógicos de la ventana, y se copia con `WebView::set_bounds` solo cuando cambia.
- Un "menú" GPUI (`deferred`) que se superpone al área del visor, para ver el z-order.

## Resultados

| Prueba | Resultado |
|---|---|
| Crear el WebView2 hijo | ✅ 0,2–0,4 s (el primero ~0,4 s). No roba el foco de la ventana (la ventana de prueba se abrió sin foco y siguió así) |
| Posición | ✅ exacta: el visor ocupa el rectángulo del `canvas` (816 × 736 px dentro de una ventana de 1200 × 760 con 360 px de panel y 12 px de margen) |
| Redimensionar la ventana | ✅ el visor sigue el nuevo tamaño en el mismo cuadro (516 × 536 tras `window.resize(900 × 560)`) |
| HTML local | ✅ se ve y ejecuta su JavaScript |
| PDF local | ✅ visor de PDF de Edge completo (barra con zoom, páginas, buscar, imprimir) |
| Z-order con menús | ❌ como se esperaba: un panel GPUI **no puede dibujarse encima** del WebView2 (el menú queda cortado en el borde del visor). ✅ Solución probada: **ocultar el visor** (`set_visible(false)`) mientras haya un menú, la paleta o un panel flotante abierto; se ve el menú completo |
| Cerrar | ✅ soltar el `WebView` destruye la ventana hija y **todos** los procesos de WebView2 (0 procesos a los pocos segundos) |
| Captura con PrintWindow | ✅ el contenido del WebView2 sale en la captura (útil para las pruebas de humo) |

### RAM (WebView2 vive en sus propios procesos)

| Contenido | Procesos `msedgewebview2.exe` | Memoria de trabajo | Memoria privada | Proceso GPUI |
|---|---|---|---|---|
| HTML de ejemplo | 6 | ~302 MB | **~141 MB** | 63 MB |
| PDF de una página | 8 | ~485 MB | **~256 MB** | 63 MB |
| Después de cerrar el visor | 0 | 0 | 0 | 65 MB |

La "memoria de trabajo" suma páginas compartidas entre los procesos de Edge (DLL); la **privada** es la que de verdad cuesta. HTML cae dentro de lo previsto en la especificación (80–150 MB); el PDF lo supera (~256 MB) porque carga el visor de PDF de Edge. Por eso el visor se **crea al abrir** una pestaña PDF/HTML y se **destruye al cerrarla**: sin vista previa web, Kuntur no paga nada.

## Decisión
**Funciona bien → se integra** para PDF y HTML (y para Office convertido a PDF con LibreOffice, si está instalado), con estas reglas:
1. Un solo WebView2 por ventana, creado al mostrar la primera pestaña PDF/HTML y destruido al cerrar la última (o al cerrar la vista previa).
2. Se oculta mientras la pestaña activa no es web, y mientras estén abiertos la paleta de comandos, el menú contextual de enlaces o la vista "Uso de agentes".
3. Se crea con `with_focused(false)` para no quitarle el teclado a la terminal, y un clic en la zona GPUI devuelve el teclado a la ventana (`focus_parent`).
4. Carpeta de datos propia: `%LOCALAPPDATA%\Kuntur\WebView2` (si no, WebView2 crea `kuntur.exe.WebView2` junto al ejecutable).
5. Su RAM se mide aparte (principio 5 de la hoja de ruta): no suma a la de Kuntur.

## Riesgos y pendientes
- **Teclado:** cuando el WebView2 tiene el foco, los atajos de Kuntur (Ctrl+Shift+…) no llegan hasta hacer clic fuera. Comprobar a mano (no se puede simular teclado en las pruebas de humo).
- **Escalado de pantalla:** medido con escala 100 %. `wry` convierte posiciones lógicas con el DPI de la ventana padre; comprobar a mano en una pantalla al 125–150 %.
- **Arrastrar el separador** sobre el WebView2: el mouse entra en la ventana hija y GPUI deja de recibir el movimiento si se pasa por encima; el separador queda del lado de la terminal, así que en la práctica solo afecta al arrastrar muy rápido.
- PDF de terceros con JavaScript: el visor de Edge no ejecuta JS de PDF peligroso, pero el HTML local **sí** ejecuta su JavaScript (como abrirlo en el navegador).
