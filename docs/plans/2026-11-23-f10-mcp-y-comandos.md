# F10 · MCP y comandos — plan corto

Rama `fase/f10-mcp` (sale de `main` con F1–F9). Especificación §11, `docs/PLAN.md` (hoja Ctrl+K:
MCP por pestaña, perfiles, modelo/esfuerzo/permisos, comandos con atajo).

## Decisiones
- **Datos en `kuntur-config`** (como las conexiones SSH): `[[mcp]]` (catálogo), `[[mcp_perfiles]]`
  (conjuntos con nombre + modelo/esfuerzo/permisos), `[[comandos]]` (comandos con atajo) y, en cada
  `[[sessions]]`, `mcp = [...]` y `perfil = "..."`. Secretos (variables de entorno marcadas en
  `secretos`, token *bearer* de los http): **solo** en el Administrador de credenciales de Windows
  (servicio «Kuntur MCP», cuenta `<servidor>/<VARIABLE>`), nunca en `config.toml`.
- **Lógica en el crate nuevo `kuntur-mcp`** (sin UI): validación del catálogo, perfiles, generación de la
  configuración de cada CLI, línea de lanzamiento, prueba `initialize` (stdio y http), carpeta
  temporal por sesión, vigilancia de la carpeta de salida y comandos con atajo. Binario de prueba
  `kuntur-mcp-falso` (servidor MCP falso para las pruebas de integración y la prueba de humo).
- **Por CLI** (formatos verificados en su documentación oficial, ver `f10-resultados.md`):
  - Claude Code: `--mcp-config <sesión>\claude.json` (`{"mcpServers": …}`) y `--strict-mcp-config`
    solo si se pide (`mcp_exclusivo = true`); `--model`, `--effort`, `--permission-mode`.
  - Codex: sobrescrituras `-c mcp_servers.<nombre>.<clave>=<TOML>` (no se toca `CODEX_HOME`);
    los secretos se pasan por `env_vars` con un `codex-entorno.ps1/.cmd` de la sesión;
    `--model`, `-c model_reasoning_effort=…`, `--sandbox` / `--ask-for-approval`.
  - OpenCode: `OPENCODE_CONFIG=<sesión>\opencode.json` solo para ese comando; `--model`; permisos
    en el mismo archivo (`permission`).
  - Los archivos van a `%LOCALAPPDATA%\Kuntur\mcp\<pid>-<sesión>\` y se borran al cerrar la sesión
    (y los huérfanos al arrancar). **Nunca** se toca `~/.claude.json`, `~/.codex/config.toml`, etc.
- **UI**: módulo nuevo `workspace/mcp_view.rs` (botón de ícono, Ctrl+Shift+M —libre—, acción de la
  paleta). En los archivos compartidos solo ganchos.

## Tareas (TDD en lo puro)
1. `kuntur-config/src/mcp.rs`: tipos + pruebas de ida y vuelta.
2. `kuntur-mcp`: `catalog` (validar, cuentas de secretos), `generate` (Claude/Codex/OpenCode), `launch`
   (línea final por shell), `probe` (initialize stdio/http con tiempo límite), `session_dir`,
   `outputs` (archivos nuevos en `salida\`), `shortcuts` (atajos de comandos).
3. Integración: servidor falso stdio (binario) y http (en la prueba).
4. App: estado MCP por sesión, gestor (lista con interruptores, perfiles, Probar, secreto faltante),
   ganchos en `launch_agent`/worktree, cerrar sesión, `persist`, `on_key`, paleta, barra de título.
5. Capturas de los MCP → vista previa (sondeo de `salida\` en el ciclo del monitor).
6. `smoke.ps1 -Mcp` con servidores sintéticos (el servidor falso), capturas y resultados.
