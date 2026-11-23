//! Vista raíz de Kuntur: barra de título (34 px), riel de sesiones (completo ↔ íconos, nunca
//! oculto) y la terminal activa. Diseño v4: una sola barra arriba, jerarquía por peso y color de
//! texto, íconos de un solo color y lo secundario al pasar el mouse.

use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, TITLE_BAR_HEIGHT, TitleBar, h_flex, hover_card::HoverCard, tooltip::Tooltip, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use kuntur_config::{Settings, SidebarMode, Theme, ThemeId, builtin};
use kuntur_term::{GridSize, PaneOptions};

use crate::sessions::{SessionList, next_default_name, short_title};
use crate::terminal_view::{TerminalEvent, TerminalView};
use crate::theme_bridge::{FALLBACK_MONO, FALLBACK_UI, UiColors, apply_to_kit, palette_from, pick_font};

const RAIL_ICONS: f32 = 60.0;
const RAIL_FULL: f32 = 232.0;
/// Margen izquierdo de la barra: centra el botón del riel sobre la columna de íconos.
const TITLE_PAD_LEFT: f32 = 16.0;
const ICON_BUTTON: f32 = 28.0;
const RIGHT_GROUP: f32 = ICON_BUTTON + 8.0;
const UI_TEXT: f32 = 12.5;

struct Session {
    view: Entity<TerminalView>,
    _events: Subscription,
}

pub struct Workspace {
    sessions: SessionList<Session>,
    settings: Settings,
    /// Dónde guardar la configuración (`None` = no guardar, por ejemplo en pruebas de humo).
    config_path: Option<PathBuf>,
    theme_id: ThemeId,
    theme: Theme,
    colors: UiColors,
    mono_font: SharedString,
    ui_font: SharedString,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, settings: Settings, config_path: Option<PathBuf>) -> Self {
        let theme_id = ThemeId::from_key(&settings.theme).unwrap_or(ThemeId::Dark);
        let theme = builtin(theme_id);
        let installed = window.text_system().all_font_names();
        let mut this = Self {
            sessions: SessionList::new(),
            config_path,
            theme_id,
            colors: UiColors::from_theme(&theme),
            mono_font: pick_font(&installed, &theme.fonts.mono, FALLBACK_MONO),
            ui_font: pick_font(&installed, &theme.fonts.ui, FALLBACK_UI),
            theme,
            settings,
        };
        apply_to_kit(&this.theme, cx);

        // Prueba de humo: n sesiones sin tocar la configuración guardada.
        let smoke_sessions = std::env::var("KUNTUR_SMOKE_SESSIONS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .map(|n| n.clamp(1, 9));
        if let Some(n) = smoke_sessions {
            this.config_path = None;
            for _ in 0..n {
                this.add_session(None, None, window, cx);
            }
        } else {
            let saved = this.settings.sessions.clone();
            for s in saved {
                this.add_session(Some(s.name), s.shell, window, cx);
            }
            if this.sessions.is_empty() {
                this.add_session(None, None, window, cx);
            }
        }

        // KUNTUR_SMOKE_INPUT escribe comandos solo en la primera sesión.
        if let (Ok(input), Some(first)) = (std::env::var("KUNTUR_SMOKE_INPUT"), this.sessions.iter().next()) {
            first.value.view.read(cx).send_smoke_input(input);
        }

        this.sessions.activate_index(0);
        this.focus_active(window, cx);

        // El tema de la prueba se aplica en vivo, sobre las sesiones ya abiertas (mismo camino que
        // un cambio de tema desde la interfaz).
        if let Some(id) = std::env::var("KUNTUR_SMOKE_THEME")
            .ok()
            .and_then(|k| ThemeId::from_key(&k))
        {
            this.config_path = None;
            this.set_theme(id, window, cx);
        }
        this
    }

    // ── Sesiones ────────────────────────────────────────────────────────────

    fn add_session(
        &mut self,
        name: Option<String>,
        shell: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let size = GridSize::new(100, 30);
        let opts = match shell {
            Some(program) => PaneOptions {
                program,
                args: Vec::new(),
                cwd: None,
                size,
            },
            None => PaneOptions::default_shell(size),
        };
        let name = name.unwrap_or_else(|| {
            let existing: Vec<&str> = self.sessions.iter().map(|e| e.name.as_str()).collect();
            next_default_name("PowerShell", &existing)
        });
        let palette = palette_from(&self.theme);
        let (family, size_pt, background) = (self.mono_font.clone(), self.settings.font_size, self.colors.terminal);
        let view = cx.new(|cx| {
            let mut v = TerminalView::new(window, cx, opts, palette.clone(), family, size_pt);
            v.set_palette(palette, background, cx);
            v
        });
        let events = cx.subscribe_in(&view, window, |this, view, ev: &TerminalEvent, window, cx| match ev {
            TerminalEvent::TitleChanged => cx.notify(),
            TerminalEvent::Exited => {
                let id = this.sessions.iter().find(|e| &e.value.view == view).map(|e| e.id);
                if let Some(id) = id {
                    this.close_session(id, window, cx);
                }
            }
        });
        self.sessions.push(name, Session { view, _events: events });
        cx.notify();
    }

    fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_session(None, None, window, cx);
        self.focus_active(window, cx);
    }

    /// Cierra la sesión (su PTY termina al soltar la vista). Cerrar la última cierra la ventana.
    fn close_session(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.sessions.close(id).is_none() {
            return;
        }
        if self.sessions.is_empty() {
            window.remove_window();
            return;
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    fn activate(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.sessions.activate(id) {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    fn focus_active(&self, window: &mut Window, cx: &mut App) {
        if let Some(e) = self.sessions.active() {
            e.value.view.update(cx, |v, cx| v.focus(window, cx));
        }
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.settings.sidebar = match self.settings.sidebar {
            SidebarMode::Icons => SidebarMode::Full,
            SidebarMode::Full => SidebarMode::Icons,
        };
        self.persist();
        cx.notify();
    }

    // ── Tema ────────────────────────────────────────────────────────────────

    /// Cambia el tema en vivo: ventana, barra, riel y todas las terminales.
    fn set_theme(&mut self, id: ThemeId, window: &mut Window, cx: &mut Context<Self>) {
        let theme = builtin(id);
        let installed = window.text_system().all_font_names();
        self.mono_font = pick_font(&installed, &theme.fonts.mono, FALLBACK_MONO);
        self.ui_font = pick_font(&installed, &theme.fonts.ui, FALLBACK_UI);
        self.colors = UiColors::from_theme(&theme);
        let palette = palette_from(&theme);
        for e in self.sessions.iter() {
            e.value.view.update(cx, |v, cx| {
                v.set_palette(palette.clone(), self.colors.terminal, cx);
                v.set_font(self.mono_font.clone(), self.settings.font_size, cx);
            });
        }
        apply_to_kit(&theme, cx);
        self.theme = theme;
        self.theme_id = id;
        self.settings.theme = id.key().into();
        self.persist();
        cx.notify();
    }

    /// Botón `Command`: hasta que exista la paleta de comandos (Task 8) alterna el tema.
    fn on_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let all = ThemeId::ALL;
        let i = all.iter().position(|&t| t == self.theme_id).unwrap_or(0);
        self.set_theme(all[(i + 1) % all.len()], window, cx);
        self.focus_active(window, cx);
    }

    fn persist(&self) {
        if let Some(path) = &self.config_path {
            let _ = self.settings.save(path);
        }
    }

    // ── Teclado ─────────────────────────────────────────────────────────────

    /// Atajos de Kuntur. Se capturan antes que la terminal: con Ctrl+Shift no chocan con el shell.
    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = ev.keystroke.modifiers;
        if !m.control || m.alt || m.platform {
            return;
        }
        let key = ev.keystroke.key.to_ascii_lowercase();
        match (m.shift, key.as_str()) {
            (true, "t") => self.new_session(window, cx),
            (true, "w") => {
                if let Some(id) = self.sessions.active_id() {
                    self.close_session(id, window, cx);
                }
            }
            (false, "tab") => {
                self.sessions.next();
                self.focus_active(window, cx);
                cx.notify();
            }
            (true, "tab") => {
                self.sessions.prev();
                self.focus_active(window, cx);
                cx.notify();
            }
            (true, "b") => self.toggle_sidebar(cx),
            (false, d) if d.len() == 1 && ('1'..='9').contains(&d.chars().next().unwrap_or('0')) => {
                let n = d.parse::<usize>().unwrap_or(1);
                if self.sessions.activate_index(n - 1) {
                    self.focus_active(window, cx);
                    cx.notify();
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    // ── Dibujo ──────────────────────────────────────────────────────────────

    fn session_title(&self, view: &Entity<TerminalView>, cx: &App) -> Option<String> {
        view.read(cx).title().map(short_title).filter(|t| !t.is_empty())
    }

    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = &self.colors;
        let (name, title) = match self.sessions.active() {
            Some(e) => (e.name.clone(), self.session_title(&e.value.view, cx)),
            None => (String::new(), None),
        };
        // El grupo izquierdo compensa los botones de Windows para que el título quede centrado
        // respecto de la ventana.
        // `TitleBar` dibuja 3 botones de Windows de `TITLE_BAR_HEIGHT` de ancho cada uno.
        let window_controls = 3.0 * f32::from(TITLE_BAR_HEIGHT);
        let left_w = window_controls + RIGHT_GROUP - TITLE_PAD_LEFT;
        let rail_tip = match self.settings.sidebar {
            SidebarMode::Icons => "Mostrar sesiones completas (Ctrl+Shift+B)",
            SidebarMode::Full => "Mostrar solo íconos (Ctrl+Shift+B)",
        };

        TitleBar::new()
            .bg(c.titlebar)
            .border_color(c.border)
            .pl(px(TITLE_PAD_LEFT))
            .child(
                h_flex()
                    .w_full()
                    .h_full()
                    .items_center()
                    .child(
                        h_flex().w(px(left_w)).flex_shrink_0().child(
                            icon_button("toggle-rail", IconName::PanelLeft, c, rail_tip)
                                .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx))),
                        ),
                    )
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .justify_center()
                            .gap(px(8.))
                            .text_size(px(UI_TEXT))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(c.text)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(name),
                            )
                            .when_some(title, |this, t| {
                                this.child(div().flex_shrink_0().text_color(c.muted).child("·")).child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .text_color(c.muted)
                                        .child(t),
                                )
                            }),
                    )
                    .child(
                        h_flex()
                            .w(px(RIGHT_GROUP))
                            .flex_shrink_0()
                            .justify_end()
                            .pr(px(4.))
                            .child(
                                icon_button("command", IconName::Command, c, "Acciones (Ctrl+Shift+P)")
                                    .on_click(cx.listener(|this, _, window, cx| this.on_command(window, cx))),
                            ),
                    ),
            )
    }

    fn render_rail(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.settings.sidebar {
            SidebarMode::Icons => self.render_rail_icons(cx).into_any_element(),
            SidebarMode::Full => self.render_rail_full(cx).into_any_element(),
        }
    }

    /// Riel de 60 px: un ícono por sesión, activa con fondo sutil y barra de acento; `+` al pie.
    fn render_rail_icons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = self.colors.clone();
        let active = self.sessions.active_id();
        let tiles: Vec<AnyElement> = self
            .sessions
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let id = e.id;
                let is_active = Some(id) == active;
                let name = e.name.clone();
                let title = self.session_title(&e.value.view, cx);
                let shortcut = (i < 9).then(|| format!("Ctrl+{}", i + 1));
                let tile = div()
                    .id(("tile", id))
                    .relative()
                    .w(px(RAIL_ICONS))
                    .h(px(40.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| this.activate(id, window, cx)))
                    .when(is_active, |this| this.child(accent_bar(c.accent, 20.)))
                    .child(
                        div()
                            .size(px(36.))
                            .rounded(px(10.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .map(|this| {
                                if is_active {
                                    this.bg(c.surface_2).text_color(c.text)
                                } else {
                                    this.text_color(c.muted).hover(|s| s.bg(c.hover).text_color(c.text))
                                }
                            })
                            .child(Icon::new(IconName::SquareTerminal).size(px(18.))),
                    );
                let card = c.clone();
                HoverCard::new(("session-card", id))
                    .anchor(Anchor::LeftCenter)
                    .open_delay(Duration::from_millis(350))
                    .close_delay(Duration::from_millis(120))
                    .trigger(tile)
                    .content(move |_, _, _| session_card(&card, &name, title.as_deref(), shortcut.as_deref()))
                    .into_any_element()
            })
            .collect();

        v_flex()
            .w(px(RAIL_ICONS))
            .h_full()
            .flex_shrink_0()
            .bg(c.surface)
            .border_r_1()
            .border_color(c.border)
            .pt(px(8.))
            .pb(px(10.))
            .child(v_flex().flex_1().min_h_0().gap(px(4.)).children(tiles))
            .child(
                h_flex().w_full().justify_center().child(
                    icon_button("new-session", IconName::Plus, &c, "Nueva sesión (Ctrl+Shift+T)")
                        .size(px(36.))
                        .rounded(px(10.))
                        .on_click(cx.listener(|this, _, window, cx| this.new_session(window, cx))),
                ),
            )
    }

    /// Riel de 232 px: ícono + nombre + título del programa en tenue; `×` al pasar el mouse.
    fn render_rail_full(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = self.colors.clone();
        let active = self.sessions.active_id();
        let rows: Vec<AnyElement> = self
            .sessions
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let id = e.id;
                let is_active = Some(id) == active;
                let group: SharedString = format!("session-row-{id}").into();
                let subtitle = self.session_title(&e.value.view, cx).unwrap_or_else(|| {
                    if i < 9 {
                        format!("Ctrl+{}", i + 1)
                    } else {
                        String::new()
                    }
                });
                div()
                    .id(("row", id))
                    .group(group.clone())
                    .relative()
                    .h(px(44.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .map(|this| {
                        if is_active {
                            this.bg(c.surface_2)
                        } else {
                            this.hover(|s| s.bg(c.hover))
                        }
                    })
                    .on_click(cx.listener(move |this, _, window, cx| this.activate(id, window, cx)))
                    .when(is_active, |this| this.child(accent_bar(c.accent, 22.)))
                    .child(
                        Icon::new(IconName::SquareTerminal)
                            .size(px(16.))
                            .text_color(if is_active { c.text } else { c.muted }),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(if is_active {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::MEDIUM
                                    })
                                    .text_color(if is_active { c.text } else { c.muted })
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(e.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_family(self.mono_font.clone())
                                    .text_color(c.muted)
                                    .opacity(0.85)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(subtitle),
                            ),
                    )
                    .child(
                        icon_button(("close", id), IconName::Close, &c, "Cerrar sesión (Ctrl+Shift+W)")
                            .size(px(22.))
                            .rounded(px(6.))
                            .when(!is_active, |this| {
                                this.opacity(0.).group_hover(group, |s| s.opacity(1.))
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.close_session(id, window, cx);
                            })),
                    )
                    .into_any_element()
            })
            .collect();

        v_flex()
            .w(px(RAIL_FULL))
            .h_full()
            .flex_shrink_0()
            .bg(c.surface)
            .border_r_1()
            .border_color(c.border)
            .child(
                h_flex()
                    .h(px(40.))
                    .pl(px(16.))
                    .pr(px(10.))
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(c.muted)
                            .child("Sesiones"),
                    )
                    .child(
                        icon_button("new-session", IconName::Plus, &c, "Nueva sesión (Ctrl+Shift+T)")
                            .on_click(cx.listener(|this, _, window, cx| this.new_session(window, cx))),
                    ),
            )
            .child(v_flex().flex_1().min_h_0().px(px(8.)).gap(px(2.)).children(rows))
    }
}

/// Botón de un solo ícono: gris en reposo, color de texto y fondo sutil al pasar el mouse.
fn icon_button(id: impl Into<ElementId>, icon: IconName, c: &UiColors, tip: &'static str) -> Stateful<Div> {
    let (hover, text) = (c.hover, c.text);
    div()
        .id(id)
        .size(px(ICON_BUTTON))
        .flex_shrink_0()
        .rounded(px(7.))
        .flex()
        .items_center()
        .justify_center()
        .text_color(c.muted)
        .cursor_pointer()
        // Sin esto, la barra de título trataría el clic como arrastre de la ventana.
        .occlude()
        .hover(move |s| s.bg(hover).text_color(text))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .child(Icon::new(icon).size(px(16.)))
}

/// Barra de acento de 3 px pegada al borde izquierdo (sesión activa).
fn accent_bar(color: Hsla, height: f32) -> Div {
    div()
        .absolute()
        .left_0()
        .top(relative(0.5))
        .mt(px(-height / 2.))
        .w(px(3.))
        .h(px(height))
        .rounded_r(px(3.))
        .bg(color)
}

/// Tarjeta al pasar el mouse sobre una sesión en el riel de íconos.
fn session_card(c: &UiColors, name: &str, title: Option<&str>, shortcut: Option<&str>) -> Div {
    v_flex()
        .ml(px(6.))
        .min_w(px(180.))
        .max_w(px(320.))
        .gap(px(4.))
        .text_size(px(UI_TEXT))
        .child(
            h_flex()
                .gap(px(12.))
                .justify_between()
                .child(
                    div()
                        .text_color(c.text)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name.to_string()),
                )
                .when_some(shortcut, |this, s| {
                    this.child(div().text_size(px(11.)).text_color(c.muted).child(s.to_string()))
                }),
        )
        .when_some(title, |this, t| {
            this.child(
                div()
                    .text_size(px(11.5))
                    .text_color(c.muted)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(t.to_string()),
            )
        })
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = self.colors.clone();
        let active = self.sessions.active().map(|e| e.value.view.clone());
        v_flex()
            .size_full()
            .bg(c.window)
            .text_color(c.text)
            .font_family(self.ui_font.clone())
            .text_size(px(UI_TEXT))
            .capture_key_down(cx.listener(Self::on_key))
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_rail(cx))
                    .child(div().flex_1().min_w_0().h_full().children(active)),
            )
    }
}
