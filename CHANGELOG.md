# Changelog

All notable changes to Kuntur. The format follows [Keep a Changelog](https://keepachangelog.com/) and the project
uses [Semantic Versioning](https://semver.org/). Development happened in phases (F1…F11); details of each phase, in
Spanish, are in [`docs/plans/`](docs/plans/).

## [Unreleased]

### In progress
- **F10 · MCP and commands:** per-session MCP servers with an icon button, catalog, enable/disable per tab,
  generated MCP config for each agent CLI, "Test" status, model/effort/permissions and custom commands.

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
