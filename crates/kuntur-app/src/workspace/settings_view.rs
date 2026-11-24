//! Panel "Configuración" (ajustes v0.2; botón ⚙, Ctrl+, o la paleta): apariencia (temas como
//! círculos con sus colores y el nombre debajo, tamaño de letra, panel lateral, idioma) y
//! notificaciones (cuáles, cada cuánto y solo en segundo plano). Cada cambio se guarda al instante.
//!
//! Teclado: ↑/↓ (o Tab) eligen la fila, ←/→ cambian el valor, Enter o Espacio alternan, Esc cierra.

use kuntur_config::{Language, NotificationSettings};

use super::*;
use crate::i18n::{self, T, t};
use crate::theme_bridge::to_hsla;

const PANEL_W: f32 = 440.0;
/// Opciones del tiempo mínimo entre avisos (minutos; 0 = sin límite).
const INTERVALS: [u32; 5] = [0, 1, 5, 15, 30];
const LANGUAGES: [Language; 3] = [Language::Auto, Language::Es, Language::En];
const SWATCH: f32 = 44.0;

/// Filas del panel, en orden (las que se eligen con el teclado).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Row {
    Theme,
    FontSize,
    Sidebar,
    Marquee,
    Language,
    Notify,
    Waiting,
    Permission,
    Finished,
    Interval,
    Background,
    UpdatesAuto,
    UpdatesNow,
}

impl Row {
    const ALL: [Row; 13] = [
        Row::Theme,
        Row::FontSize,
        Row::Sidebar,
        Row::Marquee,
        Row::Language,
        Row::Notify,
        Row::Waiting,
        Row::Permission,
        Row::Finished,
        Row::Interval,
        Row::Background,
        Row::UpdatesAuto,
        Row::UpdatesNow,
    ];

    fn index(self) -> usize {
        Row::ALL.iter().position(|&r| r == self).unwrap_or(0)
    }

    /// Depende del interruptor general de notificaciones.
    fn needs_notify(self) -> bool {
        matches!(
            self,
            Row::Waiting | Row::Permission | Row::Finished | Row::Interval | Row::Background
        )
    }
}

/// Muestra de un tema para su círculo.
#[derive(Clone)]
struct Swatch {
    id: ThemeId,
    name: SharedString,
    fill: Background,
    accent: Hsla,
    edge: Hsla,
}

#[derive(Default)]
pub(super) struct SettingsPanel {
    pub(super) open: bool,
    selected: usize,
    /// Se usó el teclado: la fila elegida se marca.
    keyboard: bool,
    swatches: Vec<Swatch>,
}

/// Siguiente / anterior intervalo de la lista (un valor escrito a mano salta al vecino de la lista).
fn step_interval(current: u32, forward: bool) -> u32 {
    if forward {
        INTERVALS
            .iter()
            .copied()
            .find(|&v| v > current)
            .unwrap_or(INTERVALS[INTERVALS.len() - 1])
    } else {
        INTERVALS
            .iter()
            .rev()
            .copied()
            .find(|&v| v < current)
            .unwrap_or(INTERVALS[0])
    }
}

fn interval_label(min: u32) -> SharedString {
    if min == 0 {
        t(T::IntervalAlways).into()
    } else {
        format!("{min} min").into()
    }
}

fn language_label(lang: Language) -> &'static str {
    match lang {
        Language::Auto => t(T::LanguageAuto),
        Language::Es => "Español",
        Language::En => "English",
    }
}

impl Workspace {
    // ── Estado ──────────────────────────────────────────────────────────────

    pub(super) fn toggle_settings(&mut self, row: Option<Row>, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_panel.open {
            self.close_settings(window, cx);
        } else {
            self.open_settings(row, window, cx);
        }
    }

    pub(super) fn open_settings(&mut self, row: Option<Row>, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        self.connections.open = false;
        self.mcp.open = false;
        let p = &mut self.settings_panel;
        p.open = true;
        p.keyboard = false;
        p.selected = row.unwrap_or(Row::Theme).index();
        if p.swatches.is_empty() {
            p.swatches = ThemeId::ALL
                .iter()
                .map(|&id| {
                    let theme = builtin(id);
                    let colors = UiColors::from_theme(&theme);
                    let name = theme.name.split('·').next().unwrap_or(&theme.name).trim().to_string();
                    Swatch {
                        id,
                        name: name.into(),
                        fill: colors.window,
                        accent: to_hsla(theme.ui.accent),
                        edge: colors.border,
                    }
                })
                .collect();
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_panel.open = false;
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Cambia el valor de una fila: `forward` = → (o Enter), si no ←.
    fn change_setting(&mut self, row: Row, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if row.needs_notify() && !self.settings.alerts {
            return;
        }
        match row {
            Row::Theme => {
                let id = crate::keynav::cycle(&ThemeId::ALL, self.theme_id, forward);
                self.set_theme(id, window, cx);
            }
            Row::FontSize => {
                let step = if forward { FONT_STEP } else { -FONT_STEP };
                self.set_font_size(self.settings.font_size + step, cx);
            }
            Row::Sidebar => self.toggle_sidebar(cx),
            Row::Marquee => {
                self.settings.marquesina = !self.settings.marquesina;
                self.persist();
            }
            Row::Language => {
                let lang = crate::keynav::cycle(&LANGUAGES, self.settings.language, forward);
                self.set_language(lang, cx);
            }
            Row::Notify => {
                self.settings.alerts = !self.settings.alerts;
                self.persist();
            }
            Row::Waiting => self.set_notifications(|n| n.waiting = !n.waiting),
            Row::Permission => self.set_notifications(|n| n.permission = !n.permission),
            Row::Finished => self.set_notifications(|n| n.finished = !n.finished),
            Row::Interval => self.set_notifications(|n| n.interval_min = step_interval(n.interval_min, forward)),
            Row::Background => self.set_notifications(|n| n.only_background = !n.only_background),
            Row::UpdatesAuto => {
                self.settings.check_updates = !self.settings.check_updates;
                self.persist();
                if self.settings.check_updates {
                    self.check_updates(false, cx);
                }
            }
            Row::UpdatesNow => {
                if self.update_available() {
                    self.start_update(cx);
                } else {
                    self.check_updates(true, cx);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn set_language(&mut self, lang: Language, cx: &mut Context<Self>) {
        self.settings.language = lang;
        i18n::set(i18n::resolve(lang, i18n::system_locale().as_deref()));
        self.persist();
        cx.notify();
    }

    fn set_notifications(&mut self, f: impl FnOnce(&mut NotificationSettings)) {
        f(&mut self.settings.notifications);
        self.persist();
    }

    pub(super) fn on_settings_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = ks.modifiers;
        let key = ks.key.to_ascii_lowercase();
        if m.control && !m.shift && !m.alt && key == "," {
            self.close_settings(window, cx);
            return;
        }
        let len = Row::ALL.len();
        let selected = self.settings_panel.selected.min(len - 1);
        let row = Row::ALL[selected];
        match key.as_str() {
            "escape" => {
                self.close_settings(window, cx);
                return;
            }
            "tab" if m.shift => self.settings_panel.selected = crate::keynav::step(selected, len, false),
            "tab" => self.settings_panel.selected = crate::keynav::step(selected, len, true),
            _ if m.modified() => return,
            "up" | "down" => self.settings_panel.selected = crate::keynav::step(selected, len, key == "down"),
            "left" | "right" => self.change_setting(row, key == "right", window, cx),
            "enter" | "space" => self.change_setting(row, true, window, cx),
            _ => return,
        }
        self.settings_panel.keyboard = true;
        cx.notify();
    }

    /// Prueba de humo: `KUNTUR_SMOKE_SETTINGS=1` abre el panel (sin guardar nada).
    pub(super) fn smoke_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(v) = std::env::var("KUNTUR_SMOKE_SETTINGS") {
            self.config_path = None;
            match v.as_str() {
                "en" => self.set_language(Language::En, cx),
                "es" => self.set_language(Language::Es, cx),
                _ => {}
            }
            self.open_settings(None, window, cx);
        }
    }

    // ── Dibujo ──────────────────────────────────────────────────────────────

    pub(super) fn render_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = self.colors.clone();
        let hover = c.hover;
        let header = h_flex()
            .h(px(44.))
            .px(px(14.))
            .gap(px(8.))
            .items_center()
            .border_b_1()
            .border_color(c.border)
            .child(Icon::new(IconName::Settings).size(px(15.)).text_color(c.muted))
            .child(
                div()
                    .flex_1()
                    .text_size(px(13.5))
                    .font_weight(FontWeight::MEDIUM)
                    .child(t(T::Settings)),
            )
            .child(
                div()
                    .id("settings-close")
                    .size(px(24.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_color(c.muted)
                    .hover(move |s| s.bg(hover))
                    .on_click(cx.listener(|this, _, window, cx| this.close_settings(window, cx)))
                    .child(Icon::new(IconName::Close).size(px(14.))),
            );

        let notify_on = self.settings.alerts;
        let n = &self.settings.notifications;
        let body = v_flex()
            .id("settings-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(8.))
            .py(px(6.))
            .gap(px(2.))
            .child(section_title(t(T::Appearance), &c))
            .child(self.render_themes(cx))
            .child(self.setting_row(Row::FontSize, t(T::FontSize), None, self.font_control(cx), cx))
            .child(self.setting_row(
                Row::Sidebar,
                t(T::SidePanel),
                None,
                self.segmented(
                    Row::Sidebar,
                    vec![
                        (t(T::SidePanelFull).into(), self.settings.sidebar == SidebarMode::Full),
                        (t(T::SidePanelIcons).into(), self.settings.sidebar == SidebarMode::Icons),
                    ],
                    |this, i, _window, cx| {
                        this.set_sidebar(if i == 0 { SidebarMode::Full } else { SidebarMode::Icons }, cx)
                    },
                    true,
                    cx,
                ),
                cx,
            ))
            .child(self.switch_row(
                Row::Marquee,
                t(T::Marquee),
                Some(t(T::MarqueeHint)),
                self.settings.marquesina,
                true,
                cx,
            ))
            .child(
                self.setting_row(
                    Row::Language,
                    t(T::Language),
                    None,
                    self.segmented(
                        Row::Language,
                        LANGUAGES
                            .iter()
                            .map(|&l| (language_label(l).into(), self.settings.language == l))
                            .collect(),
                        |this, i, _window, cx| this.set_language(LANGUAGES[i], cx),
                        true,
                        cx,
                    ),
                    cx,
                ),
            )
            .child(section_title(t(T::Notifications), &c))
            .child(self.switch_row(
                Row::Notify,
                t(T::NotifyEnabled),
                Some(t(T::NotifyEnabledHint)),
                notify_on,
                true,
                cx,
            ))
            .child(self.switch_row(Row::Waiting, t(T::NotifyWaiting), None, n.waiting, notify_on, cx))
            .child(self.switch_row(
                Row::Permission,
                t(T::NotifyPermission),
                None,
                n.permission,
                notify_on,
                cx,
            ))
            .child(self.switch_row(Row::Finished, t(T::NotifyFinished), None, n.finished, notify_on, cx))
            .child(
                self.setting_row(
                    Row::Interval,
                    t(T::NotifyInterval),
                    Some(t(T::NotifyIntervalHint)),
                    self.segmented(
                        Row::Interval,
                        INTERVALS
                            .iter()
                            .map(|&v| (interval_label(v), n.interval_min == v))
                            .collect(),
                        |this, i, _window, _cx| this.set_notifications(|n| n.interval_min = INTERVALS[i]),
                        notify_on,
                        cx,
                    ),
                    cx,
                ),
            )
            .child(self.switch_row(
                Row::Background,
                t(T::NotifyBackground),
                None,
                n.only_background,
                notify_on,
                cx,
            ))
            .child(section_title(t(T::Updates), &c))
            .child(self.switch_row(
                Row::UpdatesAuto,
                t(T::UpdatesAuto),
                Some(t(T::UpdatesAutoHint)),
                self.settings.check_updates,
                true,
                cx,
            ))
            .child(self.updates_now_row(cx));

        let footer = div()
            .px(px(14.))
            .py(px(8.))
            .border_t_1()
            .border_color(c.border)
            .text_size(px(11.))
            .text_color(c.muted)
            .child(t(T::SettingsKeys));

        let panel = v_flex()
            .id("settings")
            .track_focus(&self.focus)
            .w(px(PANEL_W))
            .max_w(relative(0.95))
            .max_h(relative(0.94))
            .bg(c.popover)
            .border_1()
            .border_color(c.border)
            .rounded(px(10.))
            .shadow_lg()
            .overflow_hidden()
            .occlude()
            .font_family(self.ui_font.clone())
            .text_color(c.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .child(body)
            .child(footer);

        deferred(
            div()
                .id("settings-backdrop")
                .absolute()
                .top(TITLE_BAR_HEIGHT)
                .left_0()
                .right_0()
                .bottom_0()
                .flex()
                .items_start()
                .justify_end()
                .pt(px(6.))
                .pr(px(8.))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.close_settings(window, cx)),
                )
                .child(panel),
        )
        .with_priority(1)
        .into_any_element()
    }

    /// Fila con el título a la izquierda y el control a la derecha; la elegida con el teclado se
    /// marca. Un clic en la fila la elige.
    fn setting_row(
        &self,
        row: Row,
        label: &'static str,
        hint: Option<&'static str>,
        control: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &self.colors;
        let marked = self.settings_panel.keyboard && self.settings_panel.selected == row.index();
        let dim = row.needs_notify() && !self.settings.alerts;
        h_flex()
            .id(("setting-row", row.index()))
            .px(px(8.))
            .py(px(7.))
            .gap(px(12.))
            .items_center()
            .rounded(px(7.))
            .when(marked, |d| d.bg(c.hover))
            .when(dim, |d| d.opacity(0.45))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.settings_panel.selected = row.index();
                cx.notify();
            }))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(1.))
                    .child(div().text_size(px(12.5)).child(label))
                    .when_some(hint, |d, h| {
                        d.child(div().text_size(px(11.)).text_color(c.muted).child(h))
                    }),
            )
            .child(control)
            .into_any_element()
    }

    fn switch_row(
        &self,
        row: Row,
        label: &'static str,
        hint: Option<&'static str>,
        on: bool,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &self.colors;
        let track = if on { c.accent } else { c.surface_2 };
        let knob = if on { c.popover } else { c.muted };
        let control = div()
            .id(("setting-switch", row.index()))
            .w(px(32.))
            .h(px(18.))
            .flex_shrink_0()
            .rounded_full()
            .bg(track)
            .border_1()
            .border_color(c.border)
            .p(px(2.))
            .flex()
            .items_center()
            .when(on, |d| d.justify_end())
            .when(enabled, |d| d.cursor_pointer())
            .on_click(cx.listener(move |this, _, window, cx| {
                this.settings_panel.selected = row.index();
                this.change_setting(row, true, window, cx);
            }))
            .child(div().size(px(12.)).rounded_full().bg(knob))
            .into_any_element();
        self.setting_row(row, label, hint, control, cx)
    }

    /// Botones de opción unidos ("segmentado"); `pick(i)` aplica la opción i.
    fn segmented(
        &self,
        row: Row,
        options: Vec<(SharedString, bool)>,
        pick: fn(&mut Workspace, usize, &mut Window, &mut Context<Workspace>),
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &self.colors;
        let hover = c.hover;
        h_flex()
            .flex_shrink_0()
            .p(px(2.))
            .gap(px(2.))
            .rounded(px(7.))
            .bg(c.surface_2)
            .children(options.into_iter().enumerate().map(|(i, (label, active))| {
                div()
                    .id(SharedString::from(format!("seg-{}-{i}", row.index())))
                    .px(px(8.))
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .text_size(px(11.5))
                    .map(|d| {
                        if active {
                            d.bg(c.popover).text_color(c.text).font_weight(FontWeight::MEDIUM)
                        } else {
                            d.text_color(c.muted)
                        }
                    })
                    .when(enabled, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !enabled {
                            return;
                        }
                        this.settings_panel.selected = row.index();
                        pick(this, i, window, cx);
                        cx.notify();
                    }))
                    .child(label)
            }))
            .into_any_element()
    }

    fn font_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let hover = c.hover;
        let button = |id: &'static str, icon: IconName, tip: &'static str, step: f32, cx: &mut Context<Self>| {
            div()
                .id(id)
                .size(px(22.))
                .rounded(px(5.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .text_color(c.muted)
                .hover(move |s| s.bg(hover))
                .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings_panel.selected = Row::FontSize.index();
                    this.set_font_size(this.settings.font_size + step, cx);
                }))
                .child(Icon::new(icon).size(px(13.)))
        };
        h_flex()
            .flex_shrink_0()
            .gap(px(4.))
            .items_center()
            .child(button(
                "font-smaller",
                IconName::Minus,
                t(T::FontSmaller),
                -FONT_STEP,
                cx,
            ))
            .child(
                div()
                    .w(px(30.))
                    .text_center()
                    .text_size(px(12.))
                    .child(format!("{}", self.settings.font_size.round() as i32)),
            )
            .child(button("font-bigger", IconName::Plus, t(T::FontBigger), FONT_STEP, cx))
            .into_any_element()
    }

    /// Temas como círculos con sus colores (fondo + acento) y el nombre debajo; el actual con anillo.
    fn render_themes(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let marked = self.settings_panel.keyboard && self.settings_panel.selected == Row::Theme.index();
        let circles = self.settings_panel.swatches.iter().map(|sw| {
            let current = sw.id == self.theme_id;
            let id = sw.id;
            let ring = if current {
                c.accent
            } else {
                gpui_kit::transparent_black()
            };
            v_flex()
                .id(SharedString::from(format!("theme-{}", sw.id.key())))
                .items_center()
                .gap(px(6.))
                .px(px(6.))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.settings_panel.selected = Row::Theme.index();
                    this.set_theme(id, window, cx);
                }))
                .child(
                    div()
                        .size(px(SWATCH + 6.))
                        .rounded_full()
                        .border_2()
                        .border_color(ring)
                        .p(px(2.))
                        .child(
                            div()
                                .size_full()
                                .rounded_full()
                                .bg(sw.fill)
                                .border_1()
                                .border_color(sw.edge)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(div().size(px(14.)).rounded_full().bg(sw.accent)),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .when(current, |d| d.font_weight(FontWeight::MEDIUM))
                        .text_color(if current { c.text } else { c.muted })
                        .child(sw.name.clone()),
                )
        });
        v_flex()
            .px(px(8.))
            .py(px(7.))
            .gap(px(8.))
            .rounded(px(7.))
            .when(marked, |d| d.bg(c.hover))
            .child(div().text_size(px(12.5)).child(t(T::Theme)))
            .child(h_flex().gap(px(14.)).children(circles))
            .into_any_element()
    }
}

impl Workspace {
    /// Fila "Versión instalada": estado (al día, disponible, descargando…) y el botón "Buscar
    /// ahora" o "Actualizar".
    fn updates_now_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let hover = c.hover;
        let available = self.update_available();
        let label = self.update_action_label();
        let control = h_flex()
            .flex_shrink_0()
            .gap(px(8.))
            .items_center()
            .child(
                div()
                    .max_w(px(180.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_size(px(11.))
                    .text_color(if available { c.accent } else { c.muted })
                    .child(self.update_status_text()),
            )
            .child(
                div()
                    .id("updates-now")
                    .px(px(8.))
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .bg(c.surface_2)
                    .text_size(px(11.5))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.settings_panel.selected = Row::UpdatesNow.index();
                        this.change_setting(Row::UpdatesNow, true, window, cx);
                    }))
                    .child(label),
            )
            .into_any_element();
        let version: &'static str = t(T::UpdatesVersion);
        self.setting_row(Row::UpdatesNow, version, None, control, cx)
    }
}

fn section_title(label: &'static str, c: &UiColors) -> Div {
    div()
        .px(px(8.))
        .pt(px(10.))
        .pb(px(2.))
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(c.muted)
        .child(label.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::step_interval;

    #[test]
    fn intervalo_avanza_y_retrocede_por_la_lista() {
        assert_eq!(step_interval(5, true), 15);
        assert_eq!(step_interval(5, false), 1);
        assert_eq!(step_interval(0, false), 0);
        assert_eq!(step_interval(30, true), 30);
        // Un valor escrito a mano en config.toml salta al más cercano de la lista.
        assert_eq!(step_interval(7, true), 15);
        assert_eq!(step_interval(7, false), 5);
        assert_eq!(step_interval(500, false), 30);
    }
}
