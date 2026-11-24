mod blocks;
pub(crate) mod ghost;
mod image_bar;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::*;
use kuntur_term::{
    Emulator, GridSize, Key, Mods, MouseAction, MouseButtonKind, MouseMode, MouseReport, MouseReporting, Palette, Pane,
    PaneOptions, Rgb8, Screen, SelectionKind, Side, TermMode, encode_key, encode_mouse,
};

mod links;
pub use image_bar::PasteEvent;
pub use links::LinkEvent;

const LINE_HEIGHT_FACTOR: f32 = 1.3;
/// Tamaño hasta el primer dibujo (después se ajusta al área real).
pub const INITIAL_SIZE: GridSize = GridSize { cols: 100, rows: 30 };

/// `on_change` de un panel: avisa a la vista (desde el hilo lector) que hay salida nueva.
pub fn notifier(tx: smol::channel::Sender<()>) -> impl Fn() + Send + 'static {
    move || {
        let _ = tx.try_send(());
    }
}
/// Contraste mínimo del texto contra su fondo (WCAG 3:1): ningún color de programa queda invisible.
const MIN_CONTRAST: f32 = 3.0;
/// Opacidad del texto fantasma del autocompletado (sobre el color del texto).
const GHOST_OPACITY: f32 = 0.42;

#[derive(Clone, Copy)]
struct Metrics {
    cell_w: Pixels,
    line_h: Pixels,
}

/// Avisos de la terminal a quien la contiene (el `Workspace`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalEvent {
    /// El programa cambió el título (OSC 0/2).
    TitleChanged,
    /// El programa terminó (por ejemplo, `exit`).
    Exited,
}

pub struct TerminalView {
    pane: Arc<Pane>,
    palette: Palette,
    /// Fondo del área (puede tener alfa para dejar ver el degradado de la ventana).
    background: Hsla,
    font_family: SharedString,
    font_size: f32,
    focus: FocusHandle,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    last_size: Rc<Cell<GridSize>>,
    selecting: bool,
    /// Botón apretado cuyo clic se envió al programa (modo mouse de nvim, htop, opencode…).
    mouse_button: Option<MouseButtonKind>,
    /// Última celda informada al programa: el movimiento solo se envía al cambiar de celda.
    mouse_cell: Option<(usize, usize)>,
    title: Option<String>,
    exited: bool,
    /// F8: enlace subrayado con Ctrl, última posición del mouse y modo de humo "todos".
    hover_link: Option<links::HoverLink>,
    last_mouse: Option<Point<Pixels>>,
    show_all_links: bool,
    /// Autocompletado en gris (ver `ghost.rs`).
    ghost: ghost::Ghost,
    /// Barra del agente con las imágenes pegadas (ajustes v0.2, ver `image_bar.rs`).
    image_bar: image_bar::ImageBar,
    _pump: Task<()>,
}

fn to_color(c: Rgb8) -> Hsla {
    rgb(((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32).into()
}

impl EventEmitter<TerminalEvent> for TerminalView {}
impl EventEmitter<PasteEvent> for TerminalView {}

impl TerminalView {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        mut opts: PaneOptions,
        palette: Palette,
        font_family: SharedString,
        font_size: f32,
    ) -> Self {
        let size = INITIAL_SIZE;
        opts.size = size;
        let make = move |tx: smol::channel::Sender<()>| {
            // Si el shell guardado ya no existe, se abre el predeterminado en vez de fallar.
            Pane::spawn(opts, notifier(tx.clone()))
                .or_else(|_| Pane::spawn(PaneOptions::default_shell(size), notifier(tx)))
                .expect("no se pudo abrir el shell")
        };
        Self::with_pane(window, cx, make, palette, font_family, font_size)
    }

    /// Vista sobre un panel ya armado (por ejemplo, una terminal remota por SSH). `make` recibe el
    /// canal de aviso: [`notifier`] lo convierte en el `on_change` del panel.
    pub fn with_pane(
        window: &mut Window,
        cx: &mut Context<Self>,
        make: impl FnOnce(smol::channel::Sender<()>) -> Pane,
        palette: Palette,
        font_family: SharedString,
        font_size: f32,
    ) -> Self {
        let (tx, rx) = smol::channel::bounded::<()>(1);
        let size = INITIAL_SIZE;
        let pane = make(tx);

        // El hilo del PTY avisa por el canal; aquí se redibuja en el hilo de la UI.
        let pump = cx.spawn(async move |this, cx| {
            while rx.recv().await.is_ok() {
                if this.update(cx, |view, cx| view.on_output(cx)).is_err() {
                    break;
                }
            }
        });

        let focus = cx.focus_handle();
        window.focus(&focus, cx);

        Self {
            pane: Arc::new(pane),
            background: to_color(palette.bg),
            palette,
            font_family,
            font_size,
            focus,
            bounds: Rc::new(Cell::new(Bounds::default())),
            last_size: Rc::new(Cell::new(size)),
            selecting: false,
            mouse_button: None,
            mouse_cell: None,
            title: None,
            exited: false,
            hover_link: None,
            last_mouse: None,
            show_all_links: false,
            ghost: ghost::Ghost::default(),
            image_bar: image_bar::ImageBar::default(),
            _pump: pump,
        }
    }

    /// Terminal remota (F6): sin autocompletado local (ver `ghost.rs`).
    pub fn set_remote(&mut self) {
        self.ghost.set_remote();
    }

    /// Hubo salida nueva del programa: redibujar y avisar si cambió el título o terminó.
    fn on_output(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        self.ghost.refresh(&self.pane);
        let title = self.pane.title();
        if title != self.title {
            self.title = title;
            cx.emit(TerminalEvent::TitleChanged);
        }
        if !self.exited && self.pane.has_exited() {
            self.exited = true;
            cx.emit(TerminalEvent::Exited);
        }
    }

    /// Prueba de humo: escribe comandos en el shell desde dentro de la app (sin simular teclado
    /// del sistema). "\n" (literal) separa comandos.
    pub fn send_smoke_input(&self, input: String) {
        let pane = self.pane.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            for cmd in input.split("\\n") {
                // Como un Enter desde el teclado: el comando queda como bloque (OSC 133).
                pane.mark_command_start();
                let _ = pane.write(format!("{cmd}\r").as_bytes());
                std::thread::sleep(std::time::Duration::from_millis(600));
            }
            ghost::smoke_type(&pane);
        });
    }

    /// Escribe una línea en el shell y Enter, como si se tecleara (abrir un agente por pedido del
    /// usuario). `delay` deja arrancar al shell de una terminal recién abierta.
    pub fn send_line(&self, line: String, delay: std::time::Duration) {
        let pane = self.pane.clone();
        std::thread::spawn(move || {
            if !delay.is_zero() {
                std::thread::sleep(delay);
            }
            let _ = pane.write(format!("{line}\r").as_bytes());
        });
    }

    /// Pega texto sin Enter (respeta bracketed paste), tras `delay` si la terminal recién se abre.
    /// "Enviar a terminal" (F5b): el comando queda escrito para revisarlo; nunca se ejecuta solo.
    pub fn paste_text(&self, text: String, delay: std::time::Duration) {
        let pane = self.pane.clone();
        std::thread::spawn(move || {
            if !delay.is_zero() {
                std::thread::sleep(delay);
            }
            let _ = pane.paste(&text);
        });
    }

    /// Escribe bytes tal cual (respuesta a un pedido de permiso, por clic del usuario).
    pub fn write_bytes(&self, bytes: &[u8]) {
        let _ = self.pane.write(bytes);
    }

    /// Texto seleccionado con el mouse, si hay.
    pub fn selection_text(&self) -> Option<String> {
        self.pane.with_emulator(|e| e.selection_text())
    }

    /// Último estado de agente informado por OSC 7501 (se entrega una vez).
    pub fn take_agent_signal(&self) -> Option<kuntur_term::AgentSignal> {
        self.pane.take_agent_signal()
    }

    pub fn set_palette(&mut self, palette: Palette, background: Hsla, cx: &mut Context<Self>) {
        self.palette = palette;
        self.background = background;
        cx.notify();
    }

    pub fn set_font(&mut self, family: SharedString, size: f32, cx: &mut Context<Self>) {
        self.font_family = family;
        self.font_size = size;
        cx.notify();
    }

    /// Título que puso el programa (OSC 0/2), si puso alguno.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// PID del shell de la sesión.
    pub fn pid(&self) -> Option<u32> {
        self.pane.pid()
    }

    /// Carpeta actual informada por el shell (OSC 7 / OSC 9;9).
    pub fn cwd(&self) -> Option<std::path::PathBuf> {
        self.pane.cwd()
    }

    /// Tiempo desde la última salida del programa.
    pub fn last_output_age(&self) -> std::time::Duration {
        self.pane.last_output_age()
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus, cx);
    }

    /// ¿Tiene el teclado?
    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    fn metrics(&self, window: &mut Window) -> Metrics {
        let font_size = px(self.font_size);
        let text = window.text_system();
        let font_id = text.resolve_font(&font(self.font_family.clone()));
        let cell_w = text
            .advance(font_id, font_size, 'm')
            .map(|s| s.width)
            .unwrap_or(px(8.0));
        Metrics {
            cell_w,
            line_h: (font_size * LINE_HEIGHT_FACTOR).round(),
        }
    }
    fn cell_at(&self, pos: Point<Pixels>, m: Metrics) -> (usize, usize, Side) {
        let b = self.bounds.get();
        let x = f32::from(pos.x - b.origin.x).max(0.0);
        let y = f32::from(pos.y - b.origin.y).max(0.0);
        let cw = f32::from(m.cell_w);
        let col = (x / cw) as usize;
        let row = (y / f32::from(m.line_h)) as usize;
        let side = if x - col as f32 * cw < cw / 2.0 {
            Side::Left
        } else {
            Side::Right
        };
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

    /// Ctrl+V: texto como siempre; una imagen o archivos copiados van al `Workspace` (ajustes v0.2).
    fn paste(&self, cx: &mut Context<Self>) {
        self.paste_or_request(cx);
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
            // Mac: Cmd+V también pega (ajustes v0.2).
            (false, _, "v") if cfg!(target_os = "macos") && m.platform => {
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

        // Autocompletado: Tab/→ aceptan la sugerencia en gris, Esc la oculta, Enter la registra.
        if !m.control && !m.alt && !m.shift && self.ghost.on_key(key, &self.pane) == ghost::KeyOutcome::Handled {
            cx.notify();
            cx.stop_propagation();
            return;
        }

        let mods = Mods {
            ctrl: m.control,
            alt: m.alt,
            shift: m.shift,
        };
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

    /// Modo de mouse del programa, si los eventos deben ir a él. Shift fuerza el comportamiento de
    /// Kuntur (seleccionar, copiar/pegar, desplazar el historial).
    fn mouse_reporting(&self, shift: bool) -> Option<MouseMode> {
        let mode = self.pane.with_emulator(|e| e.mouse_mode());
        (mode.report != MouseReporting::Off && !shift).then_some(mode)
    }

    fn send_mouse(&self, button: MouseButtonKind, action: MouseAction, cell: (usize, usize), m: Modifiers, sgr: bool) {
        let report = MouseReport {
            button,
            action,
            col: cell.0,
            row: cell.1,
            // Shift no se informa: en Kuntur significa "seleccionar en vez de enviar".
            mods: Mods {
                ctrl: m.control,
                alt: m.alt,
                shift: false,
            },
        };
        if let Some(bytes) = encode_mouse(&report, sgr) {
            let _ = self.pane.write(&bytes);
        }
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        if self.link_click(ev, window, cx) {
            return;
        }
        let m = self.metrics(window);
        let (col, row, side) = self.cell_at(ev.position, m);

        if let Some(mode) = self.mouse_reporting(ev.modifiers.shift) {
            let button = match ev.button {
                MouseButton::Left => MouseButtonKind::Left,
                MouseButton::Middle => MouseButtonKind::Middle,
                MouseButton::Right => MouseButtonKind::Right,
                _ => return,
            };
            self.pane.with_emulator(|e| e.clear_selection());
            self.mouse_button = Some(button);
            self.mouse_cell = Some((col, row));
            self.send_mouse(button, MouseAction::Press, (col, row), ev.modifiers, mode.sgr);
            cx.notify();
            return;
        }

        match ev.button {
            MouseButton::Left => {
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
            // Clic derecho: copia si hay selección; si no, pega (igual que Windows Terminal).
            MouseButton::Right if !self.copy_selection(cx) => self.paste(cx),
            _ => {}
        }
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.update_hover(ev.position, ev.modifiers, window, cx);
        let m = self.metrics(window);
        let (col, row, side) = self.cell_at(ev.position, m);

        if self.selecting {
            if ev.pressed_button == Some(MouseButton::Left) {
                self.pane.with_emulator(|e| e.update_selection(col, row, side));
                cx.notify();
            }
            return;
        }

        // Arrastre (1002) o cualquier movimiento (1003): solo al cambiar de celda.
        let mode = self.pane.with_emulator(|e| e.mouse_mode());
        let send = match mode.report {
            MouseReporting::Motion => true,
            MouseReporting::Drag => self.mouse_button.is_some(),
            MouseReporting::Click | MouseReporting::Off => false,
        };
        if send && self.mouse_cell != Some((col, row)) {
            self.mouse_cell = Some((col, row));
            let button = self.mouse_button.unwrap_or(MouseButtonKind::None);
            self.send_mouse(button, MouseAction::Move, (col, row), ev.modifiers, mode.sgr);
        }
    }

    fn on_mouse_up(&mut self, ev: &MouseUpEvent, window: &mut Window, _cx: &mut Context<Self>) {
        // El programa recibió el clic: recibe también el soltar (aunque sea fuera del área).
        if let Some(button) = self.mouse_button.take() {
            let mode = self.pane.with_emulator(|e| e.mouse_mode());
            if mode.report != MouseReporting::Off {
                let m = self.metrics(window);
                let (col, row, _) = self.cell_at(ev.position, m);
                self.send_mouse(button, MouseAction::Release, (col, row), ev.modifiers, mode.sgr);
            }
            self.mouse_cell = None;
            return;
        }
        if ev.button == MouseButton::Left && self.selecting {
            self.selecting = false;
            // Un clic sin arrastre no deja selección vacía.
            if self.pane.with_emulator(|e| e.selection_text().is_none()) {
                self.pane.with_emulator(|e| e.clear_selection());
            }
        }
    }

    fn on_wheel(&mut self, ev: &ScrollEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = self.metrics(window);
        let dy = f32::from(ev.delta.pixel_delta(m.line_h).y);
        let lines = (dy / f32::from(m.line_h)).round() as i32;
        if lines == 0 {
            return;
        }
        if let Some(mode) = self.mouse_reporting(ev.modifiers.shift) {
            let (col, row, _) = self.cell_at(ev.position, m);
            let button = if lines > 0 {
                MouseButtonKind::WheelUp
            } else {
                MouseButtonKind::WheelDown
            };
            for _ in 0..lines.unsigned_abs().min(10) {
                self.send_mouse(button, MouseAction::Press, (col, row), ev.modifiers, mode.sgr);
            }
            return;
        }
        self.pane.with_emulator(|e| e.scroll(lines));
        cx.notify();
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
        let metrics = self.metrics(window);
        let screen = self.pane.with_emulator(|e| Screen::capture(e, &self.palette));
        let bg = self.background;
        let cursor_color = to_color(self.palette.cursor);
        // Con dos paneles (principal y acoplada) el cursor dice cuál tiene el teclado.
        let focused = self.focus.is_focused(window);
        let links = self.underlined_links(&screen);
        let (family, font_size) = (self.font_family.clone(), px(self.font_size));
        let (pane, bounds_cell, size_cell) = (self.pane.clone(), self.bounds.clone(), self.last_size.clone());
        let ghost = self
            .ghost
            .shown()
            .filter(|g| focused && screen.cursor == Some((g.col, g.row)))
            .cloned();
        let ghost_color = to_color(self.palette.fg).opacity(GHOST_OPACITY);
        let shell_blocks = self.pane.visible_blocks();
        let block_colors = blocks::BlockColors {
            ok: to_color(self.palette.fg).opacity(0.25),
            error: to_color(self.palette.ansi[1]).opacity(0.85),
        };
        let image_bar = self.render_image_bar(cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(bg)
            // Aire alrededor del texto; la grilla se calcula con el tamaño real del lienzo.
            .pl(px(12.))
            .pr(px(6.))
            .pt(px(8.))
            .pb(px(4.))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_any_mouse_down(cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::on_mouse_up))
            // Soltar fuera del área también termina la selección o el arrastre del programa.
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll(cx.listener(Self::on_wheel))
            .on_modifiers_changed(cx.listener(Self::on_modifiers))
            // Ajustes v0.2: archivos soltados (una imagen con un agente abierto se le entrega como
            // imagen, no como texto; ver `image_bar.rs`).
            .on_drop(cx.listener(|_, paths: &ExternalPaths, _, cx| {
                cx.emit(PasteEvent::Dropped(paths.paths().to_vec()));
            }))
            .cursor(if self.hover_link.is_some() {
                CursorStyle::PointingHand
            } else {
                CursorStyle::Arrow
            })
            .children(image_bar)
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
                    move |bounds, _, window, cx| {
                        let style = TextStyleCfg {
                            family: family.clone(),
                            size: font_size,
                            cursor: cursor_color,
                            focused,
                            links,
                        };
                        paint_screen(&screen, bounds, metrics, &style, window, cx);
                        blocks::paint(&shell_blocks, bounds, metrics, &block_colors, window);
                        if let Some(g) = &ghost {
                            let style = ghost::GhostStyle {
                                family,
                                size: font_size,
                                color: ghost_color,
                            };
                            ghost::paint(g, screen.cols, bounds, metrics, &style, window, cx);
                        }
                    },
                )
                .w_full()
                .flex_1()
                .min_h_0(),
            )
    }
}

struct TextStyleCfg {
    family: SharedString,
    size: Pixels,
    cursor: Hsla,
    /// Sin el teclado, el cursor se dibuja hueco.
    focused: bool,
    /// Enlaces subrayados (fila, columna inicial, columna final exclusiva).
    links: Vec<(usize, usize, usize)>,
}

fn paint_screen(
    screen: &Screen,
    bounds: Bounds<Pixels>,
    m: Metrics,
    style: &TextStyleCfg,
    window: &mut Window,
    cx: &mut App,
) {
    let font_size = style.size;
    let cursor = style.cursor;
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
                window.paint_quad(fill(
                    Bounds::new(origin, size(m.cell_w * (col - start) as f32, m.line_h)),
                    to_color(bg),
                ));
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
            let mut f = font(style.family.clone());
            if cell.bold {
                f.weight = FontWeight::BOLD;
            }
            if cell.italic {
                f.style = FontStyle::Italic;
            }
            let color = to_color(cell.fg.with_min_contrast(cell.bg, MIN_CONTRAST));
            let underline = cell.underline.then(|| UnderlineStyle {
                thickness: px(1.0),
                color: Some(color),
                wavy: false,
            });
            match runs.last_mut() {
                Some(r) if r.font == f && r.color == color && r.underline == underline => r.len += len,
                _ => runs.push(TextRun {
                    len,
                    font: f,
                    color,
                    background_color: None,
                    underline,
                    strikethrough: None,
                }),
            }
        }
        if text.trim_end().is_empty() {
            continue;
        }
        let line = window
            .text_system()
            .shape_line(text.into(), font_size, &runs, Some(m.cell_w));
        let _ = line.paint(point(bounds.origin.x, y), m.line_h, TextAlign::Left, None, window, cx);
    }
    // Enlaces (F8): subrayado del color del cursor.
    for &(row, start, end) in &style.links {
        let origin = point(
            bounds.origin.x + m.cell_w * start as f32,
            bounds.origin.y + m.line_h * (row + 1) as f32 - px(2.),
        );
        window.paint_quad(fill(
            Bounds::new(origin, size(m.cell_w * (end - start) as f32, px(1.))),
            cursor,
        ));
    }
    // 3) Cursor: bloque semitransparente; hueco si el panel no tiene el teclado.
    if let Some((c, r)) = screen.cursor {
        let origin = point(
            bounds.origin.x + m.cell_w * c as f32,
            bounds.origin.y + m.line_h * r as f32,
        );
        let cell = Bounds::new(origin, size(m.cell_w, m.line_h));
        if style.focused {
            window.paint_quad(fill(cell, cursor.opacity(0.55)));
        } else {
            window.paint_quad(outline(cell, cursor.opacity(0.7), BorderStyle::default()));
        }
    }
}
