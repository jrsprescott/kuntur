# F1 · Núcleo de terminal — Plan de implementación

> **Para agentes:** SUB-SKILL REQUERIDA: usar `superpowers:subagent-driven-development` (recomendado) o `superpowers:executing-plans` para ejecutar este plan tarea por tarea. Los pasos usan casillas (`- [ ]`) para seguir el avance.

**Objetivo:** una ventana de Kuntur (GPUI) que abre PowerShell real por ConPTY, lo dibuja con colores, cursor y scroll, y permite seleccionar, copiar y pegar sin fallas, en menos de 80 MB de RAM.

**Arquitectura:** `kuntur-term` (sin UI) une `portable-pty` (ConPTY) con `alacritty_terminal` (emulador VT). Un hilo lee el PTY, alimenta el emulador y contesta las consultas del programa (`PtyWrite`). La UI (`kuntur-app`, GPUI vía gpui-kit) nunca toca el emulador al dibujar: toma una "foto" (`Screen`) por cuadro y la pinta con un `canvas`, fila por fila, con ancho de celda fijo.

**Stack:** Rust 1.100 (edition 2024) · gpui-kit 0.11 (GPUI 0.3.14) · alacritty_terminal 0.26 (vte 0.15) · portable-pty 0.9 · smol 2 · anyhow 1 · Windows 11 + ConPTY.

**Verificado:** todo el código de este plan se compiló y ejecutó en un prototipo el 2026-11-23: 25 pruebas OK (2 con ConPTY real), clippy limpio, ventana con PowerShell 7 a 57–71 MB. Ver `docs/plans/README.md`.

---

## Estructura de archivos

| Archivo | Acción | Responsabilidad |
|---|---|---|
| `Cargo.toml` | Modificar | Activar dependencias del workspace |
| `rustfmt.toml` | Crear | Ancho de línea 120 (el código del plan usa ese ancho) |
| `crates/kuntur-term/Cargo.toml` | Modificar | Dependencias del núcleo |
| `crates/kuntur-term/src/lib.rs` | Modificar | Módulos y reexportaciones |
| `crates/kuntur-term/src/size.rs` | Crear | `GridSize`: columnas × filas, conversión desde píxeles y a `PtySize` |
| `crates/kuntur-term/src/paste.rs` | Crear | Pegado seguro (saltos de línea, bracketed paste) |
| `crates/kuntur-term/src/keys.rs` | Crear | Teclas → bytes xterm |
| `crates/kuntur-term/src/palette.rs` | Crear | Colores ANSI/256/truecolor → RGB (tema Dark) |
| `crates/kuntur-term/src/emulator.rs` | Crear | `Emulator`: alacritty `Term` + parser vte, selección, scroll, eventos |
| `crates/kuntur-term/src/screen.rs` | Crear | `Screen`: foto de la parte visible lista para dibujar |
| `crates/kuntur-term/src/pane.rs` | Crear | `Pane`: proceso en ConPTY + hilo lector + escritura/pegado/redimensión |
| `crates/kuntur-app/Cargo.toml` | Modificar | Dependencias de la app |
| `crates/kuntur-app/src/main.rs` | Reemplazar | Abre la ventana |
| `crates/kuntur-app/src/terminal_view.rs` | Crear | Vista GPUI: dibujo, teclado, mouse, portapapeles, redimensión |
| `scripts/smoke.ps1` | Crear | Prueba de humo: abre Kuntur, escribe comandos desde dentro, captura su ventana, mide RAM |
| `.github/workflows/ci.yml` | Crear | fmt + clippy + tests en Windows |
| `docs/plans/f1-resultados.md` | Crear | Resultados de la aceptación manual |

---

### Task 0: Rama y dependencias

**Files:**
- Modify: `Cargo.toml`
- Create: `rustfmt.toml`
- Modify: `crates/kuntur-term/Cargo.toml`

- [ ] **Paso 1: crear la rama de la fase**

```powershell
# desde la carpeta raíz del repositorio kuntur
git switch -c fase/f1-nucleo
```
Esperado: `Switched to a new branch 'fase/f1-nucleo'`

- [ ] **Paso 2: activar dependencias del workspace** — en `Cargo.toml`, reemplazar la sección `[workspace.dependencies]` completa por:

```toml
[workspace.dependencies]
# F1
gpui-kit = "0.11"
alacritty_terminal = "0.26"
portable-pty = "0.9"
anyhow = "1"
smol = "2"
# Se irán activando por fase (ver docs/plans/README.md).
# sysinfo = "0.40"             # F4 (kuntur-agents)
# notify = "9"                 # F5 (kuntur-usage)
# nucleo = "0.5"               # F8 (kuntur-complete)
# rusqlite = { version = "0.43", features = ["bundled"] }  # F8
# toml = "1.2"                 # F2 (kuntur-config)
# serde = { version = "1", features = ["derive"] }
# serde_json = "1"
```

- [ ] **Paso 3: crear `rustfmt.toml`**

```toml
max_width = 120
```

- [ ] **Paso 4: dependencias de `kuntur-term`** — reemplazar `crates/kuntur-term/Cargo.toml` por:

```toml
[package]
name = "kuntur-term"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[dependencies]
alacritty_terminal = { workspace = true }
portable-pty = { workspace = true }
anyhow = { workspace = true }
```

- [ ] **Paso 5: comprobar que compila**

Run: `cargo check -p kuntur-term`
Esperado: `Finished` (descarga alacritty_terminal, vte, portable-pty).

- [ ] **Paso 6: commit**

```powershell
git add Cargo.toml Cargo.lock rustfmt.toml crates/kuntur-term/Cargo.toml
git commit -m "build(f1): dependencias del núcleo de terminal"
```

---

### Task 1: `GridSize`

**Files:**
- Create: `crates/kuntur-term/src/size.rs`
- Modify: `crates/kuntur-term/src/lib.rs`

- [ ] **Paso 1: escribir la prueba que falla** — crear `crates/kuntur-term/src/size.rs` solo con las pruebas:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimo_valido() {
        assert_eq!(GridSize::new(0, 0), GridSize { cols: 2, rows: 1 });
    }

    #[test]
    fn desde_pixeles() {
        assert_eq!(GridSize::from_pixels(800.0, 600.0, 8.0, 20.0), GridSize { cols: 100, rows: 30 });
    }
}
```

y reemplazar `crates/kuntur-term/src/lib.rs` por:

```rust
//! Núcleo de terminal de Kuntur: PTY (ConPTY vía portable-pty) + emulador VT (alacritty_terminal).
//! No depende de la UI: la app dibuja a partir de [`Screen`].

pub mod size;

pub use size::GridSize;
```

- [ ] **Paso 2: ver que falla**

Run: `cargo test -p kuntur-term size`
Esperado: FALLA al compilar con `cannot find type GridSize` / `unresolved import size::GridSize`.

- [ ] **Paso 3: implementación mínima** — agregar al inicio de `size.rs` (encima de las pruebas):

```rust
use alacritty_terminal::grid::Dimensions;

/// Tamaño de la grilla en celdas (columnas × filas visibles).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSize {
    pub cols: u16,
    pub rows: u16,
}

impl GridSize {
    /// Crea un tamaño válido: mínimo 2 columnas y 1 fila.
    pub fn new(cols: u16, rows: u16) -> Self {
        Self { cols: cols.max(2), rows: rows.max(1) }
    }

    /// Calcula cuántas celdas caben en un área en píxeles.
    pub fn from_pixels(width: f32, height: f32, cell_w: f32, cell_h: f32) -> Self {
        let cols = (width / cell_w).floor().max(0.0) as u16;
        let rows = (height / cell_h).floor().max(0.0) as u16;
        Self::new(cols, rows)
    }
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }
    fn screen_lines(&self) -> usize {
        self.rows as usize
    }
    fn columns(&self) -> usize {
        self.cols as usize
    }
}

impl From<GridSize> for portable_pty::PtySize {
    fn from(s: GridSize) -> Self {
        portable_pty::PtySize { rows: s.rows, cols: s.cols, pixel_width: 0, pixel_height: 0 }
    }
}
```

- [ ] **Paso 4: ver que pasa**

Run: `cargo test -p kuntur-term size`
Esperado: `test result: ok. 2 passed`

- [ ] **Paso 5: commit**

```powershell
cargo fmt --all
git add crates/kuntur-term
git commit -m "feat(term): GridSize con conversión desde píxeles y a PtySize"
```

---

### Task 2: Pegado seguro

**Files:**
- Create: `crates/kuntur-term/src/paste.rs`
- Modify: `crates/kuntur-term/src/lib.rs`

- [ ] **Paso 1: prueba que falla** — crear `paste.rs` con:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normaliza_saltos() {
        assert_eq!(paste_bytes("a\r\nb\nc", false), b"a\rb\rc".to_vec());
    }

    #[test]
    fn envuelve_en_bracketed_paste() {
        assert_eq!(paste_bytes("ls\n", true), b"\x1b[200~ls\r\x1b[201~".to_vec());
    }

    #[test]
    fn no_permite_cerrar_el_pegado_desde_el_texto() {
        assert_eq!(paste_bytes("x\x1b[201~rm -rf /", true), b"\x1b[200~xrm -rf /\x1b[201~".to_vec());
    }
}
```

y en `lib.rs` agregar `pub mod paste;` debajo de `pub mod size;` y `pub use paste::paste_bytes;` debajo de `pub use size::GridSize;`.

- [ ] **Paso 2: ver que falla**

Run: `cargo test -p kuntur-term paste`
Esperado: FALLA con `cannot find function paste_bytes`.

- [ ] **Paso 3: implementación** — agregar arriba de las pruebas en `paste.rs`:

```rust
/// Prepara texto pegado para enviarlo al programa.
/// - Normaliza saltos de línea a `\r` (lo que envía la tecla Enter).
/// - Con bracketed paste (DEC 2004) lo envuelve en `ESC[200~ … ESC[201~`, para que
///   shells y agentes (Claude Code) lo reciban como un solo bloque y no lo ejecuten línea a línea.
/// - Quita cualquier `ESC[201~` dentro del texto: evita que un texto malicioso "cierre" el pegado.
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        let safe = normalized.replace("\x1b[201~", "");
        format!("\x1b[200~{safe}\x1b[201~").into_bytes()
    } else {
        normalized.into_bytes()
    }
}
```

- [ ] **Paso 4: ver que pasa**

Run: `cargo test -p kuntur-term paste`
Esperado: `3 passed`

- [ ] **Paso 5: commit**

```powershell
cargo fmt --all
git add crates/kuntur-term
git commit -m "feat(term): pegado seguro con bracketed paste"
```

---

### Task 3: Teclas → bytes

**Files:**
- Create: `crates/kuntur-term/src/keys.rs`
- Modify: `crates/kuntur-term/src/lib.rs`

- [ ] **Paso 1: prueba que falla** — crear `keys.rs` con:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: Mods = Mods { ctrl: true, alt: false, shift: false };
    const ALT: Mods = Mods { ctrl: false, alt: true, shift: false };
    const SHIFT: Mods = Mods { ctrl: false, alt: false, shift: true };

    #[test]
    fn letras_y_utf8() {
        assert_eq!(encode_key(&Key::Char('a'), Mods::NONE, false), Some(b"a".to_vec()));
        assert_eq!(encode_key(&Key::Char('ñ'), Mods::NONE, false), Some("ñ".as_bytes().to_vec()));
    }

    #[test]
    fn ctrl_c_es_interrupcion() {
        assert_eq!(encode_key(&Key::Char('c'), CTRL, false), Some(vec![0x03]));
        assert_eq!(encode_key(&Key::Char('C'), CTRL, false), Some(vec![0x03]));
    }

    #[test]
    fn alt_antepone_escape() {
        assert_eq!(encode_key(&Key::Char('b'), ALT, false), Some(vec![0x1b, b'b']));
    }

    #[test]
    fn flechas_normales_y_modo_aplicacion() {
        assert_eq!(encode_key(&Key::Up, Mods::NONE, false), Some(b"\x1b[A".to_vec()));
        assert_eq!(encode_key(&Key::Up, Mods::NONE, true), Some(b"\x1bOA".to_vec()));
        assert_eq!(encode_key(&Key::Right, CTRL, false), Some(b"\x1b[1;5C".to_vec()));
    }

    #[test]
    fn teclas_especiales() {
        assert_eq!(encode_key(&Key::Enter, Mods::NONE, false), Some(b"\r".to_vec()));
        assert_eq!(encode_key(&Key::Backspace, Mods::NONE, false), Some(vec![0x7f]));
        assert_eq!(encode_key(&Key::Tab, SHIFT, false), Some(b"\x1b[Z".to_vec()));
        assert_eq!(encode_key(&Key::Delete, Mods::NONE, false), Some(b"\x1b[3~".to_vec()));
        assert_eq!(encode_key(&Key::F(1), Mods::NONE, false), Some(b"\x1bOP".to_vec()));
        assert_eq!(encode_key(&Key::F(5), Mods::NONE, false), Some(b"\x1b[15~".to_vec()));
    }
}
```

y en `lib.rs`: `pub mod keys;` y `pub use keys::{encode_key, Key, Mods};`.

- [ ] **Paso 2: ver que falla**

Run: `cargo test -p kuntur-term keys`
Esperado: FALLA con `cannot find type Mods` / `cannot find function encode_key`.

- [ ] **Paso 3: implementación** — agregar arriba de las pruebas en `keys.rs`:

```rust
/// Teclas que la UI traduce a bytes para el programa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
    F(u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Mods {
    pub const NONE: Mods = Mods { ctrl: false, alt: false, shift: false };

    /// Parámetro xterm de modificadores: 1 + shift + 2·alt + 4·ctrl.
    fn xterm_param(self) -> u8 {
        1 + self.shift as u8 + 2 * self.alt as u8 + 4 * self.ctrl as u8
    }

    fn any(self) -> bool {
        self.ctrl || self.alt || self.shift
    }
}

/// Convierte una tecla en los bytes que espera una terminal tipo xterm.
/// `app_cursor` = modo DECCKM activo (lo activan nvim, less, etc.).
pub fn encode_key(key: &Key, mods: Mods, app_cursor: bool) -> Option<Vec<u8>> {
    let csi = |final_byte: char| -> Vec<u8> {
        if mods.any() {
            format!("\x1b[1;{}{}", mods.xterm_param(), final_byte).into_bytes()
        } else if app_cursor {
            format!("\x1bO{final_byte}").into_bytes()
        } else {
            format!("\x1b[{final_byte}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if mods.any() {
            format!("\x1b[{n};{}~", mods.xterm_param()).into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };

    let bytes = match key {
        Key::Char(c) => {
            let mut out = Vec::new();
            if mods.alt {
                out.push(0x1b);
            }
            if mods.ctrl {
                let b = match c.to_ascii_lowercase() {
                    l @ 'a'..='z' => l as u8 - b'a' + 1,
                    ' ' | '2' | '@' => 0,
                    '[' | '3' => 0x1b,
                    '\\' | '4' => 0x1c,
                    ']' | '5' => 0x1d,
                    '6' | '^' => 0x1e,
                    '-' | '_' | '7' => 0x1f,
                    _ => return None,
                };
                out.push(b);
            } else {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            out
        }
        Key::Enter => {
            if mods.alt { b"\x1b\r".to_vec() } else { b"\r".to_vec() }
        }
        Key::Backspace => {
            if mods.ctrl { vec![0x08] } else if mods.alt { vec![0x1b, 0x7f] } else { vec![0x7f] }
        }
        Key::Tab => {
            if mods.shift { b"\x1b[Z".to_vec() } else { b"\t".to_vec() }
        }
        Key::Escape => vec![0x1b],
        Key::Up => csi('A'),
        Key::Down => csi('B'),
        Key::Right => csi('C'),
        Key::Left => csi('D'),
        Key::Home => csi('H'),
        Key::End => csi('F'),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::F(n @ 1..=4) => {
            let f = b"PQRS"[(*n - 1) as usize] as char;
            if mods.any() {
                format!("\x1b[1;{}{}", mods.xterm_param(), f).into_bytes()
            } else {
                format!("\x1bO{f}").into_bytes()
            }
        }
        Key::F(n @ 5..=12) => {
            let code = [15, 17, 18, 19, 20, 21, 23, 24][(*n - 5) as usize];
            tilde(code)
        }
        Key::F(_) => return None,
    };
    Some(bytes)
}
```

- [ ] **Paso 4: ver que pasa**

Run: `cargo test -p kuntur-term keys`
Esperado: `5 passed`

- [ ] **Paso 5: commit**

```powershell
cargo fmt --all
git add crates/kuntur-term
git commit -m "feat(term): codificación de teclas xterm"
```

---

### Task 4: Paleta de colores

**Files:**
- Create: `crates/kuntur-term/src/palette.rs`
- Modify: `crates/kuntur-term/src/lib.rs`

- [ ] **Paso 1: prueba que falla** — crear `palette.rs` con:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::Rgb;

    #[test]
    fn colores_nombrados_e_indices() {
        let p = Palette::dark();
        assert_eq!(p.resolve(Color::Named(NamedColor::Red)), p.ansi[1]);
        assert_eq!(p.resolve(Color::Named(NamedColor::Foreground)), p.fg);
        assert_eq!(p.resolve(Color::Indexed(9)), p.ansi[9]);
    }

    #[test]
    fn cubo_256_y_grises() {
        let p = Palette::dark();
        assert_eq!(p.resolve(Color::Indexed(196)), Rgb8 { r: 255, g: 0, b: 0 });
        assert_eq!(p.resolve(Color::Indexed(232)), Rgb8 { r: 8, g: 8, b: 8 });
    }

    #[test]
    fn truecolor_pasa_igual() {
        let p = Palette::dark();
        assert_eq!(p.resolve(Color::Spec(Rgb { r: 1, g: 2, b: 3 })), Rgb8 { r: 1, g: 2, b: 3 });
    }
}
```

y en `lib.rs`: `pub mod palette;` y `pub use palette::{Palette, Rgb8};`.

- [ ] **Paso 2: ver que falla**

Run: `cargo test -p kuntur-term palette`
Esperado: FALLA con `cannot find type Palette`.

- [ ] **Paso 3: implementación** — agregar arriba de las pruebas en `palette.rs`:

```rust
use alacritty_terminal::vte::ansi::{Color, NamedColor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    pub const fn hex(v: u32) -> Self {
        Self { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8 }
    }

    fn dim(self) -> Self {
        let f = |c: u8| (c as u16 * 2 / 3) as u8;
        Self { r: f(self.r), g: f(self.g), b: f(self.b) }
    }
}

/// Colores de la terminal. Los 16 ANSI salen de `themes/*.toml` (la Fase 2 los leerá del archivo).
#[derive(Clone, Debug)]
pub struct Palette {
    pub ansi: [Rgb8; 16],
    pub fg: Rgb8,
    pub bg: Rgb8,
    pub cursor: Rgb8,
    pub selection_bg: Rgb8,
}

impl Palette {
    /// Tema Dark · Medianoche (themes/dark.toml).
    pub fn dark() -> Self {
        let h = Rgb8::hex;
        Self {
            ansi: [
                h(0x15161e), h(0xf7768e), h(0x9ece6a), h(0xe0af68), h(0x7aa2f7), h(0xbb9af7), h(0x7dcfff), h(0xa9b1d6),
                h(0x414868), h(0xff899d), h(0x9fe044), h(0xfaba4a), h(0x8db0ff), h(0xc7a9ff), h(0xa4daff), h(0xc0caf5),
            ],
            fg: h(0xd5dcea),
            bg: h(0x0b1019),
            cursor: h(0x7aa2f7),
            selection_bg: h(0x2b3a5c),
        }
    }

    /// Convierte un color del emulador (nombrado, índice 0-255 o RGB) a RGB.
    pub fn resolve(&self, color: Color) -> Rgb8 {
        match color {
            Color::Spec(c) => Rgb8 { r: c.r, g: c.g, b: c.b },
            Color::Indexed(i) => self.indexed(i),
            Color::Named(n) => self.named(n),
        }
    }

    fn indexed(&self, i: u8) -> Rgb8 {
        match i {
            0..=15 => self.ansi[i as usize],
            16..=231 => {
                let i = i - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                Rgb8 { r: level(i / 36), g: level((i / 6) % 6), b: level(i % 6) }
            }
            232..=255 => {
                let g = 8 + (i - 232) * 10;
                Rgb8 { r: g, g, b: g }
            }
        }
    }

    fn named(&self, n: NamedColor) -> Rgb8 {
        let idx = n as usize;
        match n {
            NamedColor::Foreground | NamedColor::BrightForeground => self.fg,
            NamedColor::Background => self.bg,
            NamedColor::Cursor => self.cursor,
            NamedColor::DimForeground => self.fg.dim(),
            _ if idx < 16 => self.ansi[idx],
            // DimBlack..DimWhite: versión atenuada de los 8 básicos.
            _ => self.ansi[(idx - NamedColor::DimBlack as usize) % 8].dim(),
        }
    }
}
```

- [ ] **Paso 4: ver que pasa**

Run: `cargo test -p kuntur-term palette`
Esperado: `3 passed`

- [ ] **Paso 5: commit**

```powershell
cargo fmt --all
git add crates/kuntur-term
git commit -m "feat(term): paleta ANSI/256/truecolor (tema Dark)"
```

---

### Task 5: `Emulator`

**Files:**
- Create: `crates/kuntur-term/src/emulator.rs`
- Modify: `crates/kuntur-term/src/lib.rs`

- [ ] **Paso 1: prueba que falla** — crear `emulator.rs` con:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escribe_texto_plano() {
        let mut emu = Emulator::new(GridSize::new(20, 4));
        emu.feed(b"hola\r\nmundo");
        assert_eq!(emu.line_text(0), "hola");
        assert_eq!(emu.line_text(1), "mundo");
    }

    #[test]
    fn respeta_colores_ansi() {
        let mut emu = Emulator::new(GridSize::new(20, 2));
        emu.feed(b"\x1b[31mR\x1b[0mN");
        let grid = emu.term().grid();
        use alacritty_terminal::vte::ansi::{Color, NamedColor};
        assert_eq!(grid[Line(0)][Column(0)].fg, Color::Named(NamedColor::Red));
        assert_eq!(grid[Line(0)][Column(1)].fg, Color::Named(NamedColor::Foreground));
    }

    #[test]
    fn responde_consulta_de_posicion_del_cursor() {
        // ConPTY manda ESC[6n al arrancar y espera esta respuesta.
        let mut emu = Emulator::new(GridSize::new(20, 4));
        emu.feed(b"ab\x1b[6n");
        let respuestas: Vec<String> = emu
            .take_events()
            .into_iter()
            .filter_map(|e| if let Event::PtyWrite(s) = e { Some(s) } else { None })
            .collect();
        assert_eq!(respuestas, vec!["\x1b[1;3R".to_string()]);
    }

    #[test]
    fn selecciona_y_copia_texto() {
        let mut emu = Emulator::new(GridSize::new(20, 3));
        emu.feed(b"git status\r\nlimpio");
        emu.start_selection(SelectionKind::Simple, 0, 0, Side::Left);
        emu.update_selection(2, 0, Side::Right);
        assert_eq!(emu.selection_text().as_deref(), Some("git"));
    }

    #[test]
    fn une_lineas_cortadas_por_el_ancho() {
        let mut emu = Emulator::new(GridSize::new(5, 3));
        emu.feed(b"abcdefgh");
        emu.start_selection(SelectionKind::Line, 0, 0, Side::Left);
        emu.update_selection(4, 1, Side::Right);
        assert_eq!(emu.selection_text().as_deref(), Some("abcdefgh"));
    }

    #[test]
    fn modo_bracketed_paste() {
        let mut emu = Emulator::new(GridSize::new(10, 2));
        assert!(!emu.mode().contains(TermMode::BRACKETED_PASTE));
        emu.feed(b"\x1b[?2004h");
        assert!(emu.mode().contains(TermMode::BRACKETED_PASTE));
    }

    #[test]
    fn osc52_entrega_texto_al_portapapeles() {
        let mut emu = Emulator::new(GridSize::new(10, 2));
        // "hola" en base64 = aG9sYQ==
        emu.feed(b"\x1b]52;c;aG9sYQ==\x07");
        let copiado: Vec<String> = emu
            .take_events()
            .into_iter()
            .filter_map(|e| if let Event::ClipboardStore(_, s) = e { Some(s) } else { None })
            .collect();
        assert_eq!(copiado, vec!["hola".to_string()]);
    }
}
```

y en `lib.rs` agregar:

```rust
pub mod emulator;
pub use emulator::{Emulator, EventSink, SelectionKind};

// Tipos de alacritty que la UI necesita, sin depender de ese crate directamente.
pub use alacritty_terminal::index::Side;
pub use alacritty_terminal::term::TermMode;
```

- [ ] **Paso 2: ver que falla**

Run: `cargo test -p kuntur-term emulator`
Esperado: FALLA con `cannot find type Emulator`.

- [ ] **Paso 3: implementación** — agregar arriba de las pruebas en `emulator.rs`:

```rust
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{viewport_to_point, Config, Osc52, Term, TermMode};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

use crate::size::GridSize;

/// Recibe los eventos que emite el emulador (respuestas a la terminal, portapapeles, título…).
#[derive(Clone, Default)]
pub struct EventSink(Arc<Mutex<Vec<Event>>>);

impl EventListener for EventSink {
    fn send_event(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}

impl EventSink {
    pub fn drain(&self) -> Vec<Event> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

/// Tipo de selección con el ratón.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionKind {
    /// Arrastrar.
    Simple,
    /// Doble clic: palabra.
    Word,
    /// Triple clic: línea.
    Line,
    /// Alt + arrastrar: bloque rectangular.
    Block,
}

/// Emulador VT: estado de la grilla + parser de secuencias de escape.
pub struct Emulator {
    term: Term<EventSink>,
    parser: Processor<StdSyncHandler>,
    events: EventSink,
}

impl Emulator {
    pub fn new(size: GridSize) -> Self {
        let events = EventSink::default();
        let config = Config { osc52: Osc52::OnlyCopy, ..Config::default() };
        let term = Term::new(config, &size, events.clone());
        Self { term, parser: Processor::new(), events }
    }

    /// Procesa bytes que llegan del programa (salida del PTY).
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    pub fn resize(&mut self, size: GridSize) {
        self.term.resize(size);
    }

    pub fn take_events(&self) -> Vec<Event> {
        self.events.drain()
    }

    pub fn term(&self) -> &Term<EventSink> {
        &self.term
    }

    pub fn mode(&self) -> TermMode {
        *self.term.mode()
    }

    /// Texto de una fila visible (sin scroll), sin espacios al final.
    pub fn line_text(&self, row: usize) -> String {
        let grid = self.term.grid();
        let cols = self.term.grid().columns();
        let line = &grid[Line(row as i32)];
        let s: String = (0..cols).map(|c| line[Column(c)].c).collect();
        s.trim_end().to_string()
    }

    /// ¿Alguna fila visible contiene este texto? (útil en pruebas)
    pub fn contains_text(&self, needle: &str) -> bool {
        (0..self.term.screen_lines()).any(|r| self.line_text(r).contains(needle))
    }

    /// Desplaza el historial (positivo = hacia arriba).
    pub fn scroll(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
    }

    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    fn grid_point(&self, col: usize, row: usize) -> Point {
        viewport_to_point(self.display_offset(), Point::new(row, Column(col)))
    }

    pub fn start_selection(&mut self, kind: SelectionKind, col: usize, row: usize, side: Side) {
        let ty = match kind {
            SelectionKind::Simple => SelectionType::Simple,
            SelectionKind::Word => SelectionType::Semantic,
            SelectionKind::Line => SelectionType::Lines,
            SelectionKind::Block => SelectionType::Block,
        };
        let p = self.grid_point(col, row);
        self.term.selection = Some(Selection::new(ty, p, side));
    }

    pub fn update_selection(&mut self, col: usize, row: usize, side: Side) {
        let p = self.grid_point(col, row);
        if let Some(sel) = self.term.selection.as_mut() {
            sel.update(p, side);
        }
    }

    pub fn clear_selection(&mut self) {
        self.term.selection = None;
    }

    /// Texto seleccionado, sin códigos ANSI y uniendo las líneas cortadas solo por el ancho.
    /// Sin salto de línea final: así, al pegar un comando copiado, no se ejecuta solo.
    pub fn selection_text(&self) -> Option<String> {
        let text = self.term.selection_to_string()?;
        let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
        (!text.is_empty()).then_some(text)
    }

    pub fn has_selection(&self) -> bool {
        self.term.selection.is_some()
    }
}
```

- [ ] **Paso 4: ver que pasa**

Run: `cargo test -p kuntur-term emulator`
Esperado: `7 passed`

- [ ] **Paso 5: commit**

```powershell
cargo fmt --all
git add crates/kuntur-term
git commit -m "feat(term): emulador VT con selección, scroll, OSC 52 y respuestas al PTY"
```

---

### Task 6: `Screen` (foto para dibujar)

**Files:**
- Create: `crates/kuntur-term/src/screen.rs`
- Modify: `crates/kuntur-term/src/lib.rs`

- [ ] **Paso 1: prueba que falla** — crear `screen.rs` con:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::size::GridSize;

    #[test]
    fn captura_texto_colores_y_cursor() {
        let pal = Palette::dark();
        let mut emu = Emulator::new(GridSize::new(10, 3));
        emu.feed(b"\x1b[31mhola\x1b[0m");
        let s = Screen::capture(&emu, &pal);
        assert_eq!(s.row_text(0), "hola");
        assert_eq!(s.cell(0, 0).fg, pal.ansi[1]);
        assert_eq!(s.cursor, Some((4, 0)));
    }

    #[test]
    fn marca_la_seleccion() {
        let pal = Palette::dark();
        let mut emu = Emulator::new(GridSize::new(10, 2));
        emu.feed(b"abc");
        emu.start_selection(crate::SelectionKind::Simple, 0, 0, alacritty_terminal::index::Side::Left);
        emu.update_selection(1, 0, alacritty_terminal::index::Side::Right);
        let s = Screen::capture(&emu, &pal);
        assert!(s.cell(0, 0).selected && s.cell(1, 0).selected);
        assert!(!s.cell(2, 0).selected);
        assert_eq!(s.cell(0, 0).bg, pal.selection_bg);
    }

    #[test]
    fn video_inverso() {
        let pal = Palette::dark();
        let mut emu = Emulator::new(GridSize::new(10, 2));
        emu.feed(b"\x1b[7mX");
        let s = Screen::capture(&emu, &pal);
        assert_eq!(s.cell(0, 0).fg, pal.bg);
        assert_eq!(s.cell(0, 0).bg, pal.fg);
    }
}
```

y en `lib.rs`: `pub mod screen;` y `pub use screen::{CellView, Screen};`.

- [ ] **Paso 2: ver que falla**

Run: `cargo test -p kuntur-term screen`
Esperado: FALLA con `cannot find type Screen`.

- [ ] **Paso 3: implementación** — agregar arriba de las pruebas en `screen.rs`:

```rust
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::CursorShape;

use crate::emulator::Emulator;
use crate::palette::{Palette, Rgb8};

/// Una celda lista para dibujar (colores ya resueltos a RGB).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellView {
    pub ch: char,
    pub fg: Rgb8,
    pub bg: Rgb8,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// Ocupa dos columnas (CJK, emoji).
    pub wide: bool,
    pub selected: bool,
}

/// Foto de la parte visible de la terminal. La UI solo lee esto: no toca el emulador.
#[derive(Clone, Debug)]
pub struct Screen {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<CellView>,
    /// (columna, fila) del cursor si está visible.
    pub cursor: Option<(usize, usize)>,
    pub default_bg: Rgb8,
}

impl Screen {
    pub fn capture(emu: &Emulator, pal: &Palette) -> Screen {
        let term = emu.term();
        let cols = term.columns();
        let rows = term.screen_lines();
        let blank = CellView {
            ch: ' ', fg: pal.fg, bg: pal.bg, bold: false, italic: false, underline: false, wide: false, selected: false,
        };
        let mut cells = vec![blank; cols * rows];

        let content = term.renderable_content();
        let offset = content.display_offset as i32;
        let selection = content.selection;
        let cursor = content.cursor;

        for indexed in content.display_iter {
            let row = indexed.point.line.0 + offset;
            let col = indexed.point.column.0;
            if row < 0 || row as usize >= rows || col >= cols {
                continue;
            }
            let cell = indexed.cell;
            let flags = cell.flags;
            let mut fg = pal.resolve(cell.fg);
            let mut bg = pal.resolve(cell.bg);
            if flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            let selected = selection.is_some_and(|s| s.contains(indexed.point));
            if selected {
                bg = pal.selection_bg;
            }
            let ch = if flags.contains(Flags::WIDE_CHAR_SPACER) { ' ' } else { cell.c };
            cells[row as usize * cols + col] = CellView {
                ch,
                fg,
                bg,
                bold: flags.contains(Flags::BOLD),
                italic: flags.contains(Flags::ITALIC),
                underline: flags.intersects(Flags::ALL_UNDERLINES),
                wide: flags.contains(Flags::WIDE_CHAR),
                selected,
            };
        }

        let cursor_pos = (cursor.shape != CursorShape::Hidden)
            .then(|| (cursor.point.column.0, cursor.point.line.0 + offset))
            .filter(|&(c, r)| r >= 0 && (r as usize) < rows && c < cols)
            .map(|(c, r)| (c, r as usize));

        Screen { cols, rows, cells, cursor: cursor_pos, default_bg: pal.bg }
    }

    pub fn cell(&self, col: usize, row: usize) -> &CellView {
        &self.cells[row * self.cols + col]
    }

    pub fn row_text(&self, row: usize) -> String {
        let s: String = (0..self.cols).map(|c| self.cell(c, row).ch).collect();
        s.trim_end().to_string()
    }
}
```

- [ ] **Paso 4: ver que pasa**

Run: `cargo test -p kuntur-term screen`
Esperado: `3 passed`

- [ ] **Paso 5: commit**

```powershell
cargo fmt --all
git add crates/kuntur-term
git commit -m "feat(term): Screen, foto de la pantalla lista para dibujar"
```

---

### Task 7: `Pane` sobre ConPTY

**Files:**
- Create: `crates/kuntur-term/src/pane.rs`
- Modify: `crates/kuntur-term/src/lib.rs`

- [ ] **Paso 1: prueba que falla (ConPTY real)** — crear `pane.rs` con:

```rust
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_for(pane: &Pane, needle: &str) -> bool {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if pane.with_emulator(|e| e.contains_text(needle)) {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn ejecuta_un_programa_y_muestra_su_salida() {
        let opts = PaneOptions {
            program: "cmd.exe".into(),
            args: vec!["/c".into(), "echo hola-kuntur".into()],
            cwd: None,
            size: GridSize::new(80, 24),
        };
        let pane = Pane::spawn(opts, || {}).expect("no se pudo abrir el PTY");
        assert!(wait_for(&pane, "hola-kuntur"));
    }

    #[test]
    fn envia_teclas_a_un_shell_interactivo() {
        let opts = PaneOptions {
            program: "cmd.exe".into(),
            args: vec!["/q".into(), "/k".into(), "prompt $G".into()],
            cwd: None,
            size: GridSize::new(80, 24),
        };
        let pane = Pane::spawn(opts, || {}).unwrap();
        pane.write(b"echo teclado-ok\r").unwrap();
        assert!(wait_for(&pane, "teclado-ok"));
    }
}
```

y en `lib.rs`: `pub mod pane;` y `pub use pane::{Pane, PaneOptions};`.

- [ ] **Paso 2: ver que falla**

Run: `cargo test -p kuntur-term pane`
Esperado: FALLA con `cannot find type PaneOptions`.

- [ ] **Paso 3: implementación** — agregar arriba de las pruebas en `pane.rs`:

```rust
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use alacritty_terminal::event::Event;
use alacritty_terminal::term::TermMode;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty};

use crate::emulator::Emulator;
use crate::paste::paste_bytes;
use crate::size::GridSize;

/// Qué programa abrir en el panel.
#[derive(Clone, Debug)]
pub struct PaneOptions {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub size: GridSize,
}

impl PaneOptions {
    /// PowerShell 7 si está instalado; si no, Windows PowerShell.
    pub fn default_shell(size: GridSize) -> Self {
        let program = if find_in_path("pwsh.exe") { "pwsh.exe" } else { "powershell.exe" };
        Self { program: program.into(), args: vec!["-NoLogo".into()], cwd: None, size }
    }
}

fn find_in_path(exe: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
        .unwrap_or(false)
}

/// Un programa corriendo en un PTY (ConPTY en Windows) conectado a su emulador.
pub struct Pane {
    emulator: Arc<Mutex<Emulator>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    exited: Arc<AtomicBool>,
    title: Arc<Mutex<Option<String>>>,
    clipboard: Arc<Mutex<Option<String>>>,
}

impl Pane {
    /// Abre el programa. `on_change` se llama (desde otro hilo) cada vez que hay salida nueva.
    pub fn spawn(opts: PaneOptions, on_change: impl Fn() + Send + 'static) -> anyhow::Result<Pane> {
        let pair = native_pty_system().openpty(opts.size.into())?;

        let mut cmd = CommandBuilder::new(&opts.program);
        cmd.args(&opts.args);
        if let Some(dir) = &opts.cwd {
            cmd.cwd(dir);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "Kuntur");

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(pair.master.take_writer()?));
        let emulator = Arc::new(Mutex::new(Emulator::new(opts.size)));
        let exited = Arc::new(AtomicBool::new(false));
        let title = Arc::new(Mutex::new(None));
        let clipboard = Arc::new(Mutex::new(None));

        {
            let (emulator, writer, exited, title, clipboard) =
                (emulator.clone(), writer.clone(), exited.clone(), title.clone(), clipboard.clone());
            thread::Builder::new().name("kuntur-pty-lector".into()).spawn(move || {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    let events = {
                        let mut emu = emulator.lock().unwrap();
                        emu.feed(&buf[..n]);
                        emu.take_events()
                    };
                    for event in events {
                        match event {
                            // Respuestas que el programa pidió (posición del cursor, atributos…).
                            Event::PtyWrite(s) => {
                                let mut w = writer.lock().unwrap();
                                let _ = w.write_all(s.as_bytes());
                                let _ = w.flush();
                            }
                            Event::ClipboardStore(_, text) => *clipboard.lock().unwrap() = Some(text),
                            Event::Title(t) => *title.lock().unwrap() = Some(t),
                            _ => {}
                        }
                    }
                    on_change();
                }
                exited.store(true, Ordering::SeqCst);
                on_change();
            })?;
        }

        Ok(Pane {
            emulator,
            writer,
            master: Mutex::new(pair.master),
            child: Mutex::new(child),
            exited,
            title,
            clipboard,
        })
    }

    /// Envía bytes al programa (teclas).
    pub fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        let mut w = self.writer.lock().unwrap();
        w.write_all(bytes)?;
        w.flush()
    }

    /// Pega texto respetando el modo bracketed paste del programa.
    pub fn paste(&self, text: &str) -> std::io::Result<()> {
        let bracketed = self.emulator.lock().unwrap().mode().contains(TermMode::BRACKETED_PASTE);
        self.write(&paste_bytes(text, bracketed))
    }

    pub fn resize(&self, size: GridSize) -> anyhow::Result<()> {
        self.master.lock().unwrap().resize(size.into())?;
        self.emulator.lock().unwrap().resize(size);
        Ok(())
    }

    /// Acceso al emulador (selección, scroll, captura de pantalla).
    pub fn with_emulator<R>(&self, f: impl FnOnce(&mut Emulator) -> R) -> R {
        f(&mut self.emulator.lock().unwrap())
    }

    /// Texto que un programa pidió copiar con OSC 52 (la UI lo pasa al portapapeles de Windows).
    pub fn take_clipboard_request(&self) -> Option<String> {
        self.clipboard.lock().unwrap().take()
    }

    pub fn title(&self) -> Option<String> {
        self.title.lock().unwrap().clone()
    }

    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst) || self.child.lock().unwrap().try_wait().ok().flatten().is_some()
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        let _ = self.child.lock().unwrap().kill();
    }
}
```

- [ ] **Paso 4: ver que pasa**

Run: `cargo test -p kuntur-term pane`
Esperado: `2 passed` (abre `cmd.exe` real por ConPTY).

- [ ] **Paso 5: suite completa y clippy**

Run: `cargo test -p kuntur-term` → Esperado: `25 passed`
Run: `cargo clippy -p kuntur-term --all-targets -- -D warnings` → Esperado: sin advertencias.

- [ ] **Paso 6: commit**

```powershell
cargo fmt --all
git add crates/kuntur-term
git commit -m "feat(term): Pane sobre ConPTY con hilo lector y respuestas al programa"
```

---

### Task 8: Ventana y vista de terminal (GPUI)

**Files:**
- Modify: `crates/kuntur-app/Cargo.toml`
- Replace: `crates/kuntur-app/src/main.rs`
- Create: `crates/kuntur-app/src/terminal_view.rs`

La UI no tiene pruebas unitarias en F1: se verifica con la prueba de humo (Task 9) y la aceptación manual (Task 11).

- [ ] **Paso 1: dependencias** — reemplazar `crates/kuntur-app/Cargo.toml` por:

```toml
[package]
name = "kuntur-app"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[[bin]]
name = "kuntur"
path = "src/main.rs"

[dependencies]
kuntur-term = { path = "../kuntur-term" }
gpui-kit = { workspace = true }
smol = { workspace = true }
```

- [ ] **Paso 2: `main.rs`** — reemplazar `crates/kuntur-app/src/main.rs` por:

```rust
//! Kuntur: terminal para Windows pensada para agentes de IA.
// En release no abre una consola negra extra.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod terminal_view;

use gpui_kit::*;
use terminal_view::TerminalView;

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions { title: Some("Kuntur".into()), ..Default::default() }),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| TerminalView::new(window, cx)))
            .expect("no se pudo abrir la ventana");
    });
}
```

- [ ] **Paso 3: `terminal_view.rs`** — crear `crates/kuntur-app/src/terminal_view.rs`:

```rust
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::*;
use kuntur_term::{
    encode_key, Emulator, GridSize, Key, Mods, Palette, Pane, PaneOptions, Rgb8, Screen, SelectionKind, Side, TermMode,
};

const FONT_FAMILY: &str = "Cascadia Mono";
const FONT_SIZE: f32 = 14.0;
const LINE_HEIGHT_FACTOR: f32 = 1.3;

#[derive(Clone, Copy)]
struct Metrics {
    cell_w: Pixels,
    line_h: Pixels,
}

pub struct TerminalView {
    pane: Arc<Pane>,
    palette: Palette,
    focus: FocusHandle,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    last_size: Rc<Cell<GridSize>>,
    selecting: bool,
    _pump: Task<()>,
}

fn to_color(c: Rgb8) -> Hsla {
    rgb(((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32).into()
}

impl TerminalView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (tx, rx) = smol::channel::bounded::<()>(1);
        let size = GridSize::new(100, 30);
        let pane = Pane::spawn(PaneOptions::default_shell(size), move || {
            let _ = tx.try_send(());
        })
        .expect("no se pudo abrir el shell");

        // El hilo del PTY avisa por el canal; aquí se redibuja en el hilo de la UI.
        let pump = cx.spawn(async move |this, cx| {
            while rx.recv().await.is_ok() {
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });

        let focus = cx.focus_handle();
        window.focus(&focus, cx);

        let pane = Arc::new(pane);
        // Prueba de humo: KUNTUR_SMOKE_INPUT escribe comandos en el shell desde dentro de la app
        // (sin simular teclado del sistema). "\n" separa comandos.
        if let Ok(input) = std::env::var("KUNTUR_SMOKE_INPUT") {
            let pane = pane.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(2500));
                for cmd in input.split("\\n") {
                    let _ = pane.write(format!("{cmd}\r").as_bytes());
                    std::thread::sleep(std::time::Duration::from_millis(600));
                }
            });
        }

        Self {
            pane,
            palette: Palette::dark(),
            focus,
            bounds: Rc::new(Cell::new(Bounds::default())),
            last_size: Rc::new(Cell::new(size)),
            selecting: false,
            _pump: pump,
        }
    }

    fn metrics(window: &mut Window) -> Metrics {
        let font_size = px(FONT_SIZE);
        let text = window.text_system();
        let font_id = text.resolve_font(&font(FONT_FAMILY));
        let cell_w = text.advance(font_id, font_size, 'm').map(|s| s.width).unwrap_or(px(8.0));
        Metrics { cell_w, line_h: (font_size * LINE_HEIGHT_FACTOR).round() }
    }

    fn cell_at(&self, pos: Point<Pixels>, m: Metrics) -> (usize, usize, Side) {
        let b = self.bounds.get();
        let x = f32::from(pos.x - b.origin.x).max(0.0);
        let y = f32::from(pos.y - b.origin.y).max(0.0);
        let cw = f32::from(m.cell_w);
        let col = (x / cw) as usize;
        let row = (y / f32::from(m.line_h)) as usize;
        let side = if x - col as f32 * cw < cw / 2.0 { Side::Left } else { Side::Right };
        let size = self.last_size.get();
        (col.min(size.cols as usize - 1), row.min(size.rows as usize - 1), side)
    }

    fn copy_selection(&self, cx: &mut Context<Self>) -> bool {
        match self.pane.with_emulator(|e| e.selection_text()) {
            Some(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.pane.with_emulator(|e| e.clear_selection());
                cx.notify();
                true
            }
            None => false,
        }
    }

    fn paste(&self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
            let _ = self.pane.paste(&text);
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = ks.modifiers;
        let key = ks.key.as_str();

        // Atajos de copiar/pegar (como Windows Terminal).
        match (m.control, m.shift, key) {
            (true, true, "c") => {
                self.copy_selection(cx);
                return;
            }
            (true, true, "v") | (true, false, "v") => {
                self.paste(cx);
                return;
            }
            (true, false, "c") if self.copy_selection(cx) => return,
            (false, true, "insert") => {
                self.paste(cx);
                return;
            }
            (false, true, "pageup") => {
                self.pane.with_emulator(|e| e.scroll(10));
                cx.notify();
                return;
            }
            (false, true, "pagedown") => {
                self.pane.with_emulator(|e| e.scroll(-10));
                cx.notify();
                return;
            }
            _ => {}
        }

        let mods = Mods { ctrl: m.control, alt: m.alt, shift: m.shift };
        let k = match key {
            "enter" => Key::Enter,
            "backspace" => Key::Backspace,
            "tab" => Key::Tab,
            "escape" => Key::Escape,
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "home" => Key::Home,
            "end" => Key::End,
            "pageup" => Key::PageUp,
            "pagedown" => Key::PageDown,
            "delete" => Key::Delete,
            "insert" => Key::Insert,
            "space" => Key::Char(' '),
            f if f.len() >= 2 && f.starts_with('f') && f[1..].parse::<u8>().is_ok() => Key::F(f[1..].parse().unwrap()),
            _ => {
                // Texto: usar el carácter producido por el teclado (respeta distribución latinoamericana/española).
                let ch = if m.control || m.alt {
                    key.chars().next()
                } else {
                    ks.key_char.as_deref().and_then(|s| s.chars().next())
                };
                match ch {
                    Some(c) => Key::Char(c),
                    None => return,
                }
            }
        };
        let app_cursor = self.pane.with_emulator(|e| e.mode().contains(TermMode::APP_CURSOR));
        if let Some(bytes) = encode_key(&k, mods, app_cursor) {
            self.pane.with_emulator(|e: &mut Emulator| {
                e.clear_selection();
                if e.display_offset() > 0 {
                    e.scroll(-(e.display_offset() as i32));
                }
            });
            let _ = self.pane.write(&bytes);
        }
        cx.stop_propagation();
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let m = Self::metrics(window);
        let (col, row, side) = self.cell_at(ev.position, m);
        let kind = match (ev.click_count, ev.modifiers.alt) {
            (_, true) => SelectionKind::Block,
            (2, _) => SelectionKind::Word,
            (n, _) if n >= 3 => SelectionKind::Line,
            _ => SelectionKind::Simple,
        };
        self.pane.with_emulator(|e| e.start_selection(kind, col, row, side));
        self.selecting = true;
        cx.notify();
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting || ev.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let m = Self::metrics(window);
        let (col, row, side) = self.cell_at(ev.position, m);
        self.pane.with_emulator(|e| e.update_selection(col, row, side));
        cx.notify();
    }

    fn on_mouse_up(&mut self, _ev: &MouseUpEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        self.selecting = false;
        // Un clic sin arrastre no deja selección vacía.
        if self.pane.with_emulator(|e| e.selection_text().is_none()) {
            self.pane.with_emulator(|e| e.clear_selection());
        }
    }

    /// Clic derecho: copia si hay selección; si no, pega (igual que Windows Terminal).
    fn on_right_click(&mut self, _ev: &MouseDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.copy_selection(cx) {
            self.paste(cx);
        }
    }

    fn on_wheel(&mut self, ev: &ScrollEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = Self::metrics(window);
        let dy = f32::from(ev.delta.pixel_delta(m.line_h).y);
        let lines = (dy / f32::from(m.line_h)).round() as i32;
        if lines != 0 {
            self.pane.with_emulator(|e| e.scroll(lines));
            cx.notify();
        }
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // OSC 52: un programa (por ejemplo Claude Code por SSH) pidió copiar texto.
        if let Some(text) = self.pane.take_clipboard_request() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        let metrics = Self::metrics(window);
        let screen = self.pane.with_emulator(|e| Screen::capture(e, &self.palette));
        let bg = to_color(self.palette.bg);
        let cursor_color = to_color(self.palette.cursor);
        let (pane, bounds_cell, size_cell) = (self.pane.clone(), self.bounds.clone(), self.last_size.clone());

        div()
            .size_full()
            .bg(bg)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_right_click))
            .on_scroll(cx.listener(Self::on_wheel))
            .child(
                canvas(
                    move |bounds, _window, _cx| {
                        bounds_cell.set(bounds);
                        let size = GridSize::from_pixels(
                            f32::from(bounds.size.width),
                            f32::from(bounds.size.height),
                            f32::from(metrics.cell_w),
                            f32::from(metrics.line_h),
                        );
                        if size != size_cell.get() {
                            size_cell.set(size);
                            let _ = pane.resize(size);
                        }
                    },
                    move |bounds, _, window, cx| paint_screen(&screen, bounds, metrics, cursor_color, window, cx),
                )
                .size_full(),
            )
    }
}

fn paint_screen(screen: &Screen, bounds: Bounds<Pixels>, m: Metrics, cursor: Hsla, window: &mut Window, cx: &mut App) {
    let font_size = px(FONT_SIZE);
    for row in 0..screen.rows {
        let y = bounds.origin.y + m.line_h * row as f32;
        // 1) Fondos: un rectángulo por cada tramo del mismo color.
        let mut col = 0;
        while col < screen.cols {
            let bg = screen.cell(col, row).bg;
            let start = col;
            while col < screen.cols && screen.cell(col, row).bg == bg {
                col += 1;
            }
            if bg != screen.default_bg {
                let origin = point(bounds.origin.x + m.cell_w * start as f32, y);
                window.paint_quad(fill(Bounds::new(origin, size(m.cell_w * (col - start) as f32, m.line_h)), to_color(bg)));
            }
        }
        // 2) Texto: una línea por fila con un "run" por tramo de mismo estilo; ancho fijo por celda.
        let mut text = String::with_capacity(screen.cols);
        let mut runs: Vec<TextRun> = Vec::new();
        for c in 0..screen.cols {
            let cell = screen.cell(c, row);
            let ch = if cell.ch == '\0' { ' ' } else { cell.ch };
            let before = text.len();
            text.push(ch);
            let len = text.len() - before;
            let mut f = font(FONT_FAMILY);
            if cell.bold {
                f.weight = FontWeight::BOLD;
            }
            if cell.italic {
                f.style = FontStyle::Italic;
            }
            let color = to_color(cell.fg);
            let underline = cell.underline.then(|| UnderlineStyle { thickness: px(1.0), color: Some(color), wavy: false });
            match runs.last_mut() {
                Some(r) if r.font == f && r.color == color && r.underline == underline => r.len += len,
                _ => runs.push(TextRun { len, font: f, color, background_color: None, underline, strikethrough: None }),
            }
        }
        if text.trim_end().is_empty() {
            continue;
        }
        let line = window.text_system().shape_line(text.into(), font_size, &runs, Some(m.cell_w));
        let _ = line.paint(point(bounds.origin.x, y), m.line_h, TextAlign::Left, None, window, cx);
    }
    // 3) Cursor: bloque semitransparente.
    if let Some((c, r)) = screen.cursor {
        let origin = point(bounds.origin.x + m.cell_w * c as f32, bounds.origin.y + m.line_h * r as f32);
        window.paint_quad(fill(Bounds::new(origin, size(m.cell_w, m.line_h)), cursor.opacity(0.55)));
    }
}
```

- [ ] **Paso 4: compilar**

Run: `cargo build -p kuntur-app --release`
Esperado: `Finished` sin advertencias (la primera vez tarda ~2 min por GPUI).

- [ ] **Paso 5: abrir y mirar**

Run: `cargo run -p kuntur-app --release`
Esperado: ventana "Kuntur" con el prompt de PowerShell y el cursor; escribir `dir` + Enter muestra el listado.

- [ ] **Paso 6: commit**

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/kuntur-app Cargo.lock
git commit -m "feat(app): ventana GPUI con terminal real, selección y portapapeles"
```

---

### Task 9: Prueba de humo automática

**Files:**
- Create: `scripts/smoke.ps1`

- [ ] **Paso 1: crear el script**

```powershell
<#
.SYNOPSIS
  Prueba de humo de Kuntur: compila, abre la app, escribe comandos DESDE DENTRO (variable
  KUNTUR_SMOKE_INPUT, nunca teclado simulado del sistema), captura solo su ventana y mide RAM.
.EXAMPLE
  .\scripts\smoke.ps1 -Commands 'Write-Host rojo -ForegroundColor Red','dir'
#>
param(
    [string[]]$Commands = @('Write-Host rojo -ForegroundColor Red; Write-Host verde -ForegroundColor Green', 'Get-ChildItem | Select-Object -First 5'),
    [string]$Out = (Join-Path $PSScriptRoot '..\target\smoke.png'),
    [int]$MaxRamMB = 80,
    [int]$WaitSec = 7
)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $root
try { cargo build -p kuntur-app --release -q } finally { Pop-Location }
$exe = Join-Path $root 'target\release\kuntur.exe'

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class KunturSmoke {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[KunturSmoke]::SetProcessDPIAware() | Out-Null

# NO_COLOR heredado (p. ej. de herramientas de IA) hace que PowerShell quite los colores.
$savedNoColor = $env:NO_COLOR; Remove-Item Env:NO_COLOR -ErrorAction SilentlyContinue
$env:KUNTUR_SMOKE_INPUT = ($Commands -join '\n')
$p = Start-Process $exe -PassThru
Remove-Item Env:KUNTUR_SMOKE_INPUT
if ($savedNoColor) { $env:NO_COLOR = $savedNoColor }

$deadline = (Get-Date).AddSeconds(20)
while ((Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 300; $p.Refresh(); if ($p.MainWindowHandle -ne 0) { break } }
if ($p.MainWindowHandle -eq 0) { Stop-Process -Id $p.Id -Force; throw 'Kuntur no abrió ventana' }
Start-Sleep -Seconds ($WaitSec + $Commands.Count)

$r = New-Object KunturSmoke+RECT; [KunturSmoke]::GetClientRect($p.MainWindowHandle, [ref]$r) | Out-Null
$bmp = New-Object System.Drawing.Bitmap ([Math]::Max(1, $r.R)), ([Math]::Max(1, $r.B))
$g = [System.Drawing.Graphics]::FromImage($bmp); $hdc = $g.GetHdc()
[KunturSmoke]::PrintWindow($p.MainWindowHandle, $hdc, 3) | Out-Null
$g.ReleaseHdc($hdc); $g.Dispose()
New-Item -ItemType Directory -Force -Path (Split-Path $Out) | Out-Null
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()

$p.Refresh(); $ram = [Math]::Round($p.WorkingSet64 / 1MB)
Stop-Process -Id $p.Id -Force
"Captura: $Out"
"RAM de Kuntur: $ram MB (máximo $MaxRamMB MB)"
if ($ram -gt $MaxRamMB) { throw "RAM por encima del objetivo: $ram MB" }
'SMOKE OK'
```

- [ ] **Paso 2: ejecutarla**

Run: `.\scripts\smoke.ps1`
Esperado: `RAM de Kuntur: ~57–71 MB (máximo 80 MB)` y `SMOKE OK`. Abrir `target\smoke.png`: se ven "rojo" en rojo, "verde" en verde y el listado.

- [ ] **Paso 3: commit**

```powershell
git add scripts/smoke.ps1
git commit -m "test: prueba de humo con captura de ventana y presupuesto de RAM"
```

---

### Task 10: Integración continua (GitHub Actions)

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Paso 1: crear el workflow**

```yaml
name: CI

on:
  push:
    branches: [main, "fase/**"]
  pull_request:

jobs:
  windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - name: Formato
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: Pruebas (incluye ConPTY real)
        run: cargo test --workspace
```

Nota: en repositorios privados, los minutos de Windows cuentan doble en el plan gratuito de GitHub.

- [ ] **Paso 2: commit y subir la rama**

```powershell
git add .github/workflows/ci.yml
git commit -m "ci: fmt, clippy y pruebas en Windows"
git push -u origin fase/f1-nucleo
```

- [ ] **Paso 3: verificar en GitHub**

Run: `& 'C:\Program Files\GitHub CLI\gh.exe' run list --repo jrsprescott/kuntur --limit 1` (con `$env:GH_TOKEN = (gh auth token --user jrsprescott)` en esa misma consola)
Esperado: el último run en `completed success`.

---

### Task 11: Aceptación manual y cierre de la fase

**Files:**
- Create: `docs/plans/f1-resultados.md`

- [ ] **Paso 1: abrir Kuntur** — `cargo run -p kuntur-app --release`

- [ ] **Paso 2: recorrer la lista y anotar ✅/❌** — crear `docs/plans/f1-resultados.md`:

```markdown
# F1 · Resultados de aceptación (fecha: AAAA-MM-DD)

| # | Prueba | Esperado | Resultado |
|---|---|---|---|
| 1 | `claude` | Arranca y se dibuja sin basura; se puede escribir y salir con /exit | |
| 2 | `codex` | Arranca y se dibuja bien | |
| 3 | `opencode` y `qwen` | Arrancan y se dibujan bien | |
| 4 | `git log` (paginador) | Flechas y `q` funcionan | |
| 5 | Seleccionar con el mouse mientras corre `ping -t 127.0.0.1` | La selección no se pierde | |
| 6 | Ctrl+C con selección / sin selección | Copia / interrumpe el ping | |
| 7 | Ctrl+Shift+C, Ctrl+V, Shift+Insert, clic derecho | Copian y pegan | |
| 8 | Pegar 3 líneas dentro de `claude` | Llegan como un solo bloque (no se envían solas) | |
| 9 | Doble clic / triple clic / Alt+arrastrar | Palabra / línea / bloque | |
| 10 | Rueda del mouse y Shift+RePág | Desplaza el historial; al escribir vuelve abajo | |
| 11 | Redimensionar la ventana | `$Host.UI.RawUI.WindowSize` cambia y el texto se reacomoda | |
| 12 | `ñ á é ¿ ¡ @ # \` con teclado latinoamericano | Se escriben bien | |
| 13 | RAM en el Administrador de tareas con `claude` abierto | < 80 MB (sin contar los procesos hijos) | |
```

- [ ] **Paso 3: registrar los fallos** — cada ❌ se convierte en una tarea nueva al final de este plan (con su prueba) antes de cerrar la fase.

- [ ] **Paso 4: cerrar la fase**

```powershell
git add docs/plans/f1-resultados.md
git commit -m "docs(f1): resultados de aceptación"
git push
git switch main
git merge --no-ff fase/f1-nucleo -m "Fase 1: núcleo de terminal"
git push
```

---

### Task 12 (surgida en la aceptación): el PATH del hijo no debe duplicarse ✅

**Problema encontrado:** `claude`, `node`, `codex` no arrancaban dentro de Kuntur ("No hay suficientes recursos de memoria disponibles"). Causa: `portable_pty::CommandBuilder::new` relee el entorno del registro y **concatena el PATH del sistema y el del usuario**; con PATH grandes el resultado supera el límite de 32.767 caracteres de Windows y ningún programa externo puede iniciarse desde el shell.

**Files:** Modify `crates/kuntur-term/src/pane.rs`

- [x] **Prueba (falla):** `el_hijo_hereda_el_path_sin_duplicarlo` — lanza `powershell.exe -NoProfile -Command "'PATHLEN=' + $env:Path.Length"` en un `Pane` y compara con el largo UTF-16 del PATH de Kuntur. Antes del arreglo: PowerShell ni siquiera respondía.
- [x] **Arreglo:** en `Pane::spawn`, justo después de `CommandBuilder::new`:

```rust
        // portable-pty arma el entorno releyendo el registro y SUMA el PATH del sistema y del usuario.
        // Con PATH grandes supera los 32.767 caracteres y Windows no deja lanzar procesos desde el
        // shell ("no hay suficientes recursos de memoria"). Usamos exactamente el entorno de Kuntur.
        cmd.env_clear();
        for (key, value) in std::env::vars_os() {
            cmd.env(key, value);
        }
```

- [x] **Pasa:** 26 pruebas OK. `claude`, `codex`, `opencode` y `qwen` se dibujan correctamente dentro de Kuntur (capturas con `scripts/smoke.ps1`).
- [x] **Script de humo:** además de `NO_COLOR`, ahora quita las variables `CLAUDE*` heredadas cuando la prueba corre dentro de Claude Code (si no, `claude` se niega a abrir una sesión anidada).

---

## Fuera del alcance de F1 (van en fases siguientes)
- Reporte de mouse a los programas (modos 1000/1006): selección con mouse dentro de `nvim`/`htop` → F2.
- Composición IME (teclados asiáticos) y ligaduras → F2.
- Fuente, tamaño y tema configurables (hoy: Cascadia Mono 14, tema Dark) → F2.
- Menú contextual propio (hoy el clic derecho copia o pega, como Windows Terminal) → F2.
- Varias sesiones, riel lateral, barra de título propia → F2.

## Autorrevisión
- **Cobertura de la especificación (PLAN.md F1):** PTY + VT ✅ (Tasks 5–7), dibujo GPUI ✅ (8), scrollback ✅ (5, 8), selección y copiar/pegar ✅ (5, 8), bracketed paste ✅ (2, 7), OSC 52 ✅ (5, 7, 8), redimensión ✅ (7, 8), truecolor ✅ (4, 6), RAM < 80 MB ✅ (9), CI ✅ (10), aceptación con `claude`/`codex`/`nvim`/`git log` ✅ (11; `nvim` no está instalado, se usa el paginador de `git log`).
- **Sin marcadores pendientes:** cada paso de código trae el código completo; cada comando trae su resultado esperado.
- **Consistencia de nombres:** `GridSize::new/from_pixels`, `paste_bytes`, `encode_key/Key/Mods`, `Palette::dark/resolve`, `Emulator::{feed, resize, take_events, term, mode, line_text, contains_text, scroll, display_offset, start_selection, update_selection, clear_selection, selection_text}`, `Screen::{capture, cell, row_text}`, `Pane::{spawn, write, paste, resize, with_emulator, take_clipboard_request, title, has_exited}`: iguales en definición y uso.
