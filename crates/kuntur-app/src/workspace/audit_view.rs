//! Vista "Auditoría" (§13; Ctrl+Shift+A o el botón de la barra): los comandos que ejecutaron los
//! agentes, leídos de los registros locales de Claude Code y Codex (`kuntur-audit`). Búsqueda de
//! texto, filtro por agente y período, lista virtualizada y "Exportar CSV". Esc vuelve a la
//! terminal. La lectura corre fuera del hilo de la UI, con una caché incremental por archivo.

use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jiff::tz::TimeZone;
use kuntur_audit::{AuditCache, AuditFilter, AuditRecord, Period, Sources};

use super::files_panel::empty_note;
use super::*;
use crate::usage_format::{folder_name, thousands};

/// Al reabrir la vista, se vuelve a leer si la última lectura es más vieja que esto.
const RELOAD_AFTER: Duration = Duration::from_secs(30);
const ROW_HEIGHT: f32 = 30.0;
const DATE_COL: f32 = 128.0;
const FOLDER_COL: f32 = 150.0;
const SESSION_COL: f32 = 84.0;
/// Agentes con lector de auditoría (el filtro "Todos" va primero).
const AGENTS: [AgentKind; 2] = [AgentKind::ClaudeCode, AgentKind::Codex];

pub(super) struct AuditState {
    pub(super) open: bool,
    /// Foco del buscador (mientras la vista está abierta, el teclado va ahí).
    pub(super) search_focus: FocusHandle,
    filter: AuditFilter,
    records: Option<Arc<Vec<AuditRecord>>>,
    /// Índices de `records` que pasan la búsqueda y los filtros.
    shown: Arc<Vec<usize>>,
    loaded_at: Option<Instant>,
    load: Option<Task<()>>,
    /// Caché incremental de los registros reales (se presta al hilo de lectura y vuelve).
    cache: Option<AuditCache>,
    notice: Option<SharedString>,
    scroll: UniformListScrollHandle,
}

impl AuditState {
    pub(super) fn new(cx: &mut Context<Workspace>) -> Self {
        Self {
            open: false,
            search_focus: cx.focus_handle(),
            filter: AuditFilter::default(),
            records: None,
            shown: Arc::new(Vec::new()),
            loaded_at: None,
            load: None,
            cache: Some(AuditCache::new()),
            notice: None,
            scroll: UniformListScrollHandle::new(),
        }
    }

    /// Búsqueda inicial para las pruebas de humo.
    pub(super) fn set_initial_query(&mut self, query: &str) {
        self.filter.query = query.trim().chars().filter(|c| !c.is_control()).take(80).collect();
    }

    /// Vuelve a aplicar la búsqueda y los filtros.
    fn refilter(&mut self) {
        let shown = match &self.records {
            Some(r) => kuntur_audit::filter(r, &self.filter, now_secs() as i64, &TimeZone::system()),
            None => Vec::new(),
        };
        self.shown = Arc::new(shown);
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }
}

/// Parte del nombre del archivo CSV.
fn slug(period: Period) -> &'static str {
    match period {
        Period::Today => "hoy",
        Period::Days7 => "7-dias",
        Period::Days30 => "30-dias",
        Period::Year => "1-ano",
        Period::All => "todo",
    }
}

/// Fecha local corta de una fila: "2026-11-23 14:05".
fn short_date(ts: i64, tz: &TimeZone) -> String {
    jiff::Timestamp::from_second(ts)
        .map(|t| t.to_zoned_datetime(tz.clone()).strftime("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

/// Los comandos de varias líneas se muestran en una sola.
fn one_line(command: &str) -> String {
    let mut lines = command.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next().unwrap_or("").to_string();
    if lines.next().is_some() {
        format!("{first} …")
    } else {
        first
    }
}

impl Workspace {
    // ── Estado ──────────────────────────────────────────────────────────────

    pub(super) fn toggle_audit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.audit.open {
            self.close_audit(window, cx);
        } else {
            self.open_audit(window, cx);
        }
    }

    pub(super) fn open_audit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.audit.open = true;
        self.usage.open = false;
        self.audit.notice = None;
        window.focus(&self.audit.search_focus, cx);
        let stale = self.audit.loaded_at.is_none_or(|t| t.elapsed() > RELOAD_AFTER);
        if stale && self.audit.load.is_none() {
            self.load_audit(cx);
        }
        cx.notify();
    }

    pub(super) fn close_audit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.audit.open = false;
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Lee los registros en segundo plano (solo los archivos nuevos o cambiados). Con
    /// `KUNTUR_SMOKE_AUDIT` usa datos sintéticos: las capturas nunca muestran comandos reales.
    fn load_audit(&mut self, cx: &mut Context<Self>) {
        let home = self.home.clone();
        let synthetic = std::env::var_os("KUNTUR_SMOKE_AUDIT").is_some();
        let Some(mut cache) = self.audit.cache.take() else {
            return;
        };
        self.audit.load = Some(cx.spawn(async move |this, cx| {
            let (cache, records) = cx
                .background_executor()
                .spawn(async move {
                    let records = if synthetic {
                        kuntur_audit::synthetic::records(now_secs() as i64)
                    } else {
                        if let Some(home) = home.as_deref() {
                            cache.refresh(&Sources::under_home(home));
                        }
                        cache.records()
                    };
                    (cache, records)
                })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.audit.cache = Some(cache);
                w.audit.records = Some(Arc::new(records));
                w.audit.loaded_at = Some(Instant::now());
                w.audit.load = None;
                w.audit.refilter();
                cx.notify();
            });
        }));
    }

    fn set_audit_period(&mut self, period: Period, cx: &mut Context<Self>) {
        if self.audit.filter.period != period {
            self.audit.filter.period = period;
            self.audit.notice = None;
            self.audit.refilter();
            cx.notify();
        }
    }

    fn set_audit_agent(&mut self, agent: Option<AgentKind>, cx: &mut Context<Self>) {
        if self.audit.filter.agent != agent {
            self.audit.filter.agent = agent;
            self.audit.notice = None;
            self.audit.refilter();
            cx.notify();
        }
    }

    /// Teclado con la vista abierta: lo que se escribe va al buscador. Devuelve `false` para lo que
    /// no le toca (atajos con Ctrl).
    pub(super) fn on_audit_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let ks = &ev.keystroke;
        let m = ks.modifiers;
        let key = ks.key.to_ascii_lowercase();
        // F11: ←/→ período, ↑/↓ agente, Ctrl+S exportar (el buscador no tiene cursor: las flechas
        // quedan libres).
        if let Some(forward) = crate::keynav::horizontal(&key, m.modified()) {
            let next = crate::keynav::cycle(&Period::ALL, self.audit.filter.period, forward);
            self.set_audit_period(next, cx);
            return true;
        }
        if let Some(down) = crate::keynav::vertical(&key, m.modified()) {
            let options = [None, Some(AGENTS[0]), Some(AGENTS[1])];
            let next = crate::keynav::cycle(&options, self.audit.filter.agent, down);
            self.set_audit_agent(next, cx);
            return true;
        }
        if key == "s" && m.control && !m.shift && !m.alt {
            self.export_audit(cx);
            return true;
        }
        let query = &mut self.audit.filter.query;
        match key.as_str() {
            "escape" if !m.modified() => {
                if query.is_empty() {
                    self.close_audit(window, cx);
                    return true;
                }
                query.clear();
            }
            "backspace" if m.control => query.clear(),
            "backspace" => {
                query.pop();
            }
            "v" if m.control && !m.alt && !m.shift => {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    let line = text.lines().next().unwrap_or("");
                    query.extend(line.chars().filter(|c| !c.is_control()).take(80));
                }
            }
            // AltGr llega como Ctrl+Alt: también escribe.
            _ if m.control && !m.alt => return false,
            _ => match ks.key_char.as_deref() {
                Some(text) if query.chars().count() < 80 => {
                    query.extend(text.chars().filter(|c| !c.is_control()));
                }
                Some(_) => {}
                None => return false,
            },
        }
        self.audit.refilter();
        cx.notify();
        true
    }

    /// "Exportar CSV": pide dónde guardar y escribe las filas que se ven (búsqueda y filtros).
    pub(super) fn export_audit(&mut self, cx: &mut Context<Self>) {
        let Some(records) = self.audit.records.clone() else {
            self.audit.notice = Some("Espera a que termine de leer los registros para exportar".into());
            cx.notify();
            return;
        };
        let shown = self.audit.shown.clone();
        let dir = self
            .home
            .clone()
            .map(|h| {
                let docs = h.join("Documents");
                if docs.is_dir() { docs } else { h }
            })
            .unwrap_or_else(|| PathBuf::from("."));
        let today = jiff::Timestamp::from_second(now_secs() as i64)
            .map(|t| t.to_zoned_datetime(TimeZone::system()).date().to_string())
            .unwrap_or_default();
        let name = format!("kuntur-auditoria-{}-{today}.csv", slug(self.audit.filter.period));
        let picked = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move {
                    let rows = shown.iter().filter_map(|&i| records.get(i));
                    let csv = kuntur_audit::export::to_csv(rows, &TimeZone::system());
                    std::fs::write(&path, csv).map(|_| path)
                })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.audit.notice = Some(
                    match result {
                        Ok(p) => format!("CSV guardado en {}", p.display()),
                        Err(e) => format!("No se pudo guardar el CSV: {e}"),
                    }
                    .into(),
                );
                cx.notify();
            });
        })
        .detach();
    }

    // ── Dibujo ──────────────────────────────────────────────────────────────

    pub(super) fn render_audit(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let body: AnyElement = match &self.audit.records {
            None => empty_note(c, "Leyendo registros…", "Claude Code y Codex").into_any_element(),
            Some(r) if r.is_empty() => empty_note(
                c,
                "No hay comandos registrados",
                "Kuntur lee los registros locales de Claude Code y Codex",
            )
            .into_any_element(),
            Some(_) if self.audit.shown.is_empty() => {
                let title = if self.audit.filter.query.trim().is_empty() {
                    "Sin comandos en este período"
                } else {
                    "Ningún comando coincide"
                };
                empty_note(c, title, "Prueba con otro período, agente o texto").into_any_element()
            }
            Some(_) => self.render_audit_list(cx),
        };
        v_flex()
            .id("audit-view")
            .size_full()
            .bg(c.terminal)
            .px(px(28.))
            .pt(px(20.))
            .pb(px(16.))
            .gap(px(14.))
            .child(self.render_audit_header(cx))
            .child(self.render_audit_controls(window, cx))
            .when_some(self.audit.notice.clone(), |this, notice| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.))
                        .text_color(c.muted)
                        .child(notice),
                )
            })
            .child(body)
            .into_any_element()
    }

    fn render_audit_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = &self.colors;
        let text = c.text;
        let mut subtitle = match &self.audit.records {
            Some(_) => {
                let n = self.audit.shown.len();
                format!(
                    "{} {} · {} · local",
                    thousands(n as u64),
                    if n == 1 { "comando" } else { "comandos" },
                    self.audit.filter.period.label()
                )
            }
            None => String::new(),
        };
        if self.audit.load.is_some() && self.audit.records.is_some() {
            subtitle.push_str(" · actualizando…");
        }
        let periods: Vec<AnyElement> = Period::ALL
            .into_iter()
            .map(|p| {
                let on = p == self.audit.filter.period;
                div()
                    .id(SharedString::from(format!("audit-period-{}", slug(p))))
                    .relative()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .map(|this| {
                        if on {
                            this.text_color(c.text).font_weight(FontWeight::MEDIUM)
                        } else {
                            this.text_color(c.muted).hover(move |s| s.text_color(text))
                        }
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.set_audit_period(p, cx)))
                    .child(p.label())
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
            .flex_shrink_0()
            .items_end()
            .gap(px(16.))
            .child(
                v_flex()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(c.text)
                            .child("Auditoría"),
                    )
                    .child(div().text_size(px(12.)).text_color(c.muted).child(subtitle)),
            )
            .child(div().flex_1())
            .child(h_flex().gap(px(18.)).children(periods))
            .child(
                icon_button("audit-export", IconName::Download, c, "Exportar CSV (lo que se ve)")
                    .on_click(cx.listener(|this, _, _, cx| this.export_audit(cx))),
            )
    }

    /// Buscador y filtro por agente.
    fn render_audit_controls(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = &self.colors;
        let focused = self.audit.search_focus.is_focused(window);
        let accent = c.accent;
        let caret = move || div().w(px(1.5)).h(px(14.)).flex_shrink_0().bg(accent);
        let query = self.audit.filter.query.clone();
        let text = c.text;
        let search = h_flex()
            .id("audit-search")
            .track_focus(&self.audit.search_focus)
            .w(px(360.))
            .max_w(relative(0.5))
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
                window.focus(&this.audit.search_focus, cx);
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
                            this.when(focused, |this| this.child(caret())).child(
                                div()
                                    .ml(px(1.))
                                    .text_color(c.muted)
                                    .child("Buscar en comandos, carpetas y sesiones"),
                            )
                        } else {
                            this.child(div().text_color(c.text).child(query.clone()))
                                .when(focused, |this| this.child(caret()))
                        }
                    }),
            );
        let agent_chip = |id: &'static str, label: &'static str, agent: Option<AgentKind>| {
            let on = self.audit.filter.agent == agent;
            div()
                .id(id)
                .relative()
                .h(px(28.))
                .flex()
                .items_center()
                .cursor_pointer()
                .text_size(px(12.5))
                .map(|this| {
                    if on {
                        this.text_color(c.text).font_weight(FontWeight::MEDIUM)
                    } else {
                        this.text_color(c.muted).hover(move |s| s.text_color(text))
                    }
                })
                .on_click(cx.listener(move |this, _, _, cx| this.set_audit_agent(agent, cx)))
                .child(label)
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
        };
        let mut chips = vec![agent_chip("audit-agent-all", "Todos los agentes", None).into_any_element()];
        chips.extend(AGENTS.iter().map(|&a| {
            agent_chip(
                if a == AgentKind::Codex {
                    "audit-agent-codex"
                } else {
                    "audit-agent-claude"
                },
                a.name(),
                Some(a),
            )
            .into_any_element()
        }));
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(18.))
            .child(search)
            .child(h_flex().gap(px(16.)).children(chips))
    }

    /// Encabezado de columnas + lista virtualizada (solo se dibujan las filas visibles).
    fn render_audit_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let head = |label: &'static str| div().text_size(px(11.)).text_color(c.muted).child(label);
        let header = h_flex()
            .flex_shrink_0()
            .h(px(24.))
            .px(px(10.))
            .gap(px(12.))
            .items_center()
            .border_b_1()
            .border_color(c.border)
            .child(head("Fecha").w(px(DATE_COL)).flex_shrink_0())
            .child(div().w(px(20.)).flex_shrink_0())
            .child(head("Comando").flex_1().min_w_0())
            .child(head("Carpeta").w(px(FOLDER_COL)).flex_shrink_0())
            .child(head("Sesión").w(px(SESSION_COL)).flex_shrink_0());
        let count = self.audit.shown.len();
        let list = uniform_list(
            "audit-list",
            count,
            cx.processor(|this, range: Range<usize>, _window, cx| this.render_audit_rows(range, cx)),
        )
        .track_scroll(&self.audit.scroll)
        .w_full()
        .flex_1()
        .min_h_0();
        v_flex()
            .flex_1()
            .min_h_0()
            .rounded(px(8.))
            .border_1()
            .border_color(c.border)
            .overflow_hidden()
            .child(header)
            .child(list)
            .into_any_element()
    }

    fn render_audit_rows(&mut self, range: Range<usize>, _cx: &mut Context<Self>) -> Vec<AnyElement> {
        let c = &self.colors;
        let Some(records) = self.audit.records.clone() else {
            return Vec::new();
        };
        let tz = TimeZone::system();
        let hover = c.hover;
        range
            .filter_map(|row| {
                let r = records.get(*self.audit.shown.get(row)?)?;
                let folder = r.folder.as_deref().map(folder_name).unwrap_or("").to_string();
                let tip: SharedString = format!(
                    "{} · {} · {} · {}{}\n{}",
                    r.agent.name(),
                    r.tool,
                    r.host,
                    r.folder.as_deref().unwrap_or("carpeta desconocida"),
                    if r.session.is_empty() {
                        String::new()
                    } else {
                        format!(" · sesión {}", r.session)
                    },
                    r.command
                )
                .into();
                let session: String = r.session.chars().take(8).collect();
                Some(
                    h_flex()
                        .id(("audit-row", row))
                        .w_full()
                        .h(px(ROW_HEIGHT))
                        .px(px(10.))
                        .gap(px(12.))
                        .items_center()
                        .text_size(px(12.))
                        .hover(move |s| s.bg(hover))
                        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                        .child(
                            div()
                                .w(px(DATE_COL))
                                .flex_shrink_0()
                                .text_color(c.muted)
                                .font_family(self.mono_font.clone())
                                .text_size(px(11.5))
                                .child(short_date(r.ts, &tz)),
                        )
                        .child(self.usage_badge(r.agent, 20., 5.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .font_family(self.mono_font.clone())
                                .text_color(c.text)
                                .child(one_line(&r.command)),
                        )
                        .child(
                            div()
                                .w(px(FOLDER_COL))
                                .flex_shrink_0()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_color(c.muted)
                                .child(folder),
                        )
                        .child(
                            div()
                                .w(px(SESSION_COL))
                                .flex_shrink_0()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_color(c.muted)
                                .font_family(self.mono_font.clone())
                                .text_size(px(11.))
                                .child(session),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    // Sin `super::*`: GPUI exporta su propio `test`, que taparía el de Rust.
    use super::one_line;

    #[test]
    fn comandos_de_varias_lineas_en_una() {
        assert_eq!(one_line("cargo test"), "cargo test");
        assert_eq!(one_line("\n  git add -A\n git commit\n"), "git add -A …");
        assert_eq!(one_line(""), "");
    }
}
