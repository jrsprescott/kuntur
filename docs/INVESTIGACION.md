# Investigación — referencias (2026-11-23)

## Medición inicial en este equipo
- Warp: ~903 MB de RAM (2 procesos, 164 s de CPU acumulados).
- Windows Terminal (OpenConsole): 36 MB.
- Prueba `spikes/hola-gpui` (GPUI + gpui-kit 0.11, release): binario de 18.4 MB, **70 MB** de RAM, 74 MB privada.
- Equipo: Windows 11 Pro 26200, RTX 4060, 64 GB RAM. Rust 1.100.0, MSVC 14.51 (Build Tools 2026), Windows SDK 10.0.26100.

## Librerías elegidas (versiones en crates.io)
| Crate | Versión | Uso |
|---|---|---|
| gpui-kit | 0.11.0 | GPUI + componentes (dock, pestañas, temas, tablas, editor con tree-sitter) |
| alacritty_terminal | 0.26.0 | Parser VT + grilla + scrollback |
| portable-pty | 0.9.0 | ConPTY en Windows / PTY en Unix |
| sysinfo | 0.40.4 | Árbol de procesos (detectar `claude.exe`, `codex`) |
| notify | 9.0.0 | Vigilar archivos (rollouts de Codex, config) |
| nucleo | 0.5.0 | Búsqueda difusa para el autocompletado |
| rusqlite | 0.43.0 | Base de carpetas visitadas e historial |
| toml | 1.2.2 | Configuración y temas |
| russh | 0.65.4 | Cliente SSH en Rust: una conexión, varios canales (sesión + terminal acoplada) |
| arboard | 3.7.0 | Portapapeles del sistema (copiar/pegar, OSC 52) |
| russh-sftp | 4.0.0 | SFTP sobre la conexión de `russh` (panel Archivos, subir/bajar) |
| clipboard-win | 5.4.3 | Portapapeles de archivos de Windows (CF_HDROP): copiar/pegar con el Explorador |
| windows | 0.62.2 | OLE `DoDragDrop` para arrastrar desde Kuntur hacia el Explorador |

## Terminales y multiplexores revisados
- **Zellij 0.46** (11/11/2026): `zellij terminal` (terminal propia, también en Windows), bandeja de atención con OSC 7501, vistas previas de sesiones. Windows nativo desde la 0.44. — https://github.com/zellij-org/zellij/releases · https://alternativeto.net/news/2026/11/zellij-0-46-brings-built-in-terminal-attention-inbox-for-ai-agents-and-session-previews/ · https://zellij.dev/news/remote-sessions-windows-cli/
- **WezTerm**: Rust, GPU, multiplexor, `portable-pty`. — https://github.com/wez/wezterm
- **Alacritty**: núcleo `alacritty_terminal`. — https://github.com/alacritty/alacritty
- **Rio**: Rust + WebGPU; en Windows todavía menos estable. Variante con foco en Windows: tacterm. — https://github.com/raphamorim/rio · https://github.com/narma/tacterm
- **tty7**: GPUI de Zed + núcleo de Alacritty (proyecto inicial, sirve de plantilla). — https://github.com/yanyaoer/tty7
- **termie**: multiplexor GPU solo para Windows (wgpu + winit, inicial).
- **par-term**: wgpu, DirectX 12 en Windows. — https://docs.rs/crate/par-term/0.39.0
- **Zed**: GPUI + alacritty_terminal; `crates/terminal_view` es GPL-3. — https://zed.dev
- **gpui-kit / gpui-component** (Longbridge): Apache-2.0. — https://github.com/longbridge/gpui-kit · https://gpui-kit.com

## Gestores de agentes revisados
- claude-squad (Go, tmux + worktrees) — https://github.com/smtg-ai/claude-squad
- agent-deck (estados trabajando/esperando/listo) — https://github.com/asheshgoplani/agent-deck
- agentgc (Rust, worktrees aislados + supervisor) — https://github.com/JohnWayneeee/agentgc
- ccmanager — https://github.com/kbwo/ccmanager
- Listas: https://github.com/andyrewlee/awesome-agent-orchestrators · https://github.com/bradAGI/awesome-cli-coding-agents

## Claude Code (docs oficiales, code.claude.com)
- **statusLine** (stdin JSON): `model{id,display_name}`, `session_id`, `transcript_path`, `cwd`, `workspace{current_dir,project_dir,git_worktree}`, `cost{total_cost_usd,total_duration_ms,total_api_duration_ms,total_lines_added,total_lines_removed}`, `context_window{used_percentage,context_window_size,current_usage{…}}`, `effort{level}`, `thinking{enabled}`, `rate_limits{five_hour{used_percentage,resets_at}, seven_day{…}}` (solo Pro/Max), `worktree{name,path,branch}`. — https://code.claude.com/docs/en/statusline
- **Hooks**: 33 eventos; para el estado: `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Notification` (`permission_prompt`, `idle_prompt`, `elicitation_dialog`, `agent_needs_input`), `PermissionRequest`, `Stop`, `SessionEnd`. — https://code.claude.com/docs/en/hooks
- **MCP por sesión**: `--mcp-config` + `--strict-mcp-config`; `/mcp` en caliente; `--resume` no recuerda `--mcp-config`. — https://code.claude.com/docs/en/mcp
- **OpenTelemetry**: `claude_code.api_request` (tokens, costo), `claude_code.token.usage`. — https://code.claude.com/docs/en/monitoring-usage
- **Terminal**: OSC 7501 (estado de programa, ≥ 2.1.295), `preferredNotifChannel`, `CLAUDE_CODE_DISABLE_TERMINAL_TITLE`, `--session-id`, `-n`. — https://code.claude.com/docs/en/terminal-config
- Por confirmar: formato de bytes de OSC 7501, esquema exacto de `Notification`, nombres de campos de uso dentro de los JSONL.

## Codex CLI
- Rollouts: `~/.codex/sessions/YYYY/MM/DD/rollout-<fecha>-<uuid>.jsonl`; eventos `token_count` con `info` (uso) y `rate_limits`. — https://lin-guanguo.github.io/llm-memory-research/agent-cli/codex-session-file-schema/ · https://codex.danielvaughan.com/2026/03/30/codex-cli-session-analytics-jsonl-rollout
- Apagar un MCP en una ejecución: `-c mcp_servers.<nombre>.enabled=false`. — https://developers.openai.com/codex/config-advanced
- Referencia de lectores en Rust: crate `abtop` (`src/collector/codex.rs`).

## Hugging Face
No hay terminales ahí; solo sería útil más adelante para un modelo local de autocompletado. No hace falta para las fases F1–F7.
