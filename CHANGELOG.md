# Changelog

All notable changes to Kuntur. The format follows [Keep a Changelog](https://keepachangelog.com/) and the project
uses [Semantic Versioning](https://semver.org/). Development happened in phases (F1…F11); details of each phase, in
Spanish, are in [`docs/plans/`](docs/plans/).

## [Unreleased]

## [0.2.4] — Pestañas reordenables y la ñ de cualquier teclado

### Added
- **Tabs can be reordered.** Drag a tab and drop it on another to take its place (the target is highlighted);
  Ctrl+Shift+PgUp / Ctrl+Shift+PgDn move the active tab; the right-click menu adds "Move left" / "Move right". The
  order is saved and the active tab stays active when it moves.

### Fixed
- **Characters typed through the system text input reach the terminal**, whatever the user's Windows language or
  keyboard setup: the ñ of an AutoHotkey or PowerToys shortcut (injected as `VK_PACKET`), dead keys (US-International
  `~` + `n` → ñ), AltGr, Alt+numpad codes and IMEs (Chinese, Japanese, Korean…). The terminal now registers a text
  input handler and writes the text the system composes; key presses are kept for special keys and shortcuts, and a
  keystroke interceptor prevents a character from being written twice. While the palette, Connections, MCP or
  Settings are open, that text does not reach the terminal behind them.

## [0.2.3] — Uso y Auditoría más livianos, reanudar chats y renombrar pestañas

### Added
- **Usage: open a chat from "Tasks that used the most tokens".** Clicking a task opens a new tab in its project
  folder that resumes that chat with its agent (`claude --resume`, `codex resume`, `opencode --session`); the tab is
  named after the chat. Only plain session ids are accepted, so nothing else reaches the shell.
- **Audit: the chat each command was used in.** A new "Chat" column (and CSV column) shows the chat title: the one
  the user set or, if none, the one the agent generated (Claude Code `custom-title` / `ai-title`, Codex
  `session_index.jsonl`). The search also matches chat titles.
- **Tabs: right-click menu** with "Rename…" (the palette field, prefilled with the name the tab shows — the chat title
  when an agent is running; Enter saves, Esc cancels) and "Close tab". Double-click still renames.

### Changed
- **Usage and Audit load at most the last 30 days.** The period selectors are Today / 7 days / 30 days ("1 year" and
  "All" were removed); weekly and monthly chart buckets went with them.
- **Lower memory and faster reads** (measured on a real history of 827 chats, 431 MB):
  - Audit reads each log line by line with a reused buffer and typed JSON that skips text, tool output and written
    files without copying them: peak memory while reading went from 49.7 MB to 5.6 MB.
  - Audit keeps commands once (`Arc`), shared between the cache and the list, with session, chat title and folder
    shared per file: retained memory went from 6.2 MB to 2.8 MB. Files not modified in the last 30 days are not
    opened.
  - Usage asks the store only for the last 30 days and drops the per-response dedup keys: 2.3 MB → 1.3 MB.
  - Closing Usage or Audit (in any way: Esc, opening a tab, an agent, the dock…) releases their data; opening them
    reads again through the incremental store/cache, which only re-reads changed files.

## [0.2.2] — SSH más limpio y seguro

### Fixed
- **SSH: mouse movements showing up as text** (`^[[<35;37;51M…`) after reconnecting. A full-screen program on the
  server (htop, vim, claude…) could leave mouse tracking, the alternate screen or a hidden cursor on when the
  connection dropped; the reconnected shell then received every mouse move as text. Kuntur now resets the terminal
  modes when an SSH connection drops and before it (re)connects (nothing is sent to the server).
- **Local tabs:** when the shell shows its prompt again (OSC 133), mouse tracking left on by a program that exited
  badly is turned off.

### Security
- SSH passwords and key passphrases are wiped from memory right after use (`zeroize`), including what was typed but
  not sent. They were already never written to disk (only to the Windows Credential Manager with "Remember").

## [0.2.1] — íconos e IA local

### Added
- **New app icon:** an Andean condor in neon blue and violet (app, taskbar, Start menu and desktop shortcut), with
  thicker strokes at 16–32 px so it reads in the taskbar.
- **Kuntur icon and name in the title bar** over the full side panel; clicking it opens the project on GitHub.
- **Brand icons** for the agents (Claude, OpenAI/Codex, OpenCode, Qwen, Gemini, Kimi, DeepSeek) and local providers
  (LM Studio, Ollama), from [Lobe Icons](https://github.com/lobehub/lobe-icons) (MIT; logos are trademarks of their
  owners). Custom agents keep their colored letter.
- **Local AI:** Kuntur detects LM Studio, Ollama, Jan and llama.cpp (only `localhost`), lists their models (size,
  loaded or not) in a "Local AI" section and offers **OpenCode with this model** (a per-session OpenCode config with
  the local OpenAI-compatible provider, together with the session MCP servers; the global config is never touched),
  **Quick chat** and **Start server**.
- Code signing and privacy policy (`CODE_SIGNING_POLICY.md`) for the SignPath Foundation application.

### Changed
- **Agent usage:** agents that are no longer installed keep their history and show "not installed"; models served
  locally (LM Studio, Ollama…) count as "Local" with cost US$ 0 instead of "no price".

## [0.2.0] — ajustes v0.2

### Added
- **Settings panel** (gear button, Ctrl+, or the palette): themes as color circles with their name below, font size,
  side panel, **language (automatic / Spanish / English)**, notifications and updates. Changes are saved at once.
- **Notifications that do not nag:** per type (waiting for you, asks for permission, finished), at most one per
  session every N minutes (5 by default) and only when Kuntur is in the background; the rest only adds to the badge.
- **File tree like VS Code's Explorer** (chevrons, indent guides, lazy loading, keyboard) and its **right-click
  menu** in Spanish and English: new file/folder, rename (F2), delete to the Recycle Bin, copy path / relative path,
  cut/copy/paste (local and remote through the transfer queue), reveal in File Explorer, open in integrated
  terminal, add to chat / Claude Code: add to chat, share (GitHub link), images preview, add as Python project.
- **Update notice:** checks GitHub when Kuntur opens and every 24 h (can be turned off); "Update" downloads the MSI,
  verifies its SHA-256 and Authenticode signature, installs it and reopens Kuntur (`crates/kuntur-update`).
- **Desktop shortcut** from the installer.
- Code signing ready: `scripts\msi.ps1 -Sign` (own certificate) and an optional SignPath step in CI
  (see `docs/FIRMA.md`).
- Linux and macOS groundwork: data folders per OS (XDG / Library), `$SHELL` as the default shell, and CI jobs that
  build there (Windows is still the only released platform).
- **AI title in the session tab:** while an agent runs, the tab shows the title it sets (in Claude Code, the topic of
  the conversation) instead of "PowerShell"; when it does not fit, the active tab scrolls it back and forth and stops
  on hover. A name given by hand always wins. Default tab names follow the shell (PowerShell, cmd, bash, zsh…).
- **Warp-style image paste:** Ctrl+V with a screenshot in the clipboard attaches it instead of pasting nothing.
  Claude Code gets its own image-paste key and shows `[Image #N]`; other agents and the plain shell get the saved
  PNG path; SSH tabs upload it by SFTP first. Dragging an image onto the terminal works the same. An agent bar shows
  the thumbnails (click to preview).
- Title bar tooltips in Spanish and English.

### Changed
- Release binaries no longer contain paths of the build machine (`--remap-path-prefix`), and `msi.ps1` refuses to
  build an installer that does.
- The theme button opens Settings at the themes.

## [0.1.0] — 2026-11-24 (preview)

First preview release (phases F1–F11).

### Added (F10 · MCP and commands)
- New `kuntur-mcp` crate: MCP server catalog (`[[mcp]]`, stdio or http) and profiles (`[[mcp_perfiles]]`) in
  `config.toml`; secrets only by name, values in the Windows Credential Manager ("Kuntur MCP").
- MCP manager per tab (plug icon button, Ctrl+Shift+M or the palette): enable/disable servers, apply profiles
  with model/effort/permissions, enter missing secrets and "Test" (launch/connect + `initialize`, 15 s max).
- When an agent is launched, the MCP config for that session is generated for its CLI and never touches the
  global one: Claude Code (`--mcp-config`), Codex (`-c mcp_servers.…`) and OpenCode (`OPENCODE_CONFIG`); a per-session
  temporary folder is removed when the tab closes.
- Screenshots written by an MCP (`${KUNTUR_SALIDA}` / `KUNTUR_MCP_SALIDA`) open by themselves in the preview.
- Custom commands with a shortcut (`[[comandos]]`, per session or global, Ctrl+Shift+key or a modified F key).

### Added (F11 · polish and release)
- MSI installer (WiX v3, `cargo wix`): `kuntur.exe` and `kuntur-hook.exe` in `Program Files\Kuntur`, Start menu
  shortcut with its own AppUserModelID (`Kuntur.Kuntur`), so Windows notifications say "Kuntur".
- App icon and version info embedded in `kuntur.exe`.
- Keyboard navigation in Usage (← → period, Ctrl+S CSV), Audit (← → period, ↑ ↓ agent, Ctrl+S CSV) and
  Connections (↑ ↓, Enter, F2, Del, Ctrl+N).
- WCAG contrast test for the three themes; `smoke.ps1 -Measure` (startup time, idle CPU, private memory).
- Dual license MIT OR Apache-2.0, English README, CONTRIBUTING, SECURITY and this changelog.

### Changed
- Blue theme: darker gradient end and lighter muted/accent/state colors to meet WCAG AA contrast.
- Lower idle CPU: the 2 s agent/RAM monitor only redraws the window when something visible changed (agents,
  their state and elapsed time, the RAM shown in MB, badges, folder, remote state) and reads every 5 s while the
  window is not focused (alerts still arrive). The files panel no longer redraws when a folder re-read is unchanged.
- The Connections panel and the MCP manager are exclusive; Ctrl+Shift+M inside Connections opens the MCP manager
  and Ctrl+Shift+O inside the MCP manager opens Connections.

## F9 · Shell integration and completion
- OSC 133 marks from the PowerShell startup script; command blocks with a thin mark (red on error).
- Ghost-text completion over PSReadLine for paths, folder memory (frecency, `nucleo-matcher`) and per-folder
  history; Tab or → accepts, Esc hides. Off in remote tabs.

## F8 · Preview and diff
- Paths and URLs in the output are detected and clickable (Ctrl+hover underline, Ctrl+click, context menu).
- Split view with tabs: Markdown, images, SVG, text with line numbers, colored diffs (`similar`, `git diff`),
  auto-reload (`notify`); PDF and HTML through WebView2 (`wry`); Office → PDF with LibreOffice when installed.

## F7 · Remote files and transfers
- Files panel for the active tab: local, SFTP or WSL, never mixed, always labeled; drag from Explorer to upload,
  right-click to download; transfer queue with progress, cancel and overwrite/rename; per-session "for the agent"
  folder.

## F6 · SSH and connections
- `russh` client: remote tabs and a docked channel on the same connection, `known_hosts` verification, the Windows
  ssh-agent, keys, passwords only in the Windows Credential Manager, clear reconnect.
- Connection manager (icon button, Ctrl+Shift+O); agents installed on the server; OSC 7 guide for remote shells.

## F5 · Exact agents, alerts and audit
- `kuntur-hook.exe` + named pipe + OSC 7501: exact state (working / asking permission / waiting / done), sub-agents
  with their current action, Allow/Deny from the row, a git worktree per agent, "Send to terminal", Claude status
  line ring (5 h / weekly), hook installer with confirmation.
- Configurable agents (name, executable, arguments, log path) and more agents in the catalog; one click opens an
  agent in the active session.
- Alerts: tab badge and Windows notification; command audit with search, filters and CSV.

## F4 · Docked terminal
- A second terminal under each session (Ctrl+`), resizable, in the session's folder; its height is saved per session.

## F3 · Tabs, agents and usage (design v5)
- Sessions as tabs on top; left agents panel (search, All/Waiting/Working, installed agents with usage ring).
- Agent usage view: Today / 7 / 30 days / 1 year / All, tokens over time, hour heat map, models, agents, top tasks,
  estimated cost (`pricing.toml`), CSV export and an incremental SQLite store. RAM indicator.

## F2 · Sessions and themes
- Multiple sessions, side rail (full ↔ icons) with hover card, custom minimal title bar, Blue/Dark/White themes,
  command palette, saved and restored sessions.

## F1 · Terminal core
- GPUI window with a real PowerShell over ConPTY and `alacritty_terminal`: colors, cursor, scrollback, selection,
  smart copy/paste (Ctrl+C, Ctrl+V, right click, bracketed paste, OSC 52) and resizing.
