//! Barra de título (34 px): botón del panel · pestañas de sesiones · `+` · espacio de arrastre ·
//! botones de ícono (Terminal acoplada, Tema, Uso de agentes, Auditoría, Conexiones, MCP, Acciones) ·
//! botones de Windows.

use gpui_kit::component::TitleBar;
use kuntur_agents::AgentState;

use super::*;

/// Ancho máximo de una pestaña (el nombre se corta con elipsis).
const TAB_MAX: f32 = 200.0;
const TAB_HEIGHT: f32 = 26.0;
/// Ancho máximo del texto de una pestaña (lo demás: ícono, punto de estado, insignia y ×).
const TAB_TEXT_MAX: f32 = 118.0;

/// Marquesina del título de la pestaña activa (ajustes v0.2): solo se mueve si el texto no entra,
/// con la ventana enfocada, sin el mouse encima y con `marquesina = true`. Fuera de eso no hay
/// temporizador (nada se redibuja en reposo).
/// Pestaña que se está arrastrando para cambiarla de lugar: lo que se ve bajo el mouse.
pub(super) struct TabDrag {
    id: u64,
    name: SharedString,
    bg: Hsla,
    text: Hsla,
    border: Hsla,
}

impl Render for TabDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(TAB_HEIGHT))
            .max_w(px(TAB_MAX))
            .px(px(10.))
            .flex()
            .items_center()
            .rounded(px(7.))
            .bg(self.bg)
            .border_1()
            .border_color(self.border)
            .shadow_md()
            .opacity(0.9)
            .text_size(px(UI_TEXT))
            .text_color(self.text)
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .child(self.name.clone())
    }
}

#[derive(Default)]
pub(super) struct TabMarquee {
    session: Option<u64>,
    text: String,
    /// Ancho del texto menos el visible (≤ 0: entra).
    overflow: f32,
    hovered: bool,
    clock: crate::marquee::Marquee,
    /// Hay un temporizador corriendo.
    ticking: bool,
}

impl Workspace {
    /// Cambia de lugar una pestaña (arrastrándola o con Ctrl+Shift+RePág/AvPág) y guarda el orden.
    pub(super) fn move_session(&mut self, id: u64, to: usize, cx: &mut Context<Self>) {
        if self.sessions.move_to(id, to) {
            self.persist();
            cx.notify();
        }
    }

    /// Corre una pestaña un lugar a la izquierda (`-1`) o a la derecha (`+1`) y guarda el orden.
    fn shift_session(&mut self, id: u64, delta: isize, cx: &mut Context<Self>) {
        if self.sessions.shift(id, delta) {
            self.persist();
            cx.notify();
        }
    }

    /// Ctrl+Shift+RePág / AvPág: corre la pestaña activa.
    pub(super) fn shift_active_session(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(id) = self.sessions.active_id() {
            self.shift_session(id, delta, cx);
        }
    }

    pub(super) fn panel_width(&self) -> f32 {
        match self.settings.sidebar {
            SidebarMode::Icons => PANEL_COMPACT,
            SidebarMode::Full => PANEL_FULL,
        }
    }

    /// Estado más urgente de los agentes de una sesión: esperando gana a trabajando.
    fn session_state(&self, id: u64, cx: &App) -> Option<AgentState> {
        // F5b: con estado exacto si hay eventos (hooks / OSC 7501); si no, por actividad.
        self.exact_session_state(id, cx)
    }

    fn marquee_running(&self) -> bool {
        let m = &self.tab_marquee;
        self.settings.marquesina && self.window_active && !m.hovered && m.overflow > 0.0
    }

    /// Antes de dibujar: mide el título de la pestaña activa (solo cuando cambia) y, si hace falta,
    /// arranca el temporizador de la marquesina, que se detiene solo cuando ya no corresponde.
    pub(super) fn update_marquee(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.sessions.active_id();
        let text = active.map(|id| self.tab_label(id, cx)).unwrap_or_default();
        if self.tab_marquee.session != active || self.tab_marquee.text != text {
            let overflow = if text.is_empty() {
                0.0
            } else {
                let mut f = font(self.ui_font.clone());
                f.weight = FontWeight::MEDIUM;
                let run = TextRun {
                    len: text.len(),
                    font: f,
                    color: self.colors.text,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let width = window
                    .text_system()
                    .shape_line(text.clone().into(), px(UI_TEXT), &[run], None)
                    .width;
                f32::from(width) - TAB_TEXT_MAX
            };
            let m = &mut self.tab_marquee;
            m.session = active;
            m.text = text;
            m.overflow = overflow;
            m.hovered = false;
            m.clock.reset();
        }
        if self.marquee_running() && !self.tab_marquee.ticking {
            self.tab_marquee.ticking = true;
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(crate::marquee::TICK_MS))
                        .await;
                    let go = this
                        .update(cx, |w, cx| {
                            if !w.marquee_running() {
                                w.tab_marquee.ticking = false;
                                return false;
                            }
                            let overflow = w.tab_marquee.overflow;
                            w.tab_marquee.clock.tick(overflow);
                            cx.notify();
                            true
                        })
                        .unwrap_or(false);
                    if !go {
                        break;
                    }
                }
            })
            .detach();
        }
    }

    pub(super) fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = &self.colors;
        let panel_tip = match self.settings.sidebar {
            SidebarMode::Icons => crate::i18n::t(crate::i18n::T::TipPanelFull),
            SidebarMode::Full => crate::i18n::t(crate::i18n::T::TipPanelCompact),
        };
        let active = self.sessions.active_id();
        let tabs: Vec<AnyElement> = self
            .sessions
            .iter()
            .enumerate()
            .map(|(i, e)| self.render_tab(i, e.id, &self.tab_label(e.id, cx), Some(e.id) == active, cx))
            .collect();

        TitleBar::new()
            .bg(c.titlebar)
            .border_color(c.border)
            .pl(px(TITLE_PAD_LEFT))
            .child(
                h_flex()
                    .w_full()
                    .h_full()
                    .items_center()
                    // Columna del panel: el botón queda centrado sobre el panel compacto y las
                    // pestañas empiezan donde empieza la terminal.
                    .child(
                        h_flex()
                            .w(px(self.panel_width() - TITLE_PAD_LEFT))
                            .flex_shrink_0()
                            .child(
                                icon_button("toggle-panel", IconName::PanelLeft, c, panel_tip)
                                    .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx))),
                            )
                            // Ajustes v0.2.1: identidad (ícono + nombre) centrada sobre el panel completo.
                            .when(self.settings.sidebar == SidebarMode::Full, |this| {
                                this.child(render_brand(c))
                                    .child(div().w(px(ICON_BUTTON)).flex_shrink_0())
                            }),
                    )
                    .child(
                        h_flex()
                            .id("tabs")
                            .min_w_0()
                            .h_full()
                            .pl(px(6.))
                            .gap(px(2.))
                            .items_center()
                            .overflow_x_scroll()
                            .children(tabs),
                    )
                    .child(
                        div().flex_shrink_0().ml(px(4.)).child(
                            icon_button(
                                "new-session",
                                IconName::Plus,
                                c,
                                crate::i18n::t(crate::i18n::T::TipNewSession),
                            )
                            .on_click(cx.listener(|this, _, window, cx| this.new_session(window, cx))),
                        ),
                    )
                    // Espacio libre: arrastra la ventana.
                    .child(div().flex_1().min_w(px(24.)).h_full())
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap(px(2.))
                            .pr(px(4.))
                            .child(
                                icon_button(
                                    "dock",
                                    IconName::PanelBottom,
                                    c,
                                    crate::i18n::t(crate::i18n::T::TipDock),
                                )
                                .when(self.dock_open() && !self.usage.open && !self.audit.open, |this| {
                                    this.bg(c.surface_2).text_color(c.text)
                                })
                                .on_click(cx.listener(|this, _, window, cx| this.toggle_dock(window, cx))),
                            )
                            .child(
                                icon_button("theme", IconName::Palette, c, crate::i18n::t(crate::i18n::T::ThemeTip))
                                    .on_click(cx.listener(|this, _, window, cx| this.toggle_theme_menu(window, cx))),
                            )
                            .child(
                                icon_button(
                                    "usage",
                                    IconName::ChartColumn,
                                    c,
                                    crate::i18n::t(crate::i18n::T::TipUsage),
                                )
                                .when(self.usage.open, |this| this.bg(c.surface_2).text_color(c.text))
                                .on_click(cx.listener(|this, _, window, cx| this.toggle_usage(window, cx))),
                            )
                            .child(
                                icon_button(
                                    "audit",
                                    IconName::ScrollText,
                                    c,
                                    crate::i18n::t(crate::i18n::T::TipAudit),
                                )
                                .when(self.audit.open, |this| this.bg(c.surface_2).text_color(c.text))
                                .on_click(cx.listener(|this, _, window, cx| this.toggle_audit(window, cx))),
                            )
                            .child(
                                icon_button(
                                    "connections",
                                    IconName::Server,
                                    c,
                                    crate::i18n::t(crate::i18n::T::TipConnections),
                                )
                                .when(self.connections.open, |this| this.bg(c.surface_2).text_color(c.text))
                                .on_click(cx.listener(|this, _, window, cx| this.toggle_connections(window, cx))),
                            )
                            .child(
                                icon_button("mcp", IconName::Plug, c, crate::i18n::t(crate::i18n::T::TipMcp))
                                    .when(self.mcp.open, |this| this.bg(c.surface_2).text_color(c.text))
                                    .on_click(cx.listener(|this, _, window, cx| this.toggle_mcp(window, cx))),
                            )
                            .child(
                                icon_button(
                                    "settings",
                                    IconName::Settings,
                                    c,
                                    crate::i18n::t(crate::i18n::T::SettingsTip),
                                )
                                .when(self.settings_panel.open, |this| this.bg(c.surface_2).text_color(c.text))
                                .on_click(cx.listener(|this, _, window, cx| this.toggle_settings(None, window, cx))),
                            )
                            .child(
                                icon_button(
                                    "command",
                                    IconName::Command,
                                    c,
                                    crate::i18n::t(crate::i18n::T::TipActions),
                                )
                                .on_click(cx.listener(|this, _, window, cx| this.toggle_palette(window, cx))),
                            ),
                    ),
            )
    }

    /// Pestaña: ícono, nombre (elipsis a 200 px), punto si sus agentes esperan o trabajan, insignia
    /// de alertas sin ver (§12) y `×` (visible en la activa y al pasar el mouse). Doble clic renombra;
    /// clic medio cierra; arrastrarla y soltarla sobre otra la deja en el lugar de esa.
    fn render_tab(&self, index: usize, id: u64, name: &str, is_active: bool, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let group: SharedString = format!("tab-{id}").into();
        let hover = c.hover;
        let state = self.session_state(id, cx);
        let alerts = self.badges.get(&id).copied().unwrap_or(0);
        let title = self
            .sessions
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| self.session_title(&e.value.view, cx));
        let remote = self.remote_label(id);
        let tip: SharedString = {
            let mut parts = vec![name.to_string()];
            parts.extend(remote.clone());
            // El título completo (si la pestaña ya lo muestra, no se repite).
            parts.extend(title.filter(|t| t != name));
            match alerts {
                0 => {}
                1 => parts.push(crate::i18n::t(crate::i18n::T::TipOneAlert).into()),
                n => parts.push(format!("{n} {}", crate::i18n::t(crate::i18n::T::TipAlerts))),
            }
            if index < 9 {
                parts.push(format!("Ctrl+{}", index + 1));
            }
            parts.join(" · ").into()
        };

        let drag = TabDrag {
            id,
            name: name.to_string().into(),
            bg: c.surface_2,
            text: c.text,
            border: c.border,
        };
        let accent = c.accent;

        h_flex()
            .id(("tab", id))
            .group(group.clone())
            .flex_shrink_0()
            .max_w(px(TAB_MAX))
            .h(px(TAB_HEIGHT))
            .pl(px(10.))
            .pr(px(4.))
            .gap(px(7.))
            .items_center()
            .rounded(px(7.))
            .cursor_pointer()
            .occlude()
            .text_size(px(UI_TEXT))
            .map(|this| {
                if is_active {
                    this.bg(c.surface_2).text_color(c.text)
                } else {
                    this.text_color(c.muted).hover(move |s| s.bg(hover))
                }
            })
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            // Reordenar: se arrastra la pestaña y se suelta sobre otra (queda en su lugar).
            .on_drag(drag, |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| TabDrag {
                    id: drag.id,
                    name: drag.name.clone(),
                    bg: drag.bg,
                    text: drag.text,
                    border: drag.border,
                })
            })
            .drag_over::<TabDrag>(move |s, drag: &TabDrag, _, _| {
                if drag.id == id {
                    s
                } else {
                    s.bg(accent.opacity(0.18)).border_1().border_color(accent)
                }
            })
            .on_drop(cx.listener(move |this, drag: &TabDrag, _, cx| {
                this.move_session(drag.id, index, cx);
            }))
            .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                if ev.click_count() >= 2 {
                    this.start_rename(id, window, cx);
                } else {
                    this.activate(id, window, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.close_session(id, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.tab_menu = Some(TabMenu {
                        id,
                        position: ev.position,
                    });
                    cx.notify();
                }),
            )
            // Remota: globo (§6, siempre se ve si la pestaña es local o remota).
            .child(
                Icon::new(if remote.is_some() {
                    IconName::Globe
                } else {
                    IconName::SquareTerminal
                })
                .size(px(14.))
                .flex_shrink_0(),
            )
            .child(self.render_tab_text(id, name, is_active, cx))
            .when_some(state.filter(|_| alerts == 0), |this, state| {
                this.child(state_dot(c, state, 6.))
            })
            .when(alerts > 0, |this| this.child(alert_badge(c, alerts)))
            .child(
                div()
                    .id(("tab-close", id))
                    .size(px(18.))
                    .flex_shrink_0()
                    .rounded(px(5.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(c.muted)
                    .hover(move |s| s.bg(hover))
                    .when(!is_active, |this| {
                        this.opacity(0.).group_hover(group, |s| s.opacity(1.))
                    })
                    .tooltip(|window, cx| {
                        Tooltip::new(crate::i18n::t(crate::i18n::T::TipCloseSession)).build(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_session(id, window, cx);
                    }))
                    .child(Icon::new(IconName::Close).size(px(12.))),
            )
            .into_any_element()
    }
}

impl Workspace {
    /// Texto de la pestaña: con elipsis; en la activa, si no entra, la marquesina (se detiene con el
    /// mouse encima; el tooltip de la pestaña muestra el título completo).
    fn render_tab_text(&self, id: u64, name: &str, is_active: bool, cx: &mut Context<Self>) -> AnyElement {
        let m = &self.tab_marquee;
        let scrolling = is_active && self.settings.marquesina && m.session == Some(id) && m.overflow > 0.0;
        if !scrolling {
            return div()
                .min_w_0()
                .max_w(px(TAB_TEXT_MAX))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .when(is_active, |this| this.font_weight(FontWeight::MEDIUM))
                .child(name.to_string())
                .into_any_element();
        }
        div()
            .id(("tab-text", id))
            .w(px(TAB_TEXT_MAX))
            .flex_shrink_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .font_weight(FontWeight::MEDIUM)
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.tab_marquee.hovered = *hovered;
                cx.notify();
            }))
            .child(
                div()
                    .relative()
                    .left(px(-m.clock.offset()))
                    .whitespace_nowrap()
                    .child(name.to_string()),
            )
            .into_any_element()
    }
}

/// Ícono de Kuntur (el mismo de `assets/kuntur.ico`, en PNG de 64 px).
static LOGO: std::sync::LazyLock<Arc<gpui_kit::Image>> = std::sync::LazyLock::new(|| {
    Arc::new(gpui_kit::Image::from_bytes(
        gpui_kit::ImageFormat::Png,
        include_bytes!("../../../../assets/kuntur-64.png").to_vec(),
    ))
});

/// Página del proyecto.
fn project_url() -> String {
    format!("https://github.com/{}", kuntur_update::REPO)
}

/// Identidad en la barra (ajustes v0.2.1): ícono y nombre; un clic abre el proyecto en GitHub.
fn render_brand(c: &UiColors) -> Stateful<Div> {
    let (hover, muted, text) = (c.hover, c.muted, c.text);
    h_flex()
        .id("brand")
        .flex_1()
        .min_w_0()
        .h(px(TAB_HEIGHT))
        .justify_center()
        .items_center()
        .gap(px(7.))
        .rounded(px(7.))
        .cursor_pointer()
        // Sin esto, la barra de título trataría el clic como arrastre de la ventana.
        .occlude()
        .text_color(muted)
        .hover(move |s| s.bg(hover).text_color(text))
        .tooltip(|window, cx| Tooltip::new(crate::i18n::t(crate::i18n::T::BrandTip)).build(window, cx))
        .on_click(|_, _, cx| cx.open_url(&project_url()))
        .child(img(LOGO.clone()).size(px(18.)).flex_shrink_0().rounded(px(4.)))
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .whitespace_nowrap()
                .child("Kuntur"),
        )
}

/// Insignia de alertas sin ver: contador sobre ámbar (el color de "esperando").
fn alert_badge(c: &UiColors, count: u32) -> Div {
    let text = if count > 9 { "9+".to_string() } else { count.to_string() };
    div()
        .flex_shrink_0()
        .h(px(15.))
        .min_w(px(15.))
        .px(px(4.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(c.warn)
        .text_color(rgb(0x1a1a1a))
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .child(text)
}

/// Punto de estado: trabajando = acento, esperando = ámbar.
pub(super) fn state_dot(c: &UiColors, state: AgentState, size: f32) -> Div {
    div().size(px(size)).flex_shrink_0().rounded_full().bg(match state {
        AgentState::Working => c.accent,
        AgentState::Waiting => c.warn,
    })
}

/// Menú del clic derecho en una pestaña.
pub(super) struct TabMenu {
    id: u64,
    position: Point<Pixels>,
}

impl Workspace {
    /// Prueba de humo (`-TabMenu`): el menú de la pestaña activa, abierto bajo ella.
    pub(super) fn smoke_tab_menu(&mut self) {
        if let Some(id) = self.sessions.active_id() {
            self.tab_menu = Some(TabMenu {
                id,
                position: point(px(self.panel_width() + 60.), px(TAB_HEIGHT + 8.)),
            });
        }
    }

    fn close_tab_menu(&mut self, cx: &mut Context<Self>) {
        if self.tab_menu.take().is_some() {
            cx.notify();
        }
    }

    /// Esc con el menú de la pestaña abierto lo cierra.
    pub(super) fn tab_menu_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if self.tab_menu.is_some() && ev.keystroke.key == "escape" {
            self.close_tab_menu(cx);
            return true;
        }
        false
    }

    /// "Renombrar…" (el campo de la paleta, Enter guarda y Esc cancela) y "Cerrar pestaña".
    pub(super) fn render_tab_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.tab_menu.as_ref()?;
        let id = menu.id;
        let index = self.sessions.iter().position(|e| e.id == id)?;
        let can_left = index > 0;
        let can_right = index + 1 < self.sessions.iter().count();
        let c = &self.colors;
        let hover = c.hover;
        let item = |key: &'static str, label: crate::i18n::T, hint: Option<&'static str>| {
            h_flex()
                .id(key)
                .h(px(28.))
                .px(px(10.))
                .gap(px(16.))
                .items_center()
                .justify_between()
                .rounded(px(6.))
                .text_size(px(12.5))
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .child(crate::i18n::t(label))
                .children(hint.map(|h| div().text_size(px(11.)).text_color(c.muted).child(h)))
        };
        let list = v_flex()
            .id("tab-menu")
            .w(px(240.))
            .p(px(4.))
            .bg(c.popover)
            .border_1()
            .border_color(c.border)
            .rounded(px(8.))
            .shadow_lg()
            .occlude()
            .text_color(c.text)
            .font_family(self.ui_font.clone())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(
                item("tab-menu-rename", crate::i18n::T::TabRename, None).on_click(cx.listener(
                    move |this, _, window, cx| {
                        this.tab_menu = None;
                        this.start_rename(id, window, cx);
                    },
                )),
            )
            .when(can_left, |this| {
                this.child(
                    item(
                        "tab-menu-left",
                        crate::i18n::T::TabMoveLeft,
                        Some(crate::i18n::t(crate::i18n::T::TabMoveLeftKey)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab_menu = None;
                        this.shift_session(id, -1, cx);
                        cx.notify();
                    })),
                )
            })
            .when(can_right, |this| {
                this.child(
                    item(
                        "tab-menu-right",
                        crate::i18n::T::TabMoveRight,
                        Some(crate::i18n::t(crate::i18n::T::TabMoveRightKey)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab_menu = None;
                        this.shift_session(id, 1, cx);
                        cx.notify();
                    })),
                )
            })
            .child(
                item("tab-menu-close", crate::i18n::T::TabClose, Some("Ctrl+Shift+W")).on_click(cx.listener(
                    move |this, _, window, cx| {
                        this.tab_menu = None;
                        this.close_session(id, window, cx);
                    },
                )),
            );
        Some(
            deferred(
                div()
                    .id("tab-menu-backdrop")
                    .absolute()
                    .inset_0()
                    .occlude()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close_tab_menu(cx)))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _, _, cx| this.close_tab_menu(cx)),
                    )
                    .child(
                        anchored()
                            .position(menu.position)
                            .snap_to_window_with_margin(px(8.))
                            .child(list),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
