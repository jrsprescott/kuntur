//! Panel izquierdo: agentes corriendo (buscador, filtros, lista), agentes instalados con anillo de
//! límite y "Archivos". Completo (240 px) ↔ compacto (56 px, solo íconos); nunca se oculta.

use std::f32::consts::{FRAC_PI_2, TAU};

use kuntur_agents::AgentState;

use super::files_panel::empty_note;
use super::title_bar::state_dot;
use super::*;
use crate::agents_panel::{RingLevel, counts, filter_rows, format_elapsed, ring_pct, ring_tooltip};

/// Lado de la celda de un agente instalado; la insignia va dentro del anillo.
const INSTALLED_TILE: f32 = 32.0;
const INSTALLED_BADGE: f32 = 24.0;

/// Color de marca 0xRRGGBB → GPUI.
pub(super) fn brand(color: u32) -> Hsla {
    rgb(color).into()
}

/// Insignia de un agente: letra en su color sobre un fondo tenue de ese color.
pub(super) fn badge(letter: char, color: u32, size: f32, radius: f32) -> Div {
    let color = brand(color);
    div()
        .size(px(size))
        .flex_shrink_0()
        .rounded(px(radius))
        .flex()
        .items_center()
        .justify_center()
        .bg(color.opacity(0.18))
        .text_color(color)
        .text_size(px(size * 0.46))
        .font_weight(FontWeight::BOLD)
        .child(letter.to_string())
}

/// Anillo de porcentaje (0–100) que empieza arriba y avanza en sentido horario.
fn ring(pct: f32, color: Hsla, track: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let stroke = 2.0;
            let size = bounds.size.width.min(bounds.size.height).as_f32();
            let r = (size - stroke) / 2.0;
            let (cx, cy) = (
                bounds.origin.x.as_f32() + bounds.size.width.as_f32() / 2.0,
                bounds.origin.y.as_f32() + bounds.size.height.as_f32() / 2.0,
            );
            let at = |a: f32| point(px(cx + r * a.cos()), px(cy + r * a.sin()));
            let start = -FRAC_PI_2;
            // Pista completa (dos medias vueltas: un arco de 360° no se puede dibujar de una vez).
            let mut t = PathBuilder::stroke(px(stroke));
            t.move_to(at(start));
            t.arc_to(point(px(r), px(r)), px(0.), false, true, at(start + TAU / 2.0));
            t.arc_to(point(px(r), px(r)), px(0.), false, true, at(start + TAU - 0.001));
            if let Ok(path) = t.build() {
                window.paint_path(path, track);
            }
            let frac = (pct / 100.0).clamp(0.0, 0.9999);
            if frac > 0.0 {
                let mut p = PathBuilder::stroke(px(stroke));
                p.move_to(at(start));
                p.arc_to(point(px(r), px(r)), px(0.), frac > 0.5, true, at(start + frac * TAU));
                if let Ok(path) = p.build() {
                    window.paint_path(path, color);
                }
            }
        },
    )
    .absolute()
    .size_full()
}

impl Workspace {
    /// Insignia del agente `id` del catálogo (gris con "?" si ya no está).
    pub(super) fn agent_badge(&self, id: &str, size: f32, radius: f32) -> Div {
        let (letter, color) = self.catalog.get(id).map_or(('?', 0x888888), |a| (a.letter, a.color));
        badge(letter, color, size, radius)
    }

    /// Insignia de un agente de los historiales de uso / auditoría.
    pub(super) fn usage_badge(&self, kind: AgentKind, size: f32, radius: f32) -> Div {
        let (letter, color) = self
            .catalog
            .by_usage(kind)
            .map_or(('?', 0x888888), |a| (a.letter, a.color));
        badge(letter, color, size, radius)
    }

    pub(super) fn render_side_panel(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let panel = v_flex()
            .h_full()
            .flex_shrink_0()
            .bg(c.surface)
            .border_r_1()
            .border_color(c.border);
        match self.settings.sidebar {
            SidebarMode::Icons => panel
                .w(px(PANEL_COMPACT))
                .child(self.render_compact(cx))
                .into_any_element(),
            SidebarMode::Full => panel
                .w(px(PANEL_FULL))
                .child(match self.panel {
                    PanelView::Agents => self.render_agents_view(window, cx),
                    PanelView::Files => self.render_files_view(cx),
                })
                .child(self.render_panel_footer(cx))
                .into_any_element(),
        }
    }

    // ── Agentes (completo) ──────────────────────────────────────────────────

    fn render_agents_view(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let rows = self.agent_rows(cx);
        let shown = filter_rows(&rows, self.agent_filter, &self.agent_query);
        let now = now_secs();

        let list: AnyElement = if shown.is_empty() {
            let (title, hint) = if rows.is_empty() {
                (
                    "No hay agentes corriendo",
                    "Abre claude, codex, opencode… en una sesión",
                )
            } else if !self.agent_query.trim().is_empty() {
                ("Ningún agente coincide", "Prueba con otro nombre de agente o sesión")
            } else if self.agent_filter == AgentFilter::Waiting {
                ("Ningún agente esperando", "")
            } else {
                ("Ningún agente trabajando", "")
            };
            empty_note(c, title, hint).into_any_element()
        } else {
            let active = self.sessions.active_id();
            let items: Vec<AnyElement> = shown
                .iter()
                .map(|r| self.render_agent_row(r, Some(r.session_id) == active, now, cx))
                .collect();
            v_flex()
                .id("agent-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(6.))
                .py(px(6.))
                .gap(px(2.))
                .children(items)
                .into_any_element()
        };

        v_flex()
            .flex_1()
            .min_h_0()
            .child(self.render_search(window, cx))
            .child(self.render_filters(&counts(&rows), cx))
            .child(list)
            .into_any_element()
    }

    /// Buscador: campo propio (como la paleta) con cursor cuando tiene el foco.
    fn render_search(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = &self.colors;
        let focused = self.search_focus.is_focused(window);
        // `Div` no implementa `Clone`: el cursor se construye cada vez que se usa.
        let accent = c.accent;
        let caret = move || div().w(px(1.5)).h(px(14.)).flex_shrink_0().bg(accent);
        let query = self.agent_query.clone();
        div().px(px(10.)).pt(px(10.)).pb(px(4.)).child(
            h_flex()
                .id("agent-search")
                .track_focus(&self.search_focus)
                .h(px(30.))
                .px(px(10.))
                .gap(px(8.))
                .items_center()
                .rounded(px(7.))
                .bg(c.hover)
                .border_1()
                .border_color(if focused {
                    c.accent.opacity(0.6)
                } else {
                    c.hover.opacity(0.)
                })
                .cursor_text()
                .on_click(cx.listener(|this, _, window, cx| {
                    window.focus(&this.search_focus, cx);
                    cx.notify();
                }))
                .child(Icon::new(IconName::Search).size(px(14.)).text_color(c.muted))
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .map(|this| {
                            if query.is_empty() {
                                this.when(focused, |this| this.child(caret()))
                                    .child(div().ml(px(1.)).text_color(c.muted).child("Buscar agentes"))
                            } else {
                                this.child(div().text_color(c.text).child(query.clone()))
                                    .when(focused, |this| this.child(caret()))
                            }
                        }),
                )
                .when(!query.is_empty(), |this| {
                    this.child(
                        div()
                            .id("agent-search-clear")
                            .size(px(18.))
                            .rounded(px(5.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(c.muted)
                            .cursor_pointer()
                            .hover(|s| s.text_color(c.text))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.agent_query.clear();
                                cx.notify();
                            }))
                            .child(Icon::new(IconName::Close).size(px(12.))),
                    )
                }),
        )
    }

    /// Filtros como texto; el activo, en color pleno y subrayado con el acento.
    fn render_filters(&self, counts: &[usize; 3], cx: &mut Context<Self>) -> impl IntoElement {
        let c = &self.colors;
        let items: Vec<AnyElement> = AgentFilter::ALL
            .iter()
            .zip(counts)
            .map(|(&f, &n)| {
                let on = f == self.agent_filter;
                let text = c.text;
                h_flex()
                    .id(f.label())
                    .relative()
                    .h_full()
                    .gap(px(4.))
                    .items_center()
                    .cursor_pointer()
                    .text_size(px(12.))
                    .map(|this| {
                        if on {
                            this.text_color(c.text).font_weight(FontWeight::MEDIUM)
                        } else {
                            this.text_color(c.muted).hover(move |s| s.text_color(text))
                        }
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.agent_filter = f;
                        cx.notify();
                    }))
                    .child(f.label())
                    .when(n > 0, |this| {
                        this.child(
                            div()
                                .text_size(px(11.))
                                .font_weight(FontWeight::NORMAL)
                                .text_color(if f == AgentFilter::Waiting { c.warn } else { c.muted })
                                .child(n.to_string()),
                        )
                    })
                    .when(on, |this| {
                        this.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .h(px(2.))
                                .rounded(px(1.))
                                .bg(c.accent),
                        )
                    })
                    .into_any_element()
            })
            .collect();
        h_flex()
            .h(px(32.))
            .flex_shrink_0()
            .px(px(14.))
            .gap(px(16.))
            .border_b_1()
            .border_color(c.border)
            .children(items)
    }

    /// Fila: insignia, nombre y "sesión · estado" (o la acción actual) en tenue; a la derecha el punto
    /// y el tiempo. Debajo (F5b): subagentes, Permitir / Rechazar y el comando sugerido.
    fn render_agent_row(&self, r: &AgentRow, in_active: bool, now: u64, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let sid = r.session_id;
        let hover = c.hover;
        let detail = r.detail();
        let attention = r.needs_user();
        let subs = r.exact.as_ref().map(|e| e.subagent_counts()).filter(|(_, n)| *n > 0);
        let main = h_flex()
            .id(("agent", r.key() as usize))
            .relative()
            .h(px(44.))
            .px(px(8.))
            .gap(px(10.))
            .items_center()
            .rounded(px(8.))
            .cursor_pointer()
            .map(|this| {
                if in_active {
                    this.bg(c.surface_2)
                } else {
                    this.hover(move |s| s.bg(hover))
                }
            })
            .on_click(cx.listener(move |this, _, window, cx| this.activate(sid, window, cx)))
            .when(in_active, |this| this.child(accent_bar(c.accent, 20.)))
            .child(self.agent_badge(&r.agent, 26., 7.))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap(px(6.))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(c.text)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(r.name().to_string()),
                            )
                            // Subagentes trabajando / total.
                            .when_some(subs, |this, (on, n)| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_size(px(11.))
                                        .text_color(c.muted)
                                        .child(format!("{on}/{n}")),
                                )
                            }),
                    )
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap(px(4.))
                            .text_size(px(11.))
                            .text_color(c.muted)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            // Con la acción a continuación, la sesión no se encoge a "Pow…".
                            .child(
                                div()
                                    .min_w_0()
                                    .max_w(px(96.))
                                    .when(r.exact.is_some(), |this| this.flex_shrink_0())
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(r.session_name.clone()),
                            )
                            .child(div().flex_shrink_0().child("·"))
                            // En la acoplada no cabe todo: "acoplada" (que distingue la fila) reemplaza
                            // la palabra del estado, que sigue en el punto de color, los filtros y el
                            // tooltip del modo compacto. "esperando" sí se muestra (se colorea en ámbar).
                            // Con la acción actual ("Bash · npm test") la fila dice qué hace; sin
                            // eventos, el estado por actividad.
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .when(attention, |this| this.text_color(c.warn))
                                    .child(match (r.docked, r.exact.is_some(), r.state) {
                                        (true, false, AgentState::Working) => "acoplada".to_string(),
                                        (true, false, AgentState::Waiting) => "acoplada, esperando".to_string(),
                                        _ => detail,
                                    }),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap(px(6.))
                    .items_center()
                    .text_size(px(11.))
                    .text_color(c.muted)
                    .child(format_elapsed(
                        now.saturating_sub(r.exact.as_ref().map_or(r.started_at, |e| e.since)),
                    ))
                    .child(state_dot(c, r.state, 7.)),
            );
        let extras = self.render_row_extras(r, cx);
        if extras.is_empty() {
            return main.into_any_element();
        }
        v_flex()
            .rounded(px(8.))
            .when(r.exact_state() == Some(kuntur_agents::ExactState::Permission), |this| {
                this.bg(c.warn.opacity(0.08))
            })
            .child(main)
            .children(extras)
            .into_any_element()
    }

    // ── Pie: instalados + Archivos ──────────────────────────────────────────

    fn render_panel_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = &self.colors;
        let files_on = self.panel == PanelView::Files;
        let hover = c.hover;
        // Pestaña remota (F6): los agentes instalados en el servidor, no los de esta PC.
        let remote = self.active_remote_agents();
        let (ids, others) = match &remote {
            Some(names) => crate::connections::remote_agent_ids(&self.catalog, names),
            None => (self.installed.clone(), Vec::new()),
        };
        let tiles: Vec<AnyElement> = ids.iter().map(|id| self.installed_tile(id, cx)).collect();
        let has_agents = !tiles.is_empty() || !others.is_empty();
        v_flex()
            .flex_shrink_0()
            .border_t_1()
            .border_color(c.border)
            .px(px(8.))
            .pt(px(8.))
            .pb(px(8.))
            .gap(px(6.))
            .when(has_agents, |this| {
                this.child(
                    div()
                        .px(px(6.))
                        .text_size(px(11.))
                        .text_color(c.muted)
                        .child(if remote.is_some() {
                            "Agentes en el servidor"
                        } else {
                            "Agentes instalados"
                        }),
                )
                .child(h_flex().px(px(2.)).gap(px(4.)).flex_wrap().children(tiles))
                .when(!others.is_empty(), |this| {
                    this.child(
                        div()
                            .px(px(6.))
                            .text_size(px(11.))
                            .text_color(c.muted)
                            .child(others.join(" · ")),
                    )
                })
            })
            .child(
                h_flex()
                    .id("files-toggle")
                    .relative()
                    .h(px(32.))
                    .px(px(8.))
                    .gap(px(10.))
                    .items_center()
                    .rounded(px(7.))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .map(|this| {
                        if files_on {
                            this.bg(c.surface_2).text_color(c.text)
                        } else {
                            this.text_color(c.muted).hover(move |s| s.bg(hover))
                        }
                    })
                    .tooltip(|window, cx| {
                        Tooltip::new("Archivos de la carpeta de la sesión (Ctrl+Shift+E)").build(window, cx)
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_files(cx)))
                    .when(files_on, |this| this.child(accent_bar(c.accent, 18.)))
                    .child(
                        Icon::new(if files_on {
                            IconName::FolderOpen
                        } else {
                            IconName::Folder
                        })
                        .size(px(16.)),
                    )
                    .child("Archivos"),
            )
    }

    /// Agente instalado: insignia de 24 px dentro de un anillo cuando hay porcentaje real de su
    /// límite (hoy solo Codex); sin dato, sin anillo. Clic = abrirlo en la sesión activa (§3).
    fn installed_tile(&self, id: &str, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let Some(agent) = self.catalog.get(id) else {
            return Empty.into_any_element();
        };
        // Codex: sus registros; Claude Code: su línea de estado (F5b, con los ganchos instalados).
        let limits = match agent.usage {
            Some(AgentKind::Codex) => self.codex_limits.as_ref(),
            Some(AgentKind::ClaudeCode) => self.exact.claude_limits.as_ref(),
            _ => None,
        };
        let pct = ring_pct(limits);
        let action = if agent.ide {
            "IDE · clic: abrir en la carpeta de la sesión"
        } else {
            "clic: abrir en la sesión activa"
        };
        let tip: SharedString = format!("{} · {action}", ring_tooltip(&agent.name, limits)).into();
        let hover = c.hover;
        let launch_id = agent.id.clone();
        let color = match pct.map(RingLevel::from_pct) {
            Some(RingLevel::Critical) => c.error,
            Some(RingLevel::High) => c.warn,
            _ => c.accent,
        };
        let track = c.muted.opacity(0.25);
        div()
            .id(SharedString::from(format!("installed-{}", agent.id)))
            .relative()
            .size(px(INSTALLED_TILE))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            .on_click(cx.listener(move |this, _, window, cx| this.launch_agent(&launch_id, window, cx)))
            .when_some(pct, |this, p| this.child(ring(p, color, track)))
            .child(badge(agent.letter, agent.color, INSTALLED_BADGE, INSTALLED_BADGE / 2.))
            .into_any_element()
    }

    // ── Compacto (56 px) ────────────────────────────────────────────────────

    fn render_compact(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = &self.colors;
        let rows = self.agent_rows(cx);
        let active = self.sessions.active_id();
        let hover = c.hover;
        let agents: Vec<AnyElement> = rows
            .iter()
            .map(|r| {
                let sid = r.session_id;
                let is_active = Some(sid) == active;
                let tip: SharedString = format!("{} · {} · {}", r.name(), r.place(), r.detail()).into();
                div()
                    .id(("compact-agent", r.key() as usize))
                    .relative()
                    .w(px(PANEL_COMPACT))
                    .h(px(38.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| this.activate(sid, window, cx)))
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .when(is_active, |this| this.child(accent_bar(c.accent, 18.)))
                    .child(
                        div()
                            .relative()
                            .size(px(34.))
                            .rounded(px(9.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .map(|this| {
                                if is_active {
                                    this.bg(c.surface_2)
                                } else {
                                    this.hover(move |s| s.bg(hover))
                                }
                            })
                            .child(self.agent_badge(&r.agent, 26., 7.))
                            .child(
                                div()
                                    .absolute()
                                    .top(px(1.))
                                    .right(px(1.))
                                    .p(px(1.5))
                                    .rounded_full()
                                    .bg(c.surface)
                                    .child(state_dot(c, r.state, 7.)),
                            ),
                    )
                    .into_any_element()
            })
            .collect();
        // Pestaña remota (F6): los del servidor.
        let ids = match self.active_remote_agents() {
            Some(names) => crate::connections::remote_agent_ids(&self.catalog, &names).0,
            None => self.installed.clone(),
        };
        let tiles: Vec<AnyElement> = ids.iter().map(|id| self.installed_tile(id, cx)).collect();
        let files_on = self.panel == PanelView::Files;

        v_flex()
            .size_full()
            .items_center()
            .pt(px(8.))
            .pb(px(8.))
            .gap(px(4.))
            .child(
                icon_button("compact-search", IconName::Search, c, "Buscar agentes")
                    .size(px(34.))
                    .rounded(px(9.))
                    .on_click(cx.listener(|this, _, window, cx| this.focus_search(window, cx))),
            )
            .child(
                v_flex()
                    .id("compact-agents")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .gap(px(2.))
                    .children(agents),
            )
            .child(div().w(px(24.)).h(px(1.)).my(px(4.)).bg(c.border))
            .child(v_flex().items_center().gap(px(4.)).children(tiles))
            .child(
                icon_button("compact-files", IconName::Folder, c, "Archivos (Ctrl+Shift+E)")
                    .size(px(34.))
                    .rounded(px(9.))
                    .mt(px(4.))
                    .when(files_on, |this| this.text_color(c.text))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_files(cx))),
            )
    }
}
