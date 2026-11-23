# F2 · Sesiones y temas — Plan de implementación

> **Para agentes:** ejecutar tarea por tarea con revisión entre bloques (método usado en F1). Pasos con casillas (`- [ ]`).

**Objetivo:** que Kuntur ya tenga la cara del diseño v4: varias sesiones en un riel lateral (completo ↔ íconos, nunca oculto) con tarjeta al pasar el mouse, barra de título propia y minimalista con los botones de Windows a la derecha, temas Blue / Dark / White leídos de `themes/*.toml`, paleta de comandos, configuración guardada y sesiones restauradas al abrir. Además, reporte de mouse a los programas (pendiente de F1).

**Arquitectura:**
- `kuntur-config` (sin UI): temas (`Theme`, colores `Rgba8` con alfa) y configuración persistente (`Settings` en `%APPDATA%\Kuntur\config.toml`).
- `kuntur-term`: codificación del mouse para programas (SGR 1006 y X10) y lectura del modo de mouse del emulador.
- `kuntur-app`: `SessionList<T>` (lógica pura, probada) + `Workspace` (vista raíz GPUI: barra de título, riel, área de terminal, paleta) + `TerminalView` (de F1, ahora configurable por tema y tamaño de fuente).

**Stack:** igual que F1 + `serde` 1 + `toml` 1.2 en `kuntur-config`. Íconos Lucide de gpui-kit (`IconName`), `TitleBar` y `HoverCard` de gpui-component.

**Decisión de atajos (terminal-safe, como Windows Terminal/VS Code):** Ctrl+Shift+T nueva sesión · Ctrl+Shift+W cerrar · Ctrl+Tab / Ctrl+Shift+Tab siguiente/anterior · Ctrl+1…9 ir a la sesión N · Ctrl+Shift+B riel completo ↔ íconos · Ctrl+Shift+P paleta de comandos · Ctrl+= / Ctrl+- / Ctrl+0 tamaño de fuente. (Ctrl solo chocaría con PowerShell, readline y los agentes.)

---

## Estructura de archivos

| Archivo | Acción | Responsabilidad |
|---|---|---|
| `Cargo.toml` | Modificar | Activar `serde`, `toml` en el workspace |
| `crates/kuntur-config/Cargo.toml` | Modificar | Dependencias |
| `crates/kuntur-config/src/lib.rs` | Modificar | Módulos |
| `crates/kuntur-config/src/theme.rs` | Crear | `Rgba8`, `Theme`, `ThemeId`, temas incorporados desde `themes/*.toml` |
| `crates/kuntur-config/src/settings.rs` | Crear | `Settings`, `SidebarMode`, `SessionConfig`, cargar/guardar |
| `crates/kuntur-term/src/mouse.rs` | Crear | `encode_mouse` (SGR/X10) |
| `crates/kuntur-term/src/emulator.rs` | Modificar | `mouse_mode()` |
| `crates/kuntur-term/src/palette.rs` | Modificar | `Palette::new(...)` desde colores de tema |
| `crates/kuntur-app/src/sessions.rs` | Crear | `SessionList<T>` (pura) |
| `crates/kuntur-app/src/theme_bridge.rs` | Crear | Conversión `Theme` → colores GPUI y `Palette` |
| `crates/kuntur-app/src/workspace.rs` | Crear | Vista raíz: barra de título, riel, terminal activa, paleta, atajos |
| `crates/kuntur-app/src/command_palette.rs` | Crear | Paleta Ctrl+Shift+P con filtro |
| `crates/kuntur-app/src/terminal_view.rs` | Modificar | Tema/fuente configurables, reporte de mouse, `PaneOptions` externo |
| `crates/kuntur-app/src/main.rs` | Modificar | Abrir `Workspace` con `TitleBar::window_options()` y assets de íconos |
| `scripts/smoke.ps1` | Modificar | `KUNTUR_SMOKE_SESSIONS` y presupuesto de RAM con varias sesiones |
| `docs/plans/f2-resultados.md` | Crear | Aceptación |

---

## Bloque A — Lógica sin UI (TDD, código completo)

### Task 1: Temas en `kuntur-config`

**Files:** Modify `Cargo.toml`, `crates/kuntur-config/Cargo.toml`, `crates/kuntur-config/src/lib.rs` · Create `crates/kuntur-config/src/theme.rs`

- [ ] **Paso 1: dependencias.** En `Cargo.toml` del workspace, dentro de `[workspace.dependencies]`, reemplazar las líneas comentadas de `toml` y `serde` por:

```toml
toml = "1.2"
serde = { version = "1", features = ["derive"] }
```

y `crates/kuntur-config/Cargo.toml`:

```toml
[package]
name = "kuntur-config"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[dependencies]
serde = { workspace = true }
toml = { workspace = true }
```

- [ ] **Paso 2: prueba que falla.** Crear `theme.rs` solo con:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_hex_con_y_sin_alfa() {
        assert_eq!(Rgba8::parse("#0b1019").unwrap(), Rgba8 { r: 0x0b, g: 0x10, b: 0x19, a: 0xff });
        assert_eq!(Rgba8::parse("#09144299").unwrap(), Rgba8 { r: 0x09, g: 0x14, b: 0x42, a: 0x99 });
        assert!(Rgba8::parse("0b1019").is_err());
        assert!(Rgba8::parse("#xyz").is_err());
    }

    #[test]
    fn los_tres_temas_incorporados_cargan() {
        for id in ThemeId::ALL {
            let t = builtin(id);
            assert!(!t.name.is_empty());
            assert!(t.fonts.size > 8.0);
            assert_eq!(t.ansi.colors.len(), 16);
        }
        assert_eq!(builtin(ThemeId::Blue).ui.window_gradient.len(), 3);
        assert_eq!(builtin(ThemeId::Dark).ui.text, Rgba8::parse("#d5dcea").unwrap());
    }

    #[test]
    fn id_de_tema_ida_y_vuelta() {
        for id in ThemeId::ALL {
            assert_eq!(ThemeId::from_key(id.key()), Some(id));
        }
        assert_eq!(ThemeId::from_key("rosa"), None);
    }
}
```

y en `lib.rs`:

```rust
//! Configuración de Kuntur: temas (`themes/*.toml`) y ajustes persistentes.

pub mod theme;

pub use theme::{builtin, Rgba8, Theme, ThemeId};
```

- [ ] **Paso 3: ver que falla** — `cargo test -p kuntur-config theme` → no compila (`cannot find type Rgba8`).

- [ ] **Paso 4: implementación** — arriba de las pruebas en `theme.rs`:

```rust
use serde::Deserialize;

/// Color con alfa. Acepta "#rrggbb" o "#rrggbbaa".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    pub fn parse(s: &str) -> Result<Self, String> {
        let hex = s.strip_prefix('#').ok_or_else(|| format!("color sin '#': {s}"))?;
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| format!("color inválido: {s}"));
        match hex.len() {
            6 => Ok(Self { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: 0xff }),
            8 => Ok(Self { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: byte(6)? }),
            _ => Err(format!("color inválido: {s}")),
        }
    }
}

impl TryFrom<String> for Rgba8 {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        Self::parse(&s)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Fonts {
    pub ui: String,
    pub mono: String,
    pub size: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Ui {
    pub window_gradient: Vec<Rgba8>,
    pub titlebar: Rgba8,
    pub surface: Rgba8,
    pub surface_2: Rgba8,
    pub border: Rgba8,
    pub popover: Rgba8,
    pub text: Rgba8,
    pub muted: Rgba8,
    pub accent: Rgba8,
    pub accent_2: Rgba8,
    pub ok: Rgba8,
    pub warn: Rgba8,
    pub error: Rgba8,
    pub terminal_bg: Rgba8,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Ansi {
    pub colors: [Rgba8; 16],
}

/// Un tema completo (UI + 16 colores ANSI + fuentes). La sección [syntax] del TOML se ignora aquí.
#[derive(Clone, Debug, Deserialize)]
pub struct Theme {
    pub name: String,
    pub fonts: Fonts,
    pub ui: Ui,
    pub ansi: Ansi,
}

impl Theme {
    pub fn from_toml(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// ¿Es un tema claro? (para elegir el modo claro/oscuro de los componentes de gpui-kit)
    pub fn is_light(&self) -> bool {
        let c = self.ui.text;
        (c.r as u32 + c.g as u32 + c.b as u32) < 3 * 128
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeId {
    Blue,
    Dark,
    White,
}

impl ThemeId {
    pub const ALL: [ThemeId; 3] = [ThemeId::Blue, ThemeId::Dark, ThemeId::White];

    pub fn key(self) -> &'static str {
        match self {
            ThemeId::Blue => "blue",
            ThemeId::Dark => "dark",
            ThemeId::White => "white",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.key() == key)
    }
}

/// Temas incorporados en el binario (no dependen de archivos externos).
pub fn builtin(id: ThemeId) -> Theme {
    let text = match id {
        ThemeId::Blue => include_str!("../../../themes/blue.toml"),
        ThemeId::Dark => include_str!("../../../themes/dark.toml"),
        ThemeId::White => include_str!("../../../themes/white.toml"),
    };
    Theme::from_toml(text).expect("tema incorporado inválido")
}
```

- [ ] **Paso 5: ver que pasa** — `cargo test -p kuntur-config theme` → `3 passed`.
- [ ] **Paso 6: commit** — `cargo fmt --all`; `git add Cargo.toml Cargo.lock crates/kuntur-config`; mensaje `feat(config): temas Blue/Dark/White desde themes/*.toml`.

### Task 2: Ajustes persistentes

**Files:** Create `crates/kuntur-config/src/settings.rs` · Modify `crates/kuntur-config/src/lib.rs`, `crates/kuntur-config/Cargo.toml` (agregar `serde` ya está; nada más)

- [ ] **Paso 1: prueba que falla** — crear `settings.rs` con:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kuntur-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("config.toml")
    }

    #[test]
    fn sin_archivo_usa_valores_por_defecto() {
        let s = Settings::load(&tmp("vacio"));
        assert_eq!(s, Settings::default());
        assert_eq!(s.theme, "dark");
        assert_eq!(s.sidebar, SidebarMode::Icons);
    }

    #[test]
    fn guarda_y_vuelve_a_cargar() {
        let path = tmp("ida-vuelta");
        let s = Settings {
            theme: "blue".into(),
            sidebar: SidebarMode::Full,
            font_size: 15.0,
            sessions: vec![SessionConfig { name: "web".into(), shell: None }],
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
    }

    #[test]
    fn archivo_roto_no_rompe_la_app() {
        let path = tmp("roto");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "esto no es toml [[[").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn la_vista_oculta_ya_no_existe() {
        // Regla del producto: el riel de sesiones nunca se oculta.
        let path = tmp("oculta");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "sidebar = \"hidden\"\n").unwrap();
        assert_eq!(Settings::load(&path).sidebar, SidebarMode::Icons);
    }

    #[test]
    fn limita_el_tamano_de_fuente() {
        assert_eq!(Settings::clamp_font(4.0), 9.0);
        assert_eq!(Settings::clamp_font(40.0), 28.0);
        assert_eq!(Settings::clamp_font(14.0), 14.0);
    }
}
```

y en `lib.rs`: `pub mod settings;` y `pub use settings::{SessionConfig, Settings, SidebarMode};`. Agregar `serde` con feature `derive` ya cubre `Serialize`.

- [ ] **Paso 2: ver que falla** — `cargo test -p kuntur-config settings` → no compila.
- [ ] **Paso 3: implementación** — arriba de las pruebas:

```rust
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Vista del riel de sesiones. No existe "oculta": el mínimo son los íconos.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarMode {
    #[default]
    #[serde(other)]
    Icons,
    Full,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionConfig {
    pub name: String,
    /// Programa del shell; `None` = el predeterminado (PowerShell 7 o Windows PowerShell).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub sidebar: SidebarMode,
    pub font_size: f32,
    pub sessions: Vec<SessionConfig>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { theme: "dark".into(), sidebar: SidebarMode::Icons, font_size: 14.0, sessions: Vec::new() }
    }
}

impl Settings {
    /// `%APPDATA%\Kuntur\config.toml`
    pub fn default_path() -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Kuntur").join("config.toml"))
    }

    /// Nunca falla: si no hay archivo o está roto, devuelve los valores por defecto.
    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| toml::from_str::<Settings>(&t).ok())
            .map(|mut s| {
                s.font_size = Self::clamp_font(s.font_size);
                s
            })
            .unwrap_or_default()
    }

    /// Guarda de forma segura: escribe a un archivo temporal y luego lo renombra.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    pub fn clamp_font(size: f32) -> f32 {
        size.clamp(9.0, 28.0)
    }
}
```

- [ ] **Paso 4: ver que pasa** — `cargo test -p kuntur-config` → `8 passed`.
- [ ] **Paso 5: commit** — `feat(config): ajustes persistentes en %APPDATA%\Kuntur\config.toml`.

### Task 3: Paleta desde el tema

**Files:** Modify `crates/kuntur-term/src/palette.rs`

- [ ] **Paso 1: prueba que falla** — agregar al módulo `tests` de `palette.rs`:

```rust
    #[test]
    fn construye_desde_colores_de_tema() {
        let ansi = [Rgb8::hex(0x000000); 16];
        let p = Palette::new(ansi, Rgb8::hex(0xffffff), Rgb8::hex(0x101010), Rgb8::hex(0x00ff00), Rgb8::hex(0x333333));
        assert_eq!(p.fg, Rgb8::hex(0xffffff));
        assert_eq!(p.resolve(Color::Named(NamedColor::Background)), Rgb8::hex(0x101010));
        assert_eq!(p.selection_bg, Rgb8::hex(0x333333));
    }
```

- [ ] **Paso 2: ver que falla** — `cargo test -p kuntur-term palette` → `no function new`.
- [ ] **Paso 3: implementación** — en `impl Palette`:

```rust
    pub fn new(ansi: [Rgb8; 16], fg: Rgb8, bg: Rgb8, cursor: Rgb8, selection_bg: Rgb8) -> Self {
        Self { ansi, fg, bg, cursor, selection_bg }
    }
```

- [ ] **Paso 4: pasa** — `4 passed` en `palette`. **Paso 5: commit** — `feat(term): Palette::new desde colores de tema`.

### Task 4: Reporte de mouse a los programas

**Files:** Create `crates/kuntur-term/src/mouse.rs` · Modify `crates/kuntur-term/src/emulator.rs`, `crates/kuntur-term/src/lib.rs`

- [ ] **Paso 1: prueba que falla** — crear `mouse.rs` con:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Mods;

    fn ev(button: MouseButtonKind, action: MouseAction, col: usize, row: usize) -> MouseReport {
        MouseReport { button, action, col, row, mods: Mods::NONE }
    }

    #[test]
    fn sgr_clic_y_soltar() {
        assert_eq!(encode_mouse(&ev(MouseButtonKind::Left, MouseAction::Press, 0, 0), true), Some(b"\x1b[<0;1;1M".to_vec()));
        assert_eq!(encode_mouse(&ev(MouseButtonKind::Left, MouseAction::Release, 9, 4), true), Some(b"\x1b[<0;10;5m".to_vec()));
        assert_eq!(encode_mouse(&ev(MouseButtonKind::Right, MouseAction::Press, 2, 3), true), Some(b"\x1b[<2;3;4M".to_vec()));
    }

    #[test]
    fn sgr_rueda_arrastre_y_modificadores() {
        assert_eq!(encode_mouse(&ev(MouseButtonKind::WheelUp, MouseAction::Press, 0, 0), true), Some(b"\x1b[<64;1;1M".to_vec()));
        assert_eq!(encode_mouse(&ev(MouseButtonKind::WheelDown, MouseAction::Press, 0, 0), true), Some(b"\x1b[<65;1;1M".to_vec()));
        assert_eq!(encode_mouse(&ev(MouseButtonKind::Left, MouseAction::Move, 1, 1), true), Some(b"\x1b[<32;2;2M".to_vec()));
        let mut e = ev(MouseButtonKind::Left, MouseAction::Press, 0, 0);
        e.mods = Mods { ctrl: true, alt: false, shift: true };
        assert_eq!(encode_mouse(&e, true), Some(b"\x1b[<20;1;1M".to_vec()));
    }

    #[test]
    fn x10_clasico_y_su_limite() {
        assert_eq!(encode_mouse(&ev(MouseButtonKind::Left, MouseAction::Press, 0, 0), false), Some(vec![0x1b, b'[', b'M', 32, 33, 33]));
        assert_eq!(encode_mouse(&ev(MouseButtonKind::Left, MouseAction::Release, 0, 0), false), Some(vec![0x1b, b'[', b'M', 35, 33, 33]));
        assert_eq!(encode_mouse(&ev(MouseButtonKind::Left, MouseAction::Press, 300, 0), false), None);
    }
}
```

y en `emulator.rs`, dentro de su módulo `tests`:

```rust
    #[test]
    fn detecta_modo_de_mouse() {
        let mut emu = Emulator::new(GridSize::new(10, 2));
        assert_eq!(emu.mouse_mode(), MouseMode { report: MouseReporting::Off, sgr: false });
        emu.feed(b"\x1b[?1002h\x1b[?1006h");
        assert_eq!(emu.mouse_mode(), MouseMode { report: MouseReporting::Drag, sgr: true });
    }
```

En `lib.rs`: `pub mod mouse;` y `pub use mouse::{encode_mouse, MouseAction, MouseButtonKind, MouseMode, MouseReport, MouseReporting};`.

- [ ] **Paso 2: ver que falla** — `cargo test -p kuntur-term mouse` → no compila.
- [ ] **Paso 3: implementación** — arriba de las pruebas en `mouse.rs`:

```rust
use crate::keys::Mods;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButtonKind {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Move,
}

/// Evento de mouse en coordenadas de celda (0-based) para enviar al programa.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseReport {
    pub button: MouseButtonKind,
    pub action: MouseAction,
    pub col: usize,
    pub row: usize,
    pub mods: Mods,
}

/// Qué pidió el programa (DECSET 1000/1002/1003) y si usa el formato SGR (1006).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseReporting {
    Off,
    Click,
    Drag,
    Motion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseMode {
    pub report: MouseReporting,
    pub sgr: bool,
}

/// Codifica el evento como xterm. `sgr = true` → `ESC[<b;x;yM|m`; si no, X10 (`ESC[M` + 3 bytes).
pub fn encode_mouse(ev: &MouseReport, sgr: bool) -> Option<Vec<u8>> {
    let mut code: u32 = match ev.button {
        MouseButtonKind::Left => 0,
        MouseButtonKind::Middle => 1,
        MouseButtonKind::Right => 2,
        MouseButtonKind::WheelUp => 64,
        MouseButtonKind::WheelDown => 65,
    };
    if ev.action == MouseAction::Move {
        code += 32;
    }
    if ev.mods.shift {
        code += 4;
    }
    if ev.mods.alt {
        code += 8;
    }
    if ev.mods.ctrl {
        code += 16;
    }
    let (x, y) = (ev.col + 1, ev.row + 1);
    if sgr {
        let end = if ev.action == MouseAction::Release { 'm' } else { 'M' };
        return Some(format!("\x1b[<{code};{x};{y}{end}").into_bytes());
    }
    if x > 223 || y > 223 {
        return None;
    }
    if ev.action == MouseAction::Release {
        code = (code & !0b11) | 3;
    }
    Some(vec![0x1b, b'[', b'M', 32 + code as u8, 32 + x as u8, 32 + y as u8])
}
```

y en `impl Emulator` (agregar `use crate::mouse::{MouseMode, MouseReporting};` arriba):

```rust
    /// Modo de mouse que pidió el programa (nvim, htop, opencode…).
    pub fn mouse_mode(&self) -> MouseMode {
        let m = self.mode();
        let report = if m.contains(TermMode::MOUSE_MOTION) {
            MouseReporting::Motion
        } else if m.contains(TermMode::MOUSE_DRAG) {
            MouseReporting::Drag
        } else if m.contains(TermMode::MOUSE_REPORT_CLICK) {
            MouseReporting::Click
        } else {
            MouseReporting::Off
        };
        MouseMode { report, sgr: m.contains(TermMode::SGR_MOUSE) }
    }
```

- [ ] **Paso 4: pasa** — `cargo test -p kuntur-term` → `31 passed` (26 de F1 + 1 paleta + 3 mouse + 1 emulador).
- [ ] **Paso 5: commit** — `feat(term): reporte de mouse SGR/X10 y modo de mouse del emulador`.

### Task 5: Lista de sesiones (pura)

**Files:** Create `crates/kuntur-app/src/sessions.rs` · Modify `crates/kuntur-app/src/main.rs` (agregar `mod sessions;`)

- [ ] **Paso 1: prueba que falla** — crear `sessions.rs` con:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agregar_activa_la_nueva() {
        let mut s = SessionList::new();
        let a = s.push("uno", 'a');
        let b = s.push("dos", 'b');
        assert_eq!(s.active_id(), Some(b));
        assert_eq!(s.len(), 2);
        assert!(s.activate(a));
        assert_eq!(s.active().unwrap().value, 'a');
    }

    #[test]
    fn cerrar_la_activa_activa_la_vecina_izquierda() {
        let mut s = SessionList::new();
        let a = s.push("uno", 1);
        let b = s.push("dos", 2);
        let c = s.push("tres", 3);
        s.activate(b);
        assert_eq!(s.close(b), Some(2));
        assert_eq!(s.active_id(), Some(a));
        assert_eq!(s.close(a), Some(1));
        assert_eq!(s.active_id(), Some(c));
        assert_eq!(s.close(c), Some(3));
        assert!(s.is_empty());
        assert_eq!(s.active_id(), None);
    }

    #[test]
    fn siguiente_y_anterior_dan_la_vuelta() {
        let mut s = SessionList::new();
        let a = s.push("uno", ());
        let _b = s.push("dos", ());
        let c = s.push("tres", ());
        s.next();
        assert_eq!(s.active_id(), Some(a));
        s.prev();
        assert_eq!(s.active_id(), Some(c));
        assert!(s.activate_index(1));
        assert!(!s.activate_index(9));
    }

    #[test]
    fn renombrar() {
        let mut s = SessionList::new();
        let a = s.push("uno", ());
        s.rename(a, "web");
        assert_eq!(s.iter().next().unwrap().name, "web");
    }
}
```

- [ ] **Paso 2: ver que falla** — `cargo test -p kuntur-app sessions` → no compila.
- [ ] **Paso 3: implementación** — arriba de las pruebas:

```rust
/// Una sesión de la barra lateral. `T` es lo que la UI guarda (la vista de terminal).
pub struct Entry<T> {
    pub id: u64,
    pub name: String,
    pub value: T,
}

/// Lista ordenada de sesiones con una activa. Sin GPUI: se prueba aparte.
pub struct SessionList<T> {
    items: Vec<Entry<T>>,
    active: usize,
    next_id: u64,
}

impl<T> SessionList<T> {
    pub fn new() -> Self {
        Self { items: Vec::new(), active: 0, next_id: 1 }
    }

    pub fn push(&mut self, name: impl Into<String>, value: T) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Entry { id, name: name.into(), value });
        self.active = self.items.len() - 1;
        id
    }

    /// Quita la sesión; si era la activa, activa la de la izquierda (o la primera).
    pub fn close(&mut self, id: u64) -> Option<T> {
        let idx = self.items.iter().position(|e| e.id == id)?;
        let removed = self.items.remove(idx);
        if self.items.is_empty() {
            self.active = 0;
        } else if idx < self.active || (idx == self.active && idx > 0) {
            self.active -= 1;
        } else if self.active >= self.items.len() {
            self.active = self.items.len() - 1;
        }
        Some(removed.value)
    }

    pub fn activate(&mut self, id: u64) -> bool {
        match self.items.iter().position(|e| e.id == id) {
            Some(i) => {
                self.active = i;
                true
            }
            None => false,
        }
    }

    pub fn activate_index(&mut self, index: usize) -> bool {
        if index < self.items.len() {
            self.active = index;
            true
        } else {
            false
        }
    }

    pub fn next(&mut self) {
        if !self.items.is_empty() {
            self.active = (self.active + 1) % self.items.len();
        }
    }

    pub fn prev(&mut self) {
        if !self.items.is_empty() {
            self.active = (self.active + self.items.len() - 1) % self.items.len();
        }
    }

    pub fn rename(&mut self, id: u64, name: impl Into<String>) {
        if let Some(e) = self.items.iter_mut().find(|e| e.id == id) {
            e.name = name.into();
        }
    }

    pub fn active(&self) -> Option<&Entry<T>> {
        self.items.get(self.active)
    }

    pub fn active_id(&self) -> Option<u64> {
        self.active().map(|e| e.id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Entry<T>> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T> Default for SessionList<T> {
    fn default() -> Self {
        Self::new()
    }
}
```

Si clippy marca `dead_code` en la app antes de usarse (Task 6), agregar temporalmente `#![allow(dead_code)]` al inicio de `sessions.rs` y quitarlo en Task 6.

- [ ] **Paso 4: pasa** — `cargo test -p kuntur-app` → `4 passed`. Nota: el test de `cerrar…` define la regla exacta (cerrar la activa `b` → queda `a`; cerrar `a` (primera, activa) → queda `c`).
- [ ] **Paso 5: commit** — `feat(app): lista de sesiones con activa, cierre y rotación`.

---

## Bloque B — Interfaz (especificación + APIs verificadas)

> En la UI el código se escribe contra el compilador (las APIs de GPUI cambian entre versiones). Cada tarea trae **qué debe hacer**, **las APIs ya verificadas** y **cómo se acepta**. Probar SIEMPRE con `scripts/smoke.ps1` (nunca simular teclado del sistema).

**APIs verificadas (gpui-kit 0.11 / GPUI 0.3.14):**
- Barra de título: `gpui_kit::component::TitleBar::new().child(...)` (implementa `Styled` → `.bg(..)`, `.border_color(..)`); ventana con `WindowOptions { ..TitleBar::window_options() }`. En Windows dibuja minimizar/maximizar/cerrar a la derecha (nativos, con `window_control_area`) y la zona de arrastre. Alto `TITLE_BAR_HEIGHT` = 34 px. Sus colores salen de `cx.theme()` (gpui-kit): elegir modo claro/oscuro con el tema.
- Íconos: `Icon::new(IconName::X)` con `IconName::{SquareTerminal, Terminal, Server, Globe, PenTool, Package, Plus, X, PanelLeft, PanelRight, Command, Palette, Settings, Sun, Moon, Search, Ellipsis}`. **Requiere** `gpui_kit::application().with_assets(gpui_kit::assets::Assets)` en `main`.
- Tarjeta al pasar el mouse: `HoverCard::new(id).trigger(elemento).content(|window, cx| ...).open_delay(Duration).close_delay(Duration).anchor(...)`.
- Degradado: `linear_gradient(angulo, linear_color_stop(color, 0.), linear_color_stop(color, 1.))` como `Background` en `.bg(...)`.
- Foco: `window.focus(&handle, cx)`; teclado: `.on_key_down(cx.listener(...))` con `KeyDownEvent.keystroke { modifiers{control,shift,alt}, key, key_char }`.
- Texto: `ShapedLine::paint(origin, line_h, TextAlign::Left, None, window, cx)`; fuente: `font(familia)`; disponibilidad de fuentes: `window.text_system().all_font_names()`.

### Task 6: `Workspace` — barra de título, riel y sesiones

**Files:** Create `crates/kuntur-app/src/workspace.rs`, `crates/kuntur-app/src/theme_bridge.rs` · Modify `main.rs`, `terminal_view.rs`, `crates/kuntur-app/Cargo.toml` (agregar `kuntur-config = { path = "../kuntur-config" }`)

**Debe:**
1. `main.rs`: `application().with_assets(gpui_kit::assets::Assets)`, cargar `Settings::load(default_path)`, abrir ventana con `TitleBar::window_options()` (título "Kuntur") y vista raíz `Workspace`.
2. `TerminalView::new(window, cx, opts: PaneOptions, palette: Palette, font_family: String, font_size: f32)`; métodos `set_palette(Palette)`, `set_font(String, f32)`, `title() -> Option<String>` (OSC 0/2 del programa), `focus(window, cx)`. Mantener toda la lógica de F1 (teclado, selección, portapapeles, `KUNTUR_SMOKE_INPUT` solo en la primera sesión).
3. `Workspace` guarda `SessionList<Entity<TerminalView>>`, `Settings`, `Theme`. Al abrir: una sesión por cada `settings.sessions` (o una si está vacío), nombre por defecto "PowerShell", "PowerShell 2", ….
4. Diseño v4 (ver `docs/PLAN.md` "Cambios de la v4" y `docs/simulacion/index.html`):
   - **Barra de título (34 px)**: a la izquierda botón ícono `PanelLeft` (riel completo ↔ íconos); al centro, en texto tenue, "nombre · título del programa"; a la derecha botón `Command` (abre la paleta) y luego los controles nativos de Windows. Fondo = `ui.titlebar` del tema; borde inferior 1 px `ui.border`.
   - **Riel** a la izquierda: modo **Íconos** 60 px (solo ícono `SquareTerminal` por sesión, activa con fondo `ui.surface_2` y barra de acento de 3 px a la izquierda; `+` al pie) y modo **Completo** 232 px (ícono + nombre + título del programa en tenue + `×` al pasar el mouse; `+` en la cabecera). **Nunca se oculta.**
   - En modo íconos, `HoverCard` por sesión con: nombre, título del programa y atajo `Ctrl+N`.
   - **Área de terminal**: la vista activa ocupa el resto; las demás sesiones siguen vivas (sus PTY corren) pero no se dibujan.
   - Fondo de ventana: si `ui.window_gradient` tiene 2+ colores, degradado a 140°; si no, color plano.
5. Atajos (en el `Workspace`, antes que la terminal): Ctrl+Shift+T, Ctrl+Shift+W, Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+1…9, Ctrl+Shift+B. Cerrar la última sesión cierra la ventana.
6. Al cambiar de sesión, el foco pasa a la terminal activa.

**Aceptación:** `cargo build -p kuntur-app --release` sin advertencias; `scripts/smoke.ps1` (Task 9) muestra riel + barra de título; con 3 sesiones RAM < 90 MB.

- [ ] Implementar · [ ] `cargo clippy --workspace --all-targets -- -D warnings` · [ ] commit `feat(app): workspace con barra de título, riel de sesiones y atajos`.

### Task 7: Temas aplicados en vivo

**Files:** Modify `theme_bridge.rs`, `workspace.rs`, `terminal_view.rs`

**Debe:** `theme_bridge.rs` con `to_hsla(Rgba8) -> Hsla` (respetando alfa) y `palette_from(&Theme) -> Palette` (ansi, `ui.text`, `ui.terminal_bg` sin alfa mezclado sobre el primer color del degradado, cursor = `ui.accent`, selección = `ui.accent` al 30 % sobre el fondo). Cambiar de tema actualiza ventana, barra, riel y **todas** las terminales sin reiniciar. Fuente: usar `fonts.mono` si está instalada (`all_font_names`), si no **Cascadia Mono**. Ajustar el modo claro/oscuro de gpui-kit según `Theme::is_light()` para que los controles de ventana contrasten.

**Pruebas unitarias** en `theme_bridge.rs`: `to_hsla` conserva alfa (`#ffffff80` → `a ≈ 0.5`); `palette_from(&builtin(Dark)).fg == Rgb8::hex(0xd5dcea)`.

- [ ] Implementar · [ ] pruebas · [ ] commit `feat(app): temas Blue/Dark/White aplicados en vivo`.

### Task 8: Paleta de comandos y configuración persistente

**Files:** Create `command_palette.rs` · Modify `workspace.rs`

**Debe:** Ctrl+Shift+P (y el botón `Command`) abre un panel flotante centrado arriba (ancho 520 px, fondo `ui.popover`, borde `ui.border`, radio 10) con campo de filtro y lista navegable (↑/↓/Enter/Esc). Acciones: Nueva sesión · Cerrar sesión · Siguiente/Anterior sesión · Tema: Blue / Dark / White · Riel: completo / íconos · Fuente: más grande / más chica / restablecer · Renombrar sesión (pide el nombre en el mismo campo). Filtro sin distinguir mayúsculas ni tildes. Cada cambio de tema, riel, fuente o sesiones guarda `Settings` (Task 2) al momento; al abrir Kuntur se restauran tema, riel, fuente y la lista de sesiones (nombre + shell).

**Prueba unitaria** de la función pura de filtro `matches(query, label) -> bool` (sin tildes: "fuente" encuentra "Fuente: más grande"; "tema blu" encuentra "Tema: Blue").

- [ ] Implementar · [ ] pruebas · [ ] commit `feat(app): paleta de comandos y configuración persistente`.

### Task 9: Mouse para programas, humo y aceptación

**Files:** Modify `terminal_view.rs`, `scripts/smoke.ps1` · Create `docs/plans/f2-resultados.md`

**Debe:**
1. Si `emulator.mouse_mode().report != Off`, los clics, la rueda y (según el modo) el arrastre/movimiento se envían al programa con `encode_mouse`; **Shift + arrastrar** fuerza la selección de Kuntur. Con `Off`, todo sigue como en F1.
2. `scripts/smoke.ps1`: nuevo parámetro `-Sessions <n>` que exporta `KUNTUR_SMOKE_SESSIONS=n` (la app crea n sesiones al abrir, sin tocar la configuración guardada) y `-Theme <blue|dark|white>` → `KUNTUR_SMOKE_THEME`; `KUNTUR_CONFIG` (ruta de config alternativa) para que la prueba no pise la configuración real. Presupuesto por defecto 90 MB.
3. Capturas: `smoke.ps1 -Sessions 3 -Theme blue` y `-Theme white`, revisadas a ojo.
4. `docs/plans/f2-resultados.md` con la lista de aceptación manual (cambiar de sesión con clic y atajos, cerrar con ×, riel completo ↔ íconos, tarjeta al pasar el mouse, temas, paleta, reinicio restaura todo, mouse dentro de `opencode`).

- [ ] Implementar · [ ] humo en los 3 temas · [ ] CI verde · [ ] commit y push de la rama.

---

## Fuera de alcance (fases siguientes)
Terminal acoplada (F3) · panel de agentes (F4) · uso y costos (F5) · SSH (F6) · archivos (F7) · OSC 7 / carpeta actual e ícono por tipo de proyecto (F8) · MCP y comandos con atajo (F9) · IME y ligaduras (F10).

## Autorrevisión
- Cobertura de la hoja de ruta F2: varias sesiones ✅ (T5, T6) · riel completo ↔ íconos, nunca oculto ✅ (T2, T6) · tarjeta al pasar el mouse ✅ (T6) · barra de título propia con botones a la derecha ✅ (T6) · temas desde `themes/*.toml` ✅ (T1, T7) · paleta Ctrl+K→Ctrl+Shift+P ✅ (T8) · guardar/restaurar ✅ (T2, T8) · pendiente de F1: mouse a programas ✅ (T4, T9).
- Nombres consistentes: `Rgba8`, `Theme`, `ThemeId::{ALL,key,from_key}`, `builtin`, `Settings::{load,save,default_path,clamp_font}`, `SidebarMode::{Icons,Full}`, `SessionConfig`, `Palette::new`, `encode_mouse`, `MouseReport`, `MouseMode`, `MouseReporting`, `Emulator::mouse_mode`, `SessionList::{push,close,activate,activate_index,next,prev,rename,active,active_id,iter,len,is_empty}`.
