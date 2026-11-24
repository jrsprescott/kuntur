# Security policy

## Reporting a vulnerability

Please **do not open a public issue** for security problems. Use GitHub's private vulnerability reporting
(*Security → Report a vulnerability* in this repository). Include the Kuntur version, Windows version, steps to
reproduce and the impact you expect. You should get an answer within a week; fixes go out in the next release and
are credited in the changelog unless you prefer otherwise.

## Supported versions

Only the latest release is supported while Kuntur is in the 0.x series.

## Scope notes

Areas where a bug would matter most:

- **SSH** (`kuntur-ssh`): host key verification (`known_hosts`), authentication, the Windows ssh-agent pipe and
  passwords in the Windows Credential Manager.
- **Hooks** (`kuntur-hook`, named pipe `\\.\pipe\kuntur-…`): messages from agent hooks into the app, the
  Allow/Deny answers and the hook installer that edits Claude Code's `settings.json`.
- **Terminal escape sequences** (OSC 7/9;9/52/133/7501) coming from untrusted program output, and clickable links.
- **Preview** of local files (Markdown, SVG, HTML/PDF in WebView2).

Kuntur has no telemetry and makes no network connections of its own (see the README).
