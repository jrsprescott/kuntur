# Estado del proyecto — nota de traspaso

> Última actualización: 2026-11-23. Léela primero al retomar en otra PC (o pídele a Claude Code que la lea).

## Dónde está cada cosa
- Rama principal `main`: **Fases 1 y 2** (terminal real + sesiones, temas, paleta, persistencia). Estable.
- Rama de trabajo **`fase/f3-v5`**: Fase 3 (diseño v5) **en curso**. Aquí se continúa.
- Planes: `docs/plans/README.md` (hoja de ruta) y `docs/plans/2026-11-23-f3-v5-agentes-y-uso.md` (plan de la fase actual).
- Diseño de referencia: `docs/PLAN.md` ("Cambios de la v4") y la simulación `docs/simulacion/index.html`.

## Fase 3 (v5): qué está hecho y qué falta
| Parte | Estado |
|---|---|
| Bloque A: lectores de uso (Claude Code, Codex zstd, OpenCode SQLite), estadísticas por período, catálogo/detección de agentes, carpeta actual por OSC 9;9 | ✅ hecho y revisado (90 pruebas) |
| Task 7: sesiones como **pestañas arriba** | ⏳ en curso al cerrar el día (ver commits de la rama) |
| Task 8: **panel izquierdo de agentes** (buscar, Todos/Esperando/Trabajando, instalados con anillo %, "Archivos") | ⏳ en curso al cerrar el día |
| Task 10 (parte): **RAM** abajo a la derecha | ⏳ en curso al cerrar el día |
| Task 9: vista **Uso de agentes** (7/30/90/todo, tokens por día, mapa de horas, modelos, agentes, tareas top) | ⏸ **en pausa**, siguiente paso |
| Task 10 (resto): humo con datos sintéticos de uso, `f3-resultados.md`, CI | ⏸ en pausa |

Decisión pendiente ya tomada: la tarjeta "Tokens de entrada" **incluye la caché** (lectura + escritura) y muestra debajo "NN % desde caché".

## Preparar la PC (una sola vez)
1. **Git** y **GitHub CLI** (`winget install GitHub.cli`), y `gh auth login` con la cuenta personal.
2. **Rust**: `winget install Rustlang.Rustup`, luego en una consola nueva `rustup default stable`.
3. **Visual Studio Build Tools** con "Desarrollo de escritorio con C++" (compilador MSVC + Windows SDK): `winget install Microsoft.VisualStudio.BuildTools` y marcar esa carga de trabajo en el instalador.
4. Fuente **Cascadia Mono** (viene con Windows 11 / Windows Terminal).

## Clonar y compilar
```powershell
git clone https://github.com/jrsprescott/kuntur.git
cd kuntur
git switch fase/f3-v5
cargo test --workspace            # ~90 pruebas
cargo build -p kuntur-app --release
.\target\release\kuntur.exe       # o doble clic
.\scripts\smoke.ps1               # prueba de humo: captura target\smoke.png y mide RAM
```
La primera compilación tarda varios minutos (GPUI + SQLite).

## Reglas del proyecto (no romperlas)
- **Privacidad:** nada de rutas personales, nombres de instituciones ni datos reales de historiales en el repo. Las pruebas usan datos sintéticos. La prueba `crates/kuntur-usage/tests/datos_reales.rs` está `#[ignore]` y solo imprime totales.
- **Antes de cada commit:** `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- **Pruebas visuales:** solo con `scripts/smoke.ps1` (entrada por `KUNTUR_SMOKE_INPUT`, captura con PrintWindow). **Nunca** simular teclado del sistema (SendKeys): las teclas pueden caer en otra ventana.
- **Diseño:** minimalista (una barra arriba, jerarquía por texto, color solo para estado), atajos con Ctrl+Shift.
- Si en la PC el `PATH` del usuario contiene la entrada `%PATH%` (se infla y Windows lo descarta), Kuntur no encontrará `claude`/`npm`: quitar esa entrada.

## Sincronizar entre PCs
- Al terminar en una PC: `git add -A`, `git commit`, `git push`.
- Al llegar a la otra: `git status` (que esté limpio) y `git pull` en la rama `fase/f3-v5`.
- El ejecutable compilado de cada avance se publica como *pre-release* privada en GitHub (Releases), para probar sin compilar.
