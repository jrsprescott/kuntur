//! Vista "Uso de agentes" (Ctrl+Shift+U o el botón de la barra): ocupa el área central con el
//! selector Hoy / 7 días / 30 días / 1 año / Todo, cuatro tarjetas, tokens por hora/día/semana/mes,
//! cuándo se trabaja (día × hora), agentes, modelos y las tareas que más tokens usaron. Esc vuelve a
//! la terminal. La lectura del historial y los cálculos corren fuera del hilo de la UI.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jiff::tz::TimeZone;
use kuntur_usage::chart::bars;
use kuntur_usage::pricing::PriceTable;
use kuntur_usage::store::{Sources, UsageStore};
use kuntur_usage::{Period, Summary, UsageRecord, pct_change, summarize};

use super::files_panel::empty_note;
use super::side_panel::brand;
use super::*;
use crate::i18n::{T, t};
use crate::usage_format::{
    change, compact, cost_detail, duration, folder_name, heat_levels, no_change_note, providers, range, share,
    thousands, usd, versus,
};

/// Al reabrir la vista, se vuelve a leer el historial si la última lectura es más vieja que esto.
const RELOAD_AFTER: Duration = Duration::from_secs(60);
const CHART_HEIGHT: f32 = 150.0;
/// Etiquetas como máximo bajo el gráfico (se muestran cada n barras).
const MAX_LABELS: usize = 12;
/// Filas de la tabla de modelos.
const MAX_MODELS: usize = 8;
const WEEKDAYS: [&str; 7] = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"];

pub(super) struct UsageState {
    pub(super) open: bool,
    period: Period,
    records: Option<Arc<Vec<UsageRecord>>>,
    loaded_at: Option<Instant>,
    summary: Option<Arc<Summary>>,
    load: Option<Task<()>>,
    compute: Option<Task<()>>,
    /// Aviso bajo la cabecera ("CSV guardado en …").
    notice: Option<SharedString>,
}

impl Default for UsageState {
    fn default() -> Self {
        Self {
            open: false,
            period: Period::Days30,
            records: None,
            loaded_at: None,
            summary: None,
            load: None,
            compute: None,
            notice: None,
        }
    }
}

impl UsageState {
    /// Período inicial para las pruebas de humo: "hoy", "7", "30", "ano" o "todo".
    pub(super) fn set_initial_period(&mut self, key: &str) {
        self.period = match key {
            "hoy" => Period::Today,
            "7" => Period::Days7,
            "ano" | "año" => Period::Year,
            "todo" => Period::All,
            _ => Period::Days30,
        };
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

/// Color de cada proveedor en la barra de participación.
fn provider_color(provider: &str, c: &UiColors) -> Hsla {
    match provider {
        "Anthropic" => brand(0xd97757),
        "OpenAI" => brand(0x10a37f),
        "Google" => brand(0x4285f4),
        "DeepSeek" => brand(0x4d6bfe),
        "Qwen" => brand(0x8b5cf6),
        // Ajustes v0.2.1: modelos en la PC (LM Studio, Ollama…).
        "Local" => c.ok,
        _ => c.muted,
    }
}

/// Etiqueta chica junto a un nombre ("no instalado", "local").
fn tag(text: &'static str, c: &UiColors) -> Div {
    div()
        .px(px(5.))
        .rounded(px(4.))
        .bg(c.hover)
        .text_size(px(10.5))
        .text_color(c.muted)
        .whitespace_nowrap()
        .child(text)
}

fn agent_color(kind: AgentKind, c: &UiColors) -> Hsla {
    kuntur_agents::brand_color(kind).map_or(c.muted, brand)
}

/// Carpeta de datos de Kuntur (`%APPDATA%\Kuntur` en Windows; ver `kuntur_config::dirs`): ahí van
/// `usage.db` y `pricing.toml`.
fn data_dir() -> Option<PathBuf> {
    kuntur_config::dirs::config_dir()
}

/// Tabla incluida + la del usuario encima. Si la del usuario tiene un error, se usan los precios
/// incluidos y se devuelve el aviso.
fn load_prices() -> (PriceTable, Option<String>) {
    let builtin = PriceTable::builtin();
    let Some(path) = data_dir().map(|d| d.join("pricing.toml")).filter(|p| p.is_file()) else {
        return (builtin, None);
    };
    match std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|text| PriceTable::parse(&text))
    {
        Ok(user) => (builtin.with_overrides(user), None),
        Err(e) => (
            builtin,
            Some(format!(
                "pricing.toml tiene un error ({e}): se usan los precios incluidos"
            )),
        ),
    }
}

/// Historial real por el almacén incremental (`usage.db`): solo se releen los archivos que
/// cambiaron. Si la base no se puede usar, se lee todo directamente.
fn load_real(home: &Path) -> Vec<UsageRecord> {
    let sources = Sources::under_home(home);
    if let Some(mut store) = data_dir().and_then(|d| UsageStore::open(&d.join("usage.db")).ok())
        && store.import(&sources).is_ok()
        && let Ok(records) = store.records()
    {
        return records;
    }
    kuntur_usage::load_local(home)
}

impl Workspace {
    // ── Estado ──────────────────────────────────────────────────────────────

    pub(super) fn toggle_usage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.usage.open {
            self.close_usage(window, cx);
        } else {
            self.open_usage(window, cx);
        }
    }

    pub(super) fn open_usage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.usage.open = true;
        self.audit.open = false;
        self.usage.notice = None;
        window.focus(&self.focus, cx);
        let stale = self.usage.loaded_at.is_none_or(|t| t.elapsed() > RELOAD_AFTER);
        if stale && self.usage.load.is_none() {
            self.load_usage(cx);
        }
        cx.notify();
    }

    pub(super) fn close_usage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.usage.open = false;
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Lee el historial en segundo plano (el real, por el almacén incremental) y le pone el costo
    /// estimado con la tabla de precios. Con `KUNTUR_SMOKE_USAGE` usa datos sintéticos: las
    /// capturas de prueba nunca muestran el historial real.
    fn load_usage(&mut self, cx: &mut Context<Self>) {
        let home = self.home.clone();
        let synthetic = std::env::var_os("KUNTUR_SMOKE_USAGE").is_some();
        self.usage.load = Some(cx.spawn(async move |this, cx| {
            let (records, notice) = cx
                .background_executor()
                .spawn(async move {
                    let (prices, notice) = if synthetic {
                        (PriceTable::builtin(), None)
                    } else {
                        load_prices()
                    };
                    let mut records = if synthetic {
                        kuntur_usage::synthetic::records(now_secs() as i64, &TimeZone::system())
                    } else {
                        home.as_deref().map(load_real).unwrap_or_default()
                    };
                    prices.apply(&mut records);
                    (records, notice)
                })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.usage.records = Some(Arc::new(records));
                w.usage.loaded_at = Some(Instant::now());
                w.usage.load = None;
                if notice.is_some() {
                    w.usage.notice = notice.map(Into::into);
                }
                w.recompute_usage(cx);
            });
        }));
    }

    /// "Editar precios": crea `%APPDATA%\Kuntur\pricing.toml` (copia de la tabla incluida) si no
    /// existe y lo abre con el editor de Windows. Los cambios se ven al volver a abrir la vista.
    pub(super) fn edit_prices(&mut self, cx: &mut Context<Self>) {
        let Some(path) = data_dir().map(|d| d.join("pricing.toml")) else {
            self.usage.notice = Some("No se encontró la carpeta de datos de Kuntur".into());
            cx.notify();
            return;
        };
        if !path.exists() {
            let written = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|_| std::fs::write(&path, kuntur_usage::pricing::BUILTIN));
            if let Err(e) = written {
                self.usage.notice = Some(format!("No se pudo crear pricing.toml: {e}").into());
                cx.notify();
                return;
            }
        }
        cx.open_with_system(&path);
        // Los precios se aplican al cargar: la próxima apertura vuelve a cargar.
        self.usage.loaded_at = None;
        self.usage.notice = Some("Se abrió pricing.toml: guarda los cambios y vuelve a abrir Uso de agentes".into());
        cx.notify();
    }

    /// Recalcula el resumen del período elegido (fuera del hilo de la UI); mientras tanto se ve el
    /// anterior.
    fn recompute_usage(&mut self, cx: &mut Context<Self>) {
        let Some(records) = self.usage.records.clone() else {
            return;
        };
        let period = self.usage.period;
        self.usage.compute = Some(cx.spawn(async move |this, cx| {
            let summary = cx
                .background_executor()
                .spawn(async move { summarize(&records, period, now_secs() as i64, &TimeZone::system()) })
                .await;
            let _ = this.update(cx, |w, cx| {
                if w.usage.period == period {
                    w.usage.summary = Some(Arc::new(summary));
                }
                w.usage.compute = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn set_usage_period(&mut self, period: Period, cx: &mut Context<Self>) {
        if self.usage.period != period {
            self.usage.period = period;
            self.usage.notice = None;
            self.recompute_usage(cx);
        }
    }

    /// Teclado con la vista abierta (F11): ←/→ cambian el período, Ctrl+S exporta el CSV, Esc
    /// cierra. Devuelve `false` para lo demás (atajos globales).
    pub(super) fn on_usage_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let m = ev.keystroke.modifiers;
        let key = ev.keystroke.key.to_ascii_lowercase();
        if key == "escape" && !m.modified() {
            self.close_usage(window, cx);
        } else if let Some(forward) = crate::keynav::horizontal(&key, m.modified()) {
            let next = crate::keynav::cycle(&Period::ALL, self.usage.period, forward);
            self.set_usage_period(next, cx);
        } else if key == "s" && m.control && !m.shift && !m.alt {
            self.export_usage(cx);
        } else {
            return false;
        }
        true
    }

    /// "Exportar CSV": pide dónde guardar y escribe los registros del período elegido.
    pub(super) fn export_usage(&mut self, cx: &mut Context<Self>) {
        let (Some(records), Some(summary)) = (self.usage.records.clone(), self.usage.summary.clone()) else {
            self.usage.notice = Some("Espera a que termine de leer el historial para exportar".into());
            cx.notify();
            return;
        };
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
        let name = format!("kuntur-uso-{}-{today}.csv", slug(self.usage.period));
        let picked = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else {
                return;
            };
            let (start, end) = (summary.start, summary.end);
            let result = cx
                .background_executor()
                .spawn(async move {
                    let csv = kuntur_usage::export::to_csv(&records, start, end, &TimeZone::system());
                    std::fs::write(&path, csv).map(|_| path)
                })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.usage.notice = Some(
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

    pub(super) fn render_usage(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let body: AnyElement = match &self.usage.summary {
            None => empty_note(c, "Leyendo historial…", "Claude Code, Codex y OpenCode").into_any_element(),
            Some(s) if s.totals.turns == 0 => empty_note(
                c,
                "Sin uso en este período",
                "Kuntur lee el historial local de Claude Code, Codex y OpenCode",
            )
            .into_any_element(),
            Some(s) => self.render_usage_body(s, cx),
        };
        v_flex()
            .id("usage-view")
            .size_full()
            .overflow_y_scroll()
            .bg(c.terminal)
            .px(px(28.))
            .pt(px(20.))
            .pb(px(28.))
            .gap(px(18.))
            .child(self.render_usage_header(cx))
            .when_some(self.usage.notice.clone(), |this, notice| {
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

    fn render_usage_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = &self.colors;
        let text = c.text;
        let mut subtitle = self
            .usage
            .summary
            .as_ref()
            .map(|s| range(s, &TimeZone::system()))
            .unwrap_or_default();
        if self.usage.load.is_some() && self.usage.summary.is_some() {
            subtitle.push_str(" · actualizando…");
        }
        let periods: Vec<AnyElement> = Period::ALL
            .into_iter()
            .map(|p| {
                let on = p == self.usage.period;
                div()
                    .id(SharedString::from(format!("period-{}", slug(p))))
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
                    .on_click(cx.listener(move |this, _, _, cx| this.set_usage_period(p, cx)))
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
                            .child("Uso de agentes"),
                    )
                    .child(div().text_size(px(12.)).text_color(c.muted).child(subtitle)),
            )
            .child(div().flex_1())
            .child(h_flex().gap(px(18.)).children(periods))
            .child(
                icon_button("usage-export", IconName::Download, c, "Exportar CSV del período")
                    .on_click(cx.listener(|this, _, _, cx| this.export_usage(cx))),
            )
    }

    fn render_usage_body(&self, s: &Summary, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let t = &s.totals;
        let prev = s.previous.as_ref();
        let vs = versus(self.usage.period);
        let note = no_change_note(self.usage.period, prev.is_some());
        let delta = |cur: u64, old: Option<u64>| old.and_then(|o| change(pct_change(cur, o)));
        let cache_line = t.cache_share().map(|p| format!("{} % desde caché", p.round() as u64));
        // Mismo alto para todas (la de entrada y la de costo llevan una línea más).
        let cards = h_flex()
            .flex_shrink_0()
            .items_stretch()
            .gap(px(12.))
            .child(card(
                c,
                "card-input",
                "Tokens de entrada",
                compact(t.input_with_cache()),
                thousands(t.input_with_cache()),
                delta(t.input_with_cache(), prev.map(|p| p.input_with_cache())),
                (vs, note),
                cache_line,
            ))
            .child(card(
                c,
                "card-output",
                "Tokens de salida",
                compact(t.output),
                thousands(t.output),
                delta(t.output, prev.map(|p| p.output)),
                (vs, note),
                None,
            ))
            .child(card(
                c,
                "card-turns",
                "Turnos",
                thousands(t.turns),
                format!("{} sesiones", thousands(t.sessions)),
                delta(t.turns, prev.map(|p| p.turns)),
                (vs, note),
                None,
            ))
            .child(card(
                c,
                "card-time",
                "Tiempo de agente",
                duration(t.agent_time_secs),
                "Suma de los intervalos entre turnos de hasta 5 min".into(),
                delta(t.agent_time_secs, prev.map(|p| p.agent_time_secs)),
                (vs, note),
                None,
            ))
            .child(card(
                c,
                "card-cost",
                "Costo estimado",
                usd(t.cost_micros),
                cost_detail(t),
                // En el costo, bajar es lo bueno: verde al bajar, rojo al subir.
                delta(t.cost_micros, prev.map(|p| p.cost_micros)).map(|(text, up)| (text, !up)),
                (vs, note),
                Some(if t.unpriced_turns > 0 {
                    format!(
                        "{} {} sin precio",
                        thousands(t.unpriced_turns),
                        if t.unpriced_turns == 1 { "turno" } else { "turnos" }
                    )
                } else {
                    "precio de API".to_string()
                }),
            ));

        // Resumen en una línea (§10): agente más usado · agentes usados · sesiones.
        let top_agent = s.agents.first().map(|a| a.agent.name()).unwrap_or("—");
        let agents_used = s.agents.len();
        let facts = format!(
            "Agente más usado: {top_agent} · {agents_used} {} · {} {}",
            if agents_used == 1 {
                "agente usado"
            } else {
                "agentes usados"
            },
            thousands(t.sessions),
            if t.sessions == 1 { "sesión" } else { "sesiones" },
        );

        v_flex()
            .flex_shrink_0()
            .gap(px(18.))
            .child(cards)
            .child(div().text_size(px(12.5)).text_color(c.muted).child(facts))
            .child(self.render_chart(s, cx))
            .child(
                h_flex()
                    .items_start()
                    .gap(px(18.))
                    .child(div().flex_1().min_w_0().child(self.render_heatmap(s)))
                    .child(div().w(px(320.)).flex_shrink_0().child(self.render_agents(s))),
            )
            .child(
                h_flex()
                    .items_start()
                    .gap(px(18.))
                    .child(div().flex_1().min_w_0().child(self.render_models(s)))
                    .child(div().flex_1().min_w_0().child(self.render_top_tasks(s))),
            )
            .into_any_element()
    }

    fn render_chart(&self, s: &Summary, _cx: &mut Context<Self>) -> Div {
        let c = &self.colors;
        let (bucket, bars) = bars(s, self.usage.period);
        let max = bars.iter().map(|b| b.input + b.output).max().unwrap_or(0).max(1);
        let step = bars.len().div_ceil(MAX_LABELS).max(1);
        let gap = match bars.len() {
            0..=31 => 4.,
            32..=60 => 2.,
            _ => 1.,
        };
        let (input_color, output_color, hover) = (c.accent, c.accent.opacity(0.4), c.hover);
        let height = |v: u64| {
            if v == 0 {
                0.
            } else {
                (v as f32 / max as f32 * CHART_HEIGHT).max(1.)
            }
        };
        let columns: Vec<AnyElement> = bars
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let tip: SharedString = format!(
                    "{} · entrada {} · salida {}",
                    b.title,
                    thousands(b.input),
                    thousands(b.output)
                )
                .into();
                v_flex()
                    .id(("usage-bar", i))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .justify_end()
                    .rounded_t(px(2.))
                    .hover(move |st| st.bg(hover))
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .child(div().h(px(height(b.output))).rounded_t(px(2.)).bg(output_color))
                    .child(div().h(px(height(b.input))).bg(input_color))
                    .into_any_element()
            })
            .collect();
        let labels: Vec<AnyElement> = bars
            .iter()
            .enumerate()
            .map(|(i, b)| {
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .text_size(px(10.5))
                    .text_color(c.muted)
                    .when(i % step == 0, |this| this.child(b.label.clone()))
                    .into_any_element()
            })
            .collect();
        let legend = h_flex()
            .gap(px(14.))
            .text_size(px(11.5))
            .text_color(c.muted)
            .child(swatch(input_color, "Entrada"))
            .child(swatch(output_color, "Salida"));
        section(c, bucket.title(), Some(legend.into_any_element()))
            .child(
                h_flex()
                    .h(px(CHART_HEIGHT))
                    .items_end()
                    .gap(px(gap))
                    .border_b_1()
                    .border_color(c.border)
                    .children(columns),
            )
            .child(h_flex().gap(px(gap)).children(labels))
    }

    fn render_heatmap(&self, s: &Summary) -> Div {
        let c = &self.colors;
        let levels = heat_levels(&s.heatmap);
        let level_color = |l: u8| match l {
            0 => c.hover,
            1 => c.accent.opacity(0.25),
            2 => c.accent.opacity(0.45),
            3 => c.accent.opacity(0.7),
            _ => c.accent,
        };
        let rows: Vec<AnyElement> = (0..7)
            .map(|d| {
                let cells: Vec<AnyElement> = (0..24)
                    .map(|h| {
                        let n = s.heatmap[d][h];
                        let tip: SharedString = format!(
                            "{} {h:02}:00 · {} {}",
                            WEEKDAYS[d],
                            thousands(n),
                            if n == 1 { "turno" } else { "turnos" }
                        )
                        .into();
                        div()
                            .id(("heat", d * 24 + h))
                            .flex_1()
                            .h(px(14.))
                            .rounded(px(3.))
                            .bg(level_color(levels[d][h]))
                            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                            .into_any_element()
                    })
                    .collect();
                h_flex()
                    .gap(px(3.))
                    .items_center()
                    .child(
                        div()
                            .w(px(30.))
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(c.muted)
                            .child(WEEKDAYS[d]),
                    )
                    .children(cells)
                    .into_any_element()
            })
            .collect();
        let axis = h_flex()
            .gap(px(3.))
            .child(div().w(px(30.)).flex_shrink_0())
            .children((0..24).map(|h| {
                div()
                    .flex_1()
                    .whitespace_nowrap()
                    .text_size(px(10.5))
                    .text_color(c.muted)
                    .when(h % 6 == 0, |this| this.child(format!("{h} h")))
            }));
        section(c, "Cuándo trabajan", None)
            .child(v_flex().gap(px(3.)).children(rows))
            .child(axis)
    }

    fn render_agents(&self, s: &Summary) -> Div {
        let c = &self.colors;
        let total: u64 = s.agents.iter().map(|a| a.total).sum();
        let track = c.hover;
        let rows = s.agents.iter().map(|a| {
            let (kind, tokens) = (a.agent, a.total);
            let pct = share(tokens, total);
            let money = if a.cost_micros > 0 {
                format!(" · {}", usd(a.cost_micros))
            } else {
                String::new()
            };
            let color = agent_color(kind, c);
            // Ajustes v0.2.1: su historial se conserva aunque ya no esté instalado; se marca. Mientras
            // no llegó la primera lectura de instalados (lista vacía) no se marca nada.
            let gone = !self.installed.is_empty() && !self.usage_installed(kind);
            h_flex()
                .gap(px(10.))
                .items_center()
                .child(self.usage_badge(kind, 22., 6.).when(gone, |b| b.opacity(0.45)))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(5.))
                        .child(
                            h_flex()
                                .justify_between()
                                .text_size(px(12.5))
                                .child(
                                    h_flex()
                                        .gap(px(6.))
                                        .items_center()
                                        .child(div().text_color(c.text).child(kind.name()))
                                        .when(gone, |this| this.child(tag(t(T::UsageNotInstalled), c))),
                                )
                                .child(
                                    div()
                                        .text_color(c.muted)
                                        .child(format!("{}{money} · {pct} %", compact(tokens))),
                                ),
                        )
                        .child(
                            div()
                                .h(px(4.))
                                .rounded(px(2.))
                                .bg(track)
                                .child(div().h_full().rounded(px(2.)).w(relative(pct as f32 / 100.)).bg(color)),
                        ),
                )
        });
        section(c, "Agentes", None).child(v_flex().gap(px(12.)).children(rows))
    }

    fn render_models(&self, s: &Summary) -> Div {
        let c = &self.colors;
        let by_provider = providers(&s.models);
        let total: u64 = by_provider.iter().map(|p| p.1).sum();
        let segments = by_provider.iter().map(|&(p, v)| {
            div()
                .h_full()
                .w(relative(v as f32 / total.max(1) as f32))
                .bg(provider_color(p, c))
        });
        let legend = by_provider.iter().map(|&(p, v)| {
            h_flex()
                .gap(px(6.))
                .items_center()
                .child(div().size(px(8.)).rounded_full().bg(provider_color(p, c)))
                .child(format!("{p} {} %", share(v, total)))
        });
        let num = |text: String, width: f32| div().w(px(width)).flex_shrink_0().flex().justify_end().child(text);
        let header = h_flex()
            .text_size(px(11.5))
            .text_color(c.muted)
            .pb(px(4.))
            .border_b_1()
            .border_color(c.border)
            .child(div().flex_1().child("Modelo"))
            .child(num("Entrada".into(), 76.))
            .child(num("Salida".into(), 76.))
            .child(num("Turnos".into(), 60.))
            .child(num("Costo".into(), 84.));
        let rows = s.models.iter().take(MAX_MODELS).map(|m| {
            // Ajustes v0.2.1: los modelos servidos en la PC (LM Studio, Ollama…) cuestan cero.
            let local = kuntur_usage::is_local_model(&m.model);
            h_flex()
                .text_size(px(12.5))
                .py(px(3.))
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(6.))
                        .items_center()
                        .child(
                            div()
                                .min_w_0()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_color(c.text)
                                .child(m.model.clone()),
                        )
                        .when(local, |this| this.child(tag(t(T::UsageLocal), c))),
                )
                .child(num(compact(m.input), 76.).text_color(c.muted))
                .child(num(compact(m.output), 76.).text_color(c.muted))
                .child(num(thousands(m.turns), 60.).text_color(c.muted))
                .child(num(m.cost_micros.map_or_else(|| "sin precio".to_string(), usd), 84.).text_color(c.muted))
        });
        let hidden = s.models.len().saturating_sub(MAX_MODELS);
        section(c, "Modelos", None)
            .child(
                h_flex()
                    .h(px(8.))
                    .w_full()
                    .rounded(px(4.))
                    .overflow_hidden()
                    .gap(px(2.))
                    .children(segments),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .gap(px(14.))
                    .text_size(px(11.5))
                    .text_color(c.muted)
                    .children(legend),
            )
            .child(v_flex().child(header).children(rows))
            .when(hidden > 0, |this| {
                this.child(div().text_size(px(11.5)).text_color(c.muted).child(format!(
                    "y {hidden} {} más",
                    if hidden == 1 { "modelo" } else { "modelos" }
                )))
            })
    }

    fn render_top_tasks(&self, s: &Summary) -> Div {
        let c = &self.colors;
        let rows = s.top_tasks.iter().enumerate().map(|(i, task)| {
            let mut details = Vec::new();
            if let Some(p) = task.project.as_deref() {
                details.push(folder_name(p).to_string());
            }
            if !task.model.is_empty() {
                details.push(task.model.clone());
            }
            details.push(task.agent.name().to_string());
            let exact: SharedString = format!("{} tokens", thousands(task.total)).into();
            h_flex()
                .gap(px(10.))
                .items_center()
                .py(px(5.))
                .child(
                    div()
                        .w(px(16.))
                        .flex_shrink_0()
                        .text_size(px(11.5))
                        .text_color(c.muted)
                        .child((i + 1).to_string()),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_size(px(12.5))
                                .text_color(c.text)
                                .child(task.title.clone()),
                        )
                        .child(
                            div()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_size(px(11.5))
                                .text_color(c.muted)
                                .child(details.join(" · ")),
                        ),
                )
                .child(
                    div()
                        .id(("task-total", i))
                        .flex_shrink_0()
                        .text_size(px(12.5))
                        .text_color(c.text)
                        .tooltip(move |window, cx| Tooltip::new(exact.clone()).build(window, cx))
                        .child(compact(task.total)),
                )
        });
        section(c, "Tareas que más tokens usaron", None).child(v_flex().children(rows))
    }
}

/// Recuadro de una sección: título (y algo a la derecha, como una leyenda) y su contenido.
fn section(c: &UiColors, title: &'static str, right: Option<AnyElement>) -> Div {
    v_flex()
        .flex_shrink_0()
        .gap(px(12.))
        .p(px(16.))
        .rounded(px(10.))
        .bg(c.surface_2)
        .border_1()
        .border_color(c.border)
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(c.text)
                        .child(title),
                )
                .children(right),
        )
}

/// Tarjeta de un total: nombre, valor grande (tooltip con el detalle), % de cambio contra el
/// período anterior (verde sube, rojo baja) y una línea extra opcional.
#[allow(clippy::too_many_arguments)]
fn card(
    c: &UiColors,
    id: &'static str,
    label: &'static str,
    value: String,
    detail: String,
    delta: Option<(String, bool)>,
    (vs, note): (&'static str, &'static str),
    extra: Option<String>,
) -> impl IntoElement {
    let detail: SharedString = detail.into();
    v_flex()
        .id(id)
        .flex_1()
        .min_w_0()
        .gap(px(4.))
        .p(px(14.))
        .rounded(px(10.))
        .bg(c.surface_2)
        .border_1()
        .border_color(c.border)
        .tooltip(move |window, cx| Tooltip::new(detail.clone()).build(window, cx))
        .child(div().text_size(px(12.)).text_color(c.muted).child(label))
        .child(
            div()
                .text_size(px(24.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c.text)
                .child(value),
        )
        .child(
            h_flex()
                .gap(px(6.))
                .text_size(px(11.5))
                .text_color(c.muted)
                .map(|this| match delta {
                    Some((text, up)) => this
                        .child(div().text_color(if up { c.ok } else { c.error }).child(text))
                        .child(vs),
                    None => this.child(note),
                }),
        )
        .when_some(extra, |this, extra| {
            this.child(div().text_size(px(11.5)).text_color(c.muted).child(extra))
        })
}

/// Cuadradito de color + texto (leyenda).
fn swatch(color: Hsla, label: &'static str) -> impl IntoElement {
    h_flex()
        .gap(px(6.))
        .items_center()
        .child(div().size(px(9.)).rounded(px(2.)).bg(color))
        .child(label)
}
