# Integración F10–F11 — rama `integracion/f10-f11`

Rama `integracion/f10-f11`, creada desde `origin/main` (`4088f10`, F1–F9). Las dos fases se unieron con **merge** (sin rebase), una por una y con el árbol en verde después de cada una:

| Orden | Rama | Último commit unido | Commit de merge | Pruebas después |
|---|---|---|---|---|
| base | `main` | `4088f10` | — | 349 |
| 1 | `fase/f10-mcp` | `bbaff27` | `965e222` | 388 |
| 2 | `fase/f11-publicacion` | `b575e5d` | `004c7f8` | 394 |
| 3 | corrección de CPU en reposo | — | `041835f` (commit propio) | 400 |

En cada paso: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo test --workspace` sin errores ni advertencias. No se tocaron `main` ni las ramas `fase/*`.

## Conflictos y cómo se resolvieron

Regla: **se conserva todo lo de las dos fases**.

- **F10 sobre `main`:** sin conflictos (F10 salía de `main` y nadie más lo había movido).
- **F11 sobre F10:** git unió solo `workspace.rs`, `main.rs`, `Cargo.toml`, `Cargo.lock`, `crates/kuntur-app/Cargo.toml` y `scripts/smoke.ps1`. Conflictos de texto solo en:
  - `docs/plans/ESTADO.md`: se conservan las secciones «Fase 10» y «Fase 11».
  - `docs/plans/README.md`: fila de F10 con el plan de F10 y fila de F11 con el plan de F11.
- Revisado a mano en los puntos calientes:
  - `workspace.rs`: ganchos de F10 (`mcp`, `start_mcp`, comandos con atajo y Ctrl+Shift+M en `on_key`, panel en `render`) + la línea de F11 (`on_usage_key`).
  - `main.rs`: `mod keynav` y `set_app_identity` (F11) + ícono `Plug` (F10).
  - `command_palette.rs` (`ITEMS: [_; 29]` de F10) y `title_bar.rs` (botón MCP): F11 no los tocaba.
  - `scripts/smoke.ps1`: `-Mcp` (F10) y `-Measure`/`-IdleSec` (F11) juntos; compila `kuntur-app`, `kuntur-hook` y `kuntur-mcp`. **BOM UTF-8 `EF BB BF` conservado** (comprobado en el archivo y en el blob confirmado).
  - `Cargo.toml`/`Cargo.lock`: unidos sin conflicto (`embed-resource` de F11 y el crate `kuntur-mcp` de F10).
- **`kuntur-mcp/Cargo.toml`**: se le agregó `license.workspace = true` (F11 lo puso en los 11 crates que existían; `kuntur-mcp` es el 12.º).

### Atajos de teclado (F10 + F11)
- Uso y Auditoría (←/→, ↑/↓, Ctrl+S) devuelven «no es mío» para todo lo demás: Ctrl+Shift+M y los comandos con atajo de F10 (siempre Ctrl+Shift+tecla o F con modificador) siguen llegando. Ctrl+S y Ctrl+N no son Ctrl+Shift: no chocan con los comandos de F10.
- **Choque resuelto:** Conexiones y el gestor de MCP se quedan cada uno con todo el teclado, y Conexiones no cerraba el gestor al abrirse (podían quedar los dos abiertos). Ahora son **exclusivos**: abrir Conexiones cierra el gestor (el gestor ya cerraba Conexiones), Ctrl+Shift+M dentro de Conexiones abre el gestor y Ctrl+Shift+O dentro del gestor abre Conexiones (antes cada panel se tragaba el atajo del otro).

## Corrección de CPU en reposo

F11 midió 1,4–2,6 % de un núcleo con la ventana quieta. Causa principal: el monitor de cada 2 s (`spawn_monitor` → `apply_sample`) llamaba a `cx.notify()` siempre y GPUI redibujaba la ventana entera.

Cambios (`crates/kuntur-app/src/idle.rs`, lógica pura con 6 pruebas):
- `apply_sample` arma un `Visible` con lo que se dibuja de esas lecturas: filas de agentes con su estado y el tiempo que se muestra («45 s», «3 min»), RAM **redondeada al MB que se muestra**, agentes instalados, límites de Codex, insignias, y lo que cambia sin avisar (carpeta de la terminal con el foco, estado y agentes de cada pestaña remota). `VisibleTracker::changed` solo pide redibujar si cambió.
- Con la ventana **sin foco** el monitor lee cada **5 s** (con foco, 2 s). Las alertas siguen llegando (insignia y notificación), solo más espaciadas; los eventos exactos de los agentes (pipe / OSC 7501) no dependen de este intervalo. La lectura lenta (instalados, límites de Codex) pasó de «cada 15 vueltas» a «cada 30 s» por reloj, para que no se estire a 75 s sin foco.
- Panel «Archivos» (local o remoto): se relee la carpeta en cada vuelta del monitor; ahora solo redibuja si cambió la carpeta, el listado o el error.
- Revisados sin cambios: la vigilancia de capturas de MCP (cada 2 s) solo actúa si aparece un archivo nuevo (no llama a `notify`); el gestor de MCP no tiene temporizador propio; el sondeo de transferencias (150 ms) solo corre mientras hay transferencias activas.

Medición (`scripts\smoke.ps1 -Measure -IdleSec 30` (20 en las corridas sueltas), monitor secundario, sin foco, `-Commands 'cls'`; «5 sesiones» = `-Sessions 5 -Panel full -AgentEvents scripts\smoke-agentes.jsonl -MaxRamMB 120`):

**Ruido:** esta PC tenía hoy otra carga (navegador, otras sesiones) y la CPU en reposo varió mucho de una corrida a otra (de 1,4 % a 9,4 % con el mismo binario). Por eso se comparó **A/B con el mismo binario**, alternando una corrida «antes» y una «después» con un interruptor temporal por variable de entorno que restauraba el comportamiento viejo (redibujar siempre, 2 s fijos). El interruptor ya se quitó.

| Corrida (30 s en reposo) | Antes | Después |
|---|---|---|
| 1 sesión · 1 | 2,76 % | 1,98 % |
| 1 sesión · 2 | 4,69 % | 4,11 % |
| 1 sesión · 3 | 9,42 % | 3,49 % |
| 5 sesiones + agentes · 1 | 5,52 % | 5,05 % |
| 5 sesiones + agentes · 2 | 5,36 % | 1,35 % |
| 5 sesiones + agentes · 3 | 1,87 % | 1,51 % |
| **Mediana 1 sesión** | **4,69 %** | **3,49 %** |
| **Mediana 5 sesiones** | **5,36 %** | **1,51 %** |

En los 6 pares, «después» gastó menos. Primeras corridas sueltas (20 s, antes del A/B): antes 2,19–2,26 % (1 sesión) y 3,83 % (5 sesiones); después 1,8–6,0 % (mismo ruido).

**Lo que sí es exacto:** con un registro temporal de cada `render` (ya quitado) se contaron los redibujos. Antes: la ventana entera se redibujaba cada 2 s aunque nada cambiara. **Después: cero redibujos en reposo** (solo los de los primeros ~10 s mientras arrancan los shells).

**Por qué no llega a 0 %:** midiendo la CPU por hilo con la ventana quieta y sin redibujar nada (20 s), quedan ~0,5 s de CPU: el hilo de la UI de GPUI (~1,4 % de un núcleo, su bucle de mensajes y de cuadros de Windows, fuera de este código) y la lectura de procesos (`sysinfo`, ahora cada 5 s sin foco). Bajarlo más pide cambios en GPUI o dejar de leer procesos sin foco (perdería alertas por proceso) — queda como pendiente.

RAM (sin cambios): 63–66 MB con 1 sesión, 66 MB con 5 sesiones + agentes; memoria privada 81 / 90–92 MB.

## Capturas revisadas (pruebas de humo)

Todas con `scripts\smoke.ps1` en el monitor secundario, sin tomar el foco (comprobado por el script), datos sintéticos, revisadas a ojo:

| Comando | RAM | Qué se ve |
|---|---|---|
| `-Sessions 2 -Mcp probar` | 75 MB | Gestor «MCP · PowerShell»: perfil `godot` activo (sonnet · high · aceptar ediciones) y `web`; `escena-godot` **conectado** (kuntur-mcp-falso 0.1.0, MCP 2025-06-18); `base-datos` error + «Falta DB_PASSWORD»; `navegador` «no se pudo lanzar»; `docs` (http) «no se pudo conectar» + «Falta el token»; comandos con atajo; detrás, la vista previa con `captura-falsa.svg`; botón MCP resaltado |
| `-Sessions 1 -Usage 30 -Theme blue` | 72 MB | Tema **Blue** corregido de F11 (degradado más oscuro, texto y tenues legibles); Uso 30 días con tarjetas, barras, mapa de calor, agentes y modelos |
| `-Sessions 1 -Connections list` | 65 MB | Panel Conexiones con la **fila elegida resaltada** y sus acciones (F11) y el pie con los atajos (↑↓ elegir · F2 editar · Supr borrar) |
| `-Sessions 3` | 64 MB | Tres pestañas, colores ANSI, panel compacto con agentes instalados, línea de estado «Kuntur · 64 MB» |

Capturas en `target\integ2-*.png` (no se suben: muestran la carpeta personal de esta PC).

## MSI

`.\scripts\msi.ps1 -Check` desde esta rama → `target\wix\kuntur-0.1.0-x86_64.msi`, **12,9 MB** (13 500 416 bytes). **No se instaló** (solo `msiexec /a` a una carpeta temporal, que el script borra).

| Contenido | |
|---|---|
| `PFiles\Kuntur` | `kuntur.exe` (34 605 568 bytes, con F10), `kuntur-hook.exe` (291 840), `kuntur.ico`, `LICENSE-MIT.txt`, `LICENSE-APACHE.txt` |
| Acceso directo | «Kuntur» → `[APPLICATIONFOLDER]kuntur.exe`, `System.AppUserModel.ID = Kuntur.Kuntur` |
| Registro | HKLM `Software\Classes\AppUserModelId\Kuntur.Kuntur` (`DisplayName`, `IconUri`); HKCU `Software\Kuntur\Installer` (KeyPath del acceso) |
| Producto | Kuntur 0.1.0, UpgradeCode `{56DDC03F-821A-4FEC-A00A-9EE8EE53A2A4}` |

`kuntur-mcp-falso.exe` (binario de pruebas de F10) **no** va en el MSI.

## Pendientes ⏳

- ⏳ **Cierre inesperado durante `-Measure`:** en una corrida «antes» con 5 sesiones + eventos sintéticos, Kuntur se cerró solo durante la espera en reposo (F11 ya lo había visto una vez). No dejó evento en el registro de Windows; en release la app no tiene consola, así que un `panic` se pierde. No se repitió en las otras 16 corridas de hoy. Sugerencia: escribir los `panic` a un archivo en `%LOCALAPPDATA%\Kuntur` para poder diagnosticarlo.
- ⏳ CPU en reposo: el resto (~1–2 % de un núcleo) es de GPUI y de la lectura de procesos; ver arriba.
- ⏳ Pruebas manuales de F10 (MCP y agentes reales) y de F11 (instalar el MSI en una PC o VM de prueba, notificación con origen «Kuntur», actualizar y desinstalar), listadas en `f10-resultados.md` y `f11-resultados.md`.
- ⏳ Ver el CI en GitHub con esta rama (`integracion/**` está en los disparadores).
- Decisión del usuario: avanzar `main` a esta rama; publicar el repositorio (sigue **privado**) y la release.
