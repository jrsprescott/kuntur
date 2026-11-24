# F11 · Pulido y publicación — plan

> Rama `fase/f11-publicacion` (sale de `main`, F1–F9). En paralelo, F10 (MCP) trabaja en otra rama:
> se tocan lo mínimo `workspace.rs`, `command_palette.rs` y `title_bar.rs`.
> Criterio de la fase (`docs/PLAN.md` §6, antigua "F10 · Pulido"): **metas de §1 cumplidas**.

## Decisiones del usuario
- Licencia doble **MIT OR Apache-2.0**, copyright `jrsprescott` 2026.
- El repositorio lo hace público el usuario: aquí solo se deja todo listo (sin `gh repo edit`).
- Instalador MSI con **WiX Toolset v3** (binarios oficiales fuera del repo) + `cargo wix`. No se instala el MSI en esta PC.

## Pasos
1. **Licencia**: `LICENSE-MIT`, `LICENSE-APACHE`, `license = "MIT OR Apache-2.0"` en `[workspace.package]` y `license.workspace = true` en cada crate.
2. **Ícono** `assets/kuntur.ico` (una "K" sobre fondo de acento, 16–256 px) generado por `scripts/generar-icono.ps1`; `build.rs` de `kuntur-app` lo incrusta como recurso 1 (GPUI lo usa como ícono de la ventana).
3. **MSI** (`crates/kuntur-app/wix/main.wxs`): `kuntur.exe` + `kuntur-hook.exe` en `Program Files\Kuntur`, acceso en el menú Inicio con `System.AppUserModel.ID = Kuntur.Kuntur`, registro `Software\Classes\AppUserModelId\Kuntur.Kuntur` (nombre e ícono de los toasts), actualización mayor, desinstalación limpia (no toca `%APPDATA%\Kuntur`). Versión desde `Cargo.toml`. Validar con `msiexec /a` a una carpeta temporal.
4. **AUMID en la app** (`notify.rs`): si el AUMID está registrado (app instalada), los toasts y la ventana usan `Kuntur.Kuntur`; si no, el de PowerShell como hasta ahora. Lógica pura con prueba.
5. **Accesibilidad**:
   - Prueba de contraste WCAG de los 3 temas (`theme_bridge.rs`): texto ≥ 4.5:1, tenue y colores de estado ≥ 3:1 sobre cada fondo real (superficies mezcladas sobre el degradado). Corregir los temas si falla.
   - Teclado: Uso (←/→ período, Ctrl+S CSV), Auditoría (←/→ período, ↑/↓ agente, Ctrl+S CSV), Conexiones (↑/↓ elegir, Enter conectar, F2 editar, Supr borrar con confirmación). Esc cierra en todos.
   - Documentar lo que GPUI no permite (lectores de pantalla: no hay árbol de UI Automation).
6. **Medición final** con `scripts/smoke.ps1 -Measure` (nuevo: tiempo hasta la primera ventana, CPU en reposo, memoria privada): 1 y 5 sesiones, tamaño de binarios. Tabla en `f11-resultados.md`.
7. **Documentación pública**: `README.md` (inglés) + `README.es.md`, capturas sintéticas en `docs/images/`, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`.
8. **Privacidad**: revisar todo el historial (`git log -p --all`, `git grep` en todas las ramas). Corregir solo el estado actual; listar lo que habría que limpiar del historial (decide el usuario).
9. **CI**: compilar, fmt, clippy, pruebas; en etiquetas `v*`, MSI como artefacto.
10. Cierre: `f11-resultados.md`, `ESTADO.md`, fila F11 de `docs/plans/README.md`.

## Pruebas
`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` antes de cada commit. Capturas solo con `smoke.ps1` en el monitor secundario y datos sintéticos.
