# Contributing to Kuntur

Thanks for your interest! Issues and pull requests are welcome. The project's internal docs, code comments,
test names and commit messages are written in **Spanish**; English issues and PRs are fine, and maintainers will
help with the Spanish parts.

## Quick rules / Reglas rápidas

1. **Before every commit / antes de cada commit** (CI checks the same on Windows):
   ```powershell
   cargo fmt --all
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```
2. **Logic lives outside the UI.** `kuntur-*` crates have no GPUI and are tested without a window (TDD: failing
   test → minimal code → passing test). `kuntur-app` only renders "snapshots" of that state.
   *La lógica vive en crates sin GPUI, con pruebas; la UI solo lee el estado.*
3. **GPUI's `test` macro:** in `crates/kuntur-app`, `mod tests` must not do `use super::*` when `super` imports the
   `gpui_kit::*` glob (it brings its own `test` macro). Import what you need explicitly.
4. **Visual checks only with `scripts/smoke.ps1`.** It builds, opens the window on a **secondary monitor without
   taking focus**, types commands *from inside the app* (`KUNTUR_SMOKE_INPUT`), captures only its window and
   measures RAM (`-Measure` also measures startup time and idle CPU). Never simulate system keystrokes (SendKeys)
   and never show real Windows notifications from tests. Keep the UTF-8 BOM and CRLF endings of `.ps1` files.
5. **Privacy / Privacidad.** No personal paths (`C:\Users\<name>`), names, e-mails, institutions, tokens or real
   agent history in the repository, tests or screenshots. Tests use synthetic data; smoke tests never read the real
   `~/.claude`, `~/.ssh` or the user's config (`KUNTUR_CONFIG` points to `target\`). Screenshots for docs: `cd` to a
   neutral folder such as `C:\Windows` first and check the image before committing.
6. **Design:** minimal (one bar on top, hierarchy through text weight and color, color only for state), shortcuts
   with Ctrl+Shift. Text must keep WCAG AA contrast in the three themes (a test checks `themes/*.toml`).
7. **Commits:** small, with a message that explains the *why*. Spanish is preferred.

## Building the MSI installer

1. Get the official WiX Toolset v3.14.1 binaries (`wix314-binaries.zip` from the `wixtoolset/wix3` releases), put
   them in a folder such as `%LOCALAPPDATA%\Programs\WiX\bin` and set the user variable `WIX` to the parent folder.
   (The `.exe` files in that zip are not Authenticode-signed; the signed installer `wix314.exe` contains the same
   files, which can be checked by hash.)
2. `cargo install cargo-wix --locked`
3. `.\scripts\msi.ps1 -Check` → `target\wix\kuntur-<version>-x86_64.msi`. `-Check` extracts it with `msiexec /a`
   into a temporary folder (no installation) and lists its files, shortcut AUMID and registry entries.

The installer source is `crates/kuntur-app/wix/main.wxs`; the version comes from `Cargo.toml`.

## Layout

See the structure section of [README.es.md](README.es.md) and the roadmap in
[`docs/plans/README.md`](docs/plans/README.md). Each phase has its plan (`docs/plans/2026-…-fN-….md`) and its results
(`docs/plans/fN-resultados.md`).

## License

By contributing you agree that your contributions are dual licensed under MIT OR Apache-2.0, like the project.
