# F9 · Shell y autocompletado — Resultados

Rama `fase/f9-autocompletado` (sale de `fase/f4-acoplada`). Plan: [`2026-11-23-f9-shell-y-autocompletado.md`](2026-11-23-f9-shell-y-autocompletado.md). Criterio de [`PLAN.md`](../PLAN.md): *"`cd e:\pro` + Tab; `z astro`; bloques con código de salida"*. Pedido del usuario: autocompletado **como Warp** (el resto sugerido en gris; Tab o → lo acepta).

## Qué se hizo

| Parte | Estado |
|---|---|
| **OSC 133** en el script de arranque de PowerShell (`pane.rs`): `D;código` (fin del comando anterior), `A` (prompt) y `B` (entrada), además del OSC 9;9 de siempre. Lo que el usuario ve no cambia | ✅ probado con ConPTY real (PowerShell 5.1) |
| El escáner de OSC (`cwd.rs`) entrega cada marca con su posición en el flujo; el PTY alimenta el emulador por tramos y guarda dónde quedó el cursor en cada marca (`shell.rs`, coordenadas absolutas que no cambian al desplazarse la pantalla) | ✅ |
| `Pane::prompt_input`: lo escrito desde la marca B hasta el cursor, solo en el prompt (no dentro de `claude`/`codex`/`vim`, ni con pantalla alterna, ni con el historial desplazado, ni en entradas de varias líneas) | ✅ |
| El **Enter** enviado desde Kuntur pone la marca `C` (no se toca la tecla Enter de PSReadLine) y guarda el comando en el historial propio | ✅ |
| **Motor** `kuntur-complete` (puro): rutas (sin distinguir mayúsculas, carpetas con `\`, comillas simples si hay espacios), historial por carpeta, memoria de carpetas por frecuencia × recencia (estilo zoxide; `nucleo-matcher` para `cd astro`) | ✅ 29 pruebas |
| **Texto fantasma** en `TerminalView` (`terminal_view/ghost.rs`): gris después del cursor; **Tab** o **→** escriben en el shell solo lo que falta; **Esc** lo oculta hasta que cambie lo escrito; sin sugerencia, Tab llega a PowerShell como siempre | ✅ prueba de punta a punta con PowerShell real + capturas |
| **Bloques** (`terminal_view/blocks.rs`): raya fina en el margen izquierdo por cada comando enviado desde el prompt (comando + salida): tenue si terminó bien, roja si terminó con error | ✅ capturas |
| `autocompletado = true` en `config.toml` (`false` lo apaga) | ✅ |
| Persistencia: `%APPDATA%\Kuntur\historial\comandos.tsv` y `carpetas.tsv` (texto; se compacta al cargar). No se lee el historial de PSReadLine. Los comandos que empiezan con **espacio** no se guardan | ✅ |

### Orden de las sugerencias
1. **Historial**: el comando más reciente que empieza igual (primero los de la misma carpeta). Para `cd` solo los de la misma carpeta (una ruta relativa de otra carpeta no sirve).
2. **Ruta** del último argumento (o de la primera palabra si parece ruta: `.\scr`, `C:\x`, `~\`). Para `cd`/`sl`/`Set-Location`/`pushd` solo carpetas, y primero las más visitadas.
3. **Memoria de carpetas** para `cd parte`: si la ruta empieza con lo escrito se agrega el resto; si no, en gris se ve `→ C:\ruta\completa` y aceptar reemplaza la palabra.

## Verificación automática ✅

| Verificación | Resultado |
|---|---|
| `cargo test --workspace` | ✅ 188 pruebas (+2 ignoradas a propósito que leen el historial real de uso) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ |
| `cargo fmt --all -- --check` | ✅ |

Lo nuevo con pruebas (≈40): marcas OSC 133 y su posición (también partidas entre lecturas), ciclo de fases y bloques, lectura de lo escrito (cortes por el ancho, desplazamiento, pantalla alterna), **ConPTY real**: PowerShell 5.1 marca el prompt, lo escrito se lee sin el prompt, código de salida 0 y 3, bloque visible; motor: palabras de PowerShell, rutas (mayúsculas, absolutas/relativas, `/`, `~`, comillas, solo carpetas para `cd`, ranking por visitas), historial (carpeta, repetidos, límite, espacio inicial), memoria de carpetas (frecuencia × recencia, difusa, carpetas borradas), archivos (ida y vuelta, rotos, compactación); **de punta a punta** en `ghost.rs`: `cd proy` → gris `ecto-demo\`, Esc lo oculta, Tab lo escribe en PowerShell, Enter lo guarda en el historial.

### Pruebas de humo (`scripts/smoke.ps1`)

Nuevos parámetros: `-Type 'texto'` (se escribe en la primera sesión después de los comandos y **sin Enter**) y `-History 'cmd1','cmd2'` (historial **sintético** en memoria; en la prueba de humo nunca se lee ni escribe el historial real).

| Comando | RAM | Qué muestra la captura |
|---|---|---|
| `-Sessions 1 -Commands 'cls' -Type 'cd C:\Win'` (Dark) | 71 MB | `cd C:\Win` y en gris `dows\` |
| `-Sessions 1 -Theme white -History 'git status --short','git log --oneline -5' -Type 'git l' …` | 63 MB | `git l` y en gris `og --oneline -5` (del historial); raya roja junto a `cmd /c exit 3` |
| `-Sessions 1 -WaitSec 9 -Commands 'Start-Sleep 1','Write-Host bloque-ok','cmd /c exit 3','Get-ChildItem C:\Windows\Media …' -Type 'cd C:\Program'` (Dark) | 62 MB | `cd C:\Program` y en gris ` Files\` (al aceptar queda `cd 'C:\Program Files\'`); bloques tenues para los comandos que terminaron bien y rojo para `exit 3` |

Presupuesto de RAM (< 90 MB) ✅. Nota: en `-Commands` el separador interno es `\n` literal, así que una ruta como `C:\no…` se parte (artefacto de la prueba de humo, no de la app).

## Pruebas manuales ⏳ (para el usuario)

Abrir `target\release\kuntur.exe` normal (PowerShell por defecto).

- [ ] ⏳ Escribir `cd C:\Win` → aparece `dows\` en gris; **Tab** lo completa (`cd C:\Windows\`); seguir con `Fon` → `ts\`; **→** también acepta.
- [ ] ⏳ Escribir `cd c:\prog` (minúsculas) → sugiere `ram Files\`; al aceptar queda `cd 'c:\Program Files\'` (con comillas porque hay espacio).
- [ ] ⏳ Ejecutar un par de comandos (`git status`, `dir`), luego escribir `git s` → aparece el resto del historial en gris; Tab lo acepta.
- [ ] ⏳ Visitar varias veces una carpeta (`cd D:\algo\mi-proyecto`), ir a otra y escribir `cd proy` → en gris `→ D:\algo\mi-proyecto`; Tab reemplaza la palabra por la ruta.
- [ ] ⏳ Con sugerencia visible, **Esc** la oculta (y no borra la línea); sin sugerencia, Esc y **Tab** hacen lo de siempre en PowerShell (Tab cicla el completado de PowerShell).
- [ ] ⏳ Mover el cursor a la izquierda (←, Inicio) oculta la sugerencia; volver al final la muestra.
- [ ] ⏳ Dentro de `claude`, `codex`, `vim`, `less` o mientras corre un comando **no** aparece nada.
- [ ] ⏳ Los comandos que terminan con error muestran una raya roja fina a la izquierda; los que terminan bien, una raya tenue. `cls` las borra.
- [ ] ⏳ `autocompletado = false` en `%APPDATA%\Kuntur\config.toml` lo apaga (al reabrir Kuntur).
- [ ] ⏳ Con el perfil propio de PowerShell (prompt personalizado, oh-my-posh, etc.) el prompt se ve igual que antes.
- [ ] ⏳ Un comando que empieza con espacio (` secreto`) no aparece después como sugerencia.

## Notas y decisiones

- **No hay editor de entrada propio** (el plan original lo pedía): PSReadLine sigue editando la línea y Kuntur solo dibuja encima y, al aceptar, escribe lo que falta como si se tecleara. Es mucho menos invasivo (se conservan atajos, colores y completado de PowerShell) y da el comportamiento de Warp/fish pedido. Un editor propio queda para más adelante si hace falta.
- La marca `C` la pone Kuntur al enviar Enter: así no hace falta cambiar la tecla Enter de PSReadLine en el script (sería tocar la configuración del usuario). Consecuencia: los comandos que no se envían con el Enter de Kuntur (por ejemplo, pegados con un salto de línea) no forman bloque.
- El código de salida es `0` si `$?` es verdadero; si no, `$LASTEXITCODE` (o `1` para errores de cmdlets). Un `$LASTEXITCODE` viejo puede aparecer tras un error de cmdlet: el color (rojo) es correcto igual.
- **PowerShell 7 con predicciones de PSReadLine** (gris propio): el fantasma de Kuntur no aparece porque hay texto a la derecha del cursor; no se pisan.
- **Shells sin OSC 133** (cmd, bash por defecto, SSH aún no): no hay sugerencias ni bloques; nada cambia.
- **Mayúsculas**: aceptar agrega el resto tal como está en el disco pero no corrige lo ya escrito (`c:\win` → `c:\windows\`); PowerShell no distingue mayúsculas.
- Los bloques se guardan por línea absoluta; si el historial de la terminal llega a su máximo (10 000 líneas) las rayas viejas pueden quedar corridas. `cls` los borra.
- Pendiente: completar comandos/parámetros (`git sw…`, `-Recur…`) sin historial previo; completado remoto por SSH (F6); `z` como comando propio; plegar/copiar bloques.
