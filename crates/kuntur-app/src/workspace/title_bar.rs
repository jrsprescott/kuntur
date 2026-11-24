//! Barra de título (34 px): botón del panel · pestañas de sesiones · `+` · espacio de arrastre ·
//! botones de ícono (Terminal acoplada, Tema, Uso de agentes, Auditoría, Acciones) · botones de Windows.

use gpui_kit::component::TitleBar;
use kuntur_agents::AgentState;

use super::*;

/// Ancho máximo de una pestaña (el nombre se corta con elipsis).
const TAB_MAX: f32 = 200.0;
const TAB_HEIGHT: f32 = 26.0;

impl Workspace {
    pub(super) fn panel_width(&self) -> f32 {
        match self.settings.sidebar {
            SidebarMode::Icons => PANEL_COMPACT,
            SidebarMode::Full => PANEL_FULL,
        }
    }

    /// Estado más urgente de los agentes de una sesión: esperando gana a trabajando.
    fn session_state(&self, id: u64, cx: &App) -> Option<AgentState> {
        if !self.running.iter().any(|(sid, _, _)| *sid == id) {
            return None;
        }
        let entry = self.sessions.iter().find(|e| e.id == id)?;
        Some(state_for(entry.value.view.read(cx).last_output_age()))
    }

    pub(super) fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = &self.colors;
        let panel_tip = match self.settings.sidebar {
            SidebarMode::Icons => "Panel completo (Ctrl+Shift+B)",
            SidebarMode::Full => "Panel compacto (Ctrl+Shift+B)",
        };
        let active = self.sessions.active_id();
        let tabs: Vec<AnyElement> = self
            .sessions
            .iter()
            .enumerate()
            .map(|(i, e)| self.render_tab(i, e.id, &e.name, Some(e.id) == active, cx))
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
                            ),
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
                            icon_button("new-session", IconName::Plus, c, "Nueva sesión (Ctrl+Shift+T)")
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
                                icon_button("dock", IconName::PanelBottom, c, "Terminal acoplada (Ctrl+`)")
                                    .when(self.dock_open() && !self.usage.open && !self.audit.open, |this| {
                                        this.bg(c.surface_2).text_color(c.text)
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| this.toggle_dock(window, cx))),
                            )
                            .child(
                                icon_button("theme", IconName::Palette, c, "Tema: Blue · Dark · White")
                                    .on_click(cx.listener(|this, _, window, cx| this.toggle_theme_menu(window, cx))),
                            )
                            .child(
                                icon_button("usage", IconName::ChartColumn, c, "Uso de agentes (Ctrl+Shift+U)")
                                    .when(self.usage.open, |this| this.bg(c.surface_2).text_color(c.text))
                                    .on_click(cx.listener(|this, _, window, cx| this.toggle_usage(window, cx))),
                            )
                            .child(
                                icon_button("audit", IconName::ScrollText, c, "Auditoría de comandos (Ctrl+Shift+A)")
                                    .when(self.audit.open, |this| this.bg(c.surface_2).text_color(c.text))
                                    .on_click(cx.listener(|this, _, window, cx| this.toggle_audit(window, cx))),
                            )
                            .child(
                                icon_button("command", IconName::Command, c, "Acciones (Ctrl+Shift+P)")
                                    .on_click(cx.listener(|this, _, window, cx| this.toggle_palette(window, cx))),
                            ),
                    ),
            )
    }

    /// Pestaña: ícono, nombre (elipsis a 200 px), punto si sus agentes esperan o trabajan, insignia
    /// de alertas sin ver (§12) y `×` (visible en la activa y al pasar el mouse). Doble clic renombra;
    /// clic medio cierra.
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
        let tip: SharedString = {
            let mut parts = vec![name.to_string()];
            parts.extend(title);
            match alerts {
                0 => {}
                1 => parts.push("1 alerta sin ver".into()),
                n => parts.push(format!("{n} alertas sin ver")),
            }
            if index < 9 {
                parts.push(format!("Ctrl+{}", index + 1));
            }
            parts.join(" · ").into()
        };

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
            .child(Icon::new(IconName::SquareTerminal).size(px(14.)).flex_shrink_0())
            .child(
                div()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .when(is_active, |this| this.font_weight(FontWeight::MEDIUM))
                    .child(name.to_string()),
            )
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
                    .tooltip(|window, cx| Tooltip::new("Cerrar sesión (Ctrl+Shift+W)").build(window, cx))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_session(id, window, cx);
                    }))
                    .child(Icon::new(IconName::Close).size(px(12.))),
            )
            .into_any_element()
    }
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
