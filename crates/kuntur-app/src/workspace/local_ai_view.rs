//! "IA local" (ajustes v0.2.1): proveedores de modelos de esta PC (LM Studio, Ollama, Jan,
//! llama.cpp) en el panel izquierdo, debajo de los agentes instalados. Clic en un modelo:
//! **OpenCode con este modelo** (configuración de OpenCode de la sesión con el proveedor local, junto
//! con sus MCP), **Chat rápido** (`lms chat` / `ollama run`, sin Enter) e **Iniciar servidor**.
//!
//! La lectura va en el monitor lento (cada 30 s, fuera del hilo de la UI; ver `monitor.rs`) y solo
//! redibuja si algo cambió (`idle.rs`). Nunca ejecuta `lms ls` por su cuenta (arranca el servicio de
//! LM Studio): eso solo pasa cuando el usuario actualiza o elige un modelo de LM Studio.

use std::time::Duration;

use kuntur_agents::local::{self, LocalModel, LocalProvider, ProviderStatus};

use super::side_panel::icon_badge;
use super::*;
use crate::i18n::{T, t};

/// Modelos por proveedor que se muestran en el panel (el resto, en "+N más" y en la paleta).
const MAX_MODELS: usize = 5;
const MENU_W: f32 = 230.0;

/// Menú abierto sobre un modelo.
struct LocalMenu {
    model: LocalModel,
    server: bool,
    position: Point<Pixels>,
}

#[derive(Default)]
pub(super) struct LocalAiState {
    pub(super) providers: Vec<ProviderStatus>,
    menu: Option<LocalMenu>,
    /// Prueba de humo: datos sintéticos que la lectura real no pisa.
    pub(super) smoke: bool,
    /// Hay una acción en curso (iniciar servidor, leer `lms ls`).
    busy: bool,
}

impl Workspace {
    // ── Datos ───────────────────────────────────────────────────────────────

    /// Resultado de la lectura lenta del monitor.
    pub(super) fn apply_local_ai(&mut self, providers: Vec<ProviderStatus>) {
        if !self.local_ai.smoke {
            self.local_ai.providers = providers;
        }
    }

    /// "IA local: actualizar" (paleta): lectura completa ya, y en LM Studio también `lms ls`
    /// (las claves reales de sus modelos). El usuario lo pidió, así que arrancar su servicio está bien.
    pub(super) fn refresh_local_ai(&mut self, cx: &mut Context<Self>) {
        if self.local_ai.smoke {
            return;
        }
        let home = self.home.clone();
        cx.spawn(async move |this, cx| {
            let providers = cx
                .background_executor()
                .spawn(async move {
                    let Some(home) = home else { return Vec::new() };
                    let mut list = local::detect_now(&home);
                    if let Some(lms) = list
                        .iter_mut()
                        .find(|p| p.provider == LocalProvider::LmStudio && !p.server)
                    {
                        let listed = local::lms_list();
                        local::apply_lms_keys(&mut lms.models, &listed);
                    }
                    list
                })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.local_ai.providers = providers;
                cx.notify();
            });
        })
        .detach();
    }

    /// Prueba de humo: proveedores y modelos SINTÉTICOS (nunca los reales del usuario).
    /// `KUNTUR_SMOKE_LOCAL_AI=list` o `menu` (además abre el menú del primer modelo).
    pub(super) fn smoke_local_ai(&mut self, cx: &mut Context<Self>) {
        let Ok(mode) = std::env::var("KUNTUR_SMOKE_LOCAL_AI") else {
            return;
        };
        self.config_path = None;
        self.settings.sidebar = SidebarMode::Full;
        self.local_ai.smoke = true;
        let model = |provider, id: &str, name: &str, size: u64, loaded| LocalModel {
            provider,
            id: id.into(),
            name: name.into(),
            size: Some(size),
            loaded,
            path: None,
        };
        self.local_ai.providers = vec![
            ProviderStatus {
                provider: LocalProvider::LmStudio,
                installed: true,
                server: false,
                models: vec![
                    model(
                        LocalProvider::LmStudio,
                        "qwen-coder-mini",
                        "Qwen Coder Mini 7B",
                        4_683_000_000,
                        false,
                    ),
                    model(
                        LocalProvider::LmStudio,
                        "demo-instruct-3b",
                        "Demo Instruct 3B",
                        2_020_000_000,
                        false,
                    ),
                ],
            },
            ProviderStatus {
                provider: LocalProvider::Ollama,
                installed: true,
                server: true,
                models: vec![
                    model(
                        LocalProvider::Ollama,
                        "llama-demo:latest",
                        "llama-demo:latest",
                        4_700_000_000,
                        true,
                    ),
                    model(
                        LocalProvider::Ollama,
                        "coder-demo:7b",
                        "coder-demo:7b",
                        4_100_000_000,
                        false,
                    ),
                ],
            },
        ];
        if mode == "menu" {
            let m = self.local_ai.providers[0].models[0].clone();
            self.local_ai.menu = Some(LocalMenu {
                model: m,
                server: false,
                position: point(px(150.), px(560.)),
            });
        }
        cx.notify();
    }

    // ── Acciones ────────────────────────────────────────────────────────────

    fn close_local_menu(&mut self, cx: &mut Context<Self>) {
        if self.local_ai.menu.take().is_some() {
            cx.notify();
        }
    }

    /// La terminal donde escribir: la que tiene el foco en la sesión activa si está libre; si no,
    /// una sesión nueva (como al abrir un agente). `None` en pestañas remotas: el modelo corre en
    /// esta PC.
    fn local_target(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<(u64, bool, Duration)> {
        let target = self.sessions.active_id().map(|sid| (sid, self.dock_has_focus(sid)));
        if let Some((sid, _)) = target
            && self.sessions.get(sid).is_some_and(|e| e.value.remote.is_some())
        {
            self.show_notice(t(T::LocalRemoteTab), cx);
            return None;
        }
        match target.filter(|t| !self.busy.contains(t)) {
            Some((sid, docked)) => Some((sid, docked, Duration::ZERO)),
            None => {
                self.new_session(window, cx);
                let sid = self.sessions.active_id()?;
                Some((sid, false, Duration::from_millis(1_500)))
            }
        }
    }

    /// Prepara el modelo fuera del hilo de la UI (clave real de LM Studio, servidor encendido si
    /// `need_server`) y después llama a `then` con el modelo listo.
    fn with_ready_model(
        &mut self,
        model: LocalModel,
        need_server: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Workspace, LocalModel, &mut Window, &mut Context<Workspace>) + 'static,
    ) {
        self.local_ai.busy = true;
        if need_server {
            self.show_notice(t(T::LocalStarting), cx);
        }
        let smoke = self.local_ai.smoke;
        cx.spawn_in(window, async move |this, cx| {
            let ready = cx
                .background_executor()
                .spawn(async move {
                    let mut model = model;
                    if smoke {
                        return Ok(model);
                    }
                    if model.provider == LocalProvider::LmStudio && model.path.is_some() {
                        let listed = local::lms_list();
                        local::apply_lms_keys(std::slice::from_mut(&mut model), &listed);
                    }
                    if need_server {
                        local::start_server_and_wait(model.provider, Duration::from_secs(25))?;
                    }
                    Ok::<LocalModel, String>(model)
                })
                .await;
            let _ = this.update_in(cx, |w, window, cx| {
                w.local_ai.busy = false;
                match ready {
                    Ok(model) => then(w, model, window, cx),
                    Err(e) => w.show_notice(format!("{}: {e}", t(T::LocalStartFailed)), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "OpenCode con este modelo": abre OpenCode en la sesión con su configuración generada (el
    /// proveedor local más los MCP de la sesión, en un solo archivo; nunca la configuración global).
    fn open_opencode_with(&mut self, model: LocalModel, server: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.close_local_menu(cx);
        let Some(def) = self.catalog.get("opencode").cloned() else {
            return;
        };
        if !self.installed.iter().any(|id| id == "opencode") && !self.local_ai.smoke {
            self.show_notice(t(T::LocalOpenCodeMissing), cx);
            return;
        }
        self.with_ready_model(model, !server, window, cx, move |w, model, window, cx| {
            let Some((session, docked, delay)) = w.local_target(window, cx) else {
                return;
            };
            let shell = w.sessions.get(session).and_then(|e| e.value.shell.clone());
            let extra = local::opencode_provider(&model);
            let line = w.agent_launch_line_with(
                session,
                &def,
                ShellFlavor::from_program(shell.as_deref()),
                Some(extra),
                cx,
            );
            if let Some(view) = w.terminal(session, docked) {
                view.read(cx).send_line(line, delay);
            }
            w.busy.insert((session, docked));
            w.usage.open = false;
            w.audit.open = false;
            w.sessions.activate(session);
            w.focus_active(window, cx);
        });
    }

    /// "Chat rápido": escribe `lms chat <modelo>` / `ollama run <modelo>` sin Enter.
    fn local_chat(&mut self, model: LocalModel, window: &mut Window, cx: &mut Context<Self>) {
        self.close_local_menu(cx);
        self.with_ready_model(model, false, window, cx, |w, model, window, cx| {
            let Some((session, docked, delay)) = w.local_target(window, cx) else {
                return;
            };
            let shell = w.sessions.get(session).and_then(|e| e.value.shell.clone());
            let Some(line) = model
                .provider
                .chat_line(&model.id, ShellFlavor::from_program(shell.as_deref()))
            else {
                return;
            };
            if let Some(view) = w.terminal(session, docked) {
                view.read(cx).paste_text(line, delay);
            }
            w.sessions.activate(session);
            w.focus_active(window, cx);
        });
    }

    /// "Iniciar servidor" (sin ventana, en segundo plano).
    fn local_start_server(&mut self, provider: LocalProvider, cx: &mut Context<Self>) {
        self.close_local_menu(cx);
        if self.local_ai.smoke {
            return;
        }
        self.local_ai.busy = true;
        self.show_notice(t(T::LocalStarting), cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { local::start_server_and_wait(provider, Duration::from_secs(25)) })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.local_ai.busy = false;
                match result {
                    Ok(()) => {
                        if let Some(p) = w.local_ai.providers.iter_mut().find(|p| p.provider == provider) {
                            p.server = true;
                        }
                        w.show_notice(format!("{} · {}", provider.name(), t(T::LocalServerOn)), cx);
                    }
                    Err(e) => w.show_notice(format!("{}: {e}", t(T::LocalStartFailed)), cx),
                }
                cx.notify();
            });
            // Lectura completa con el servidor ya encendido (modelos y cargados).
            let _ = this.update(cx, |w, cx| w.refresh_local_ai(cx));
        })
        .detach();
    }

    // ── Dibujo ──────────────────────────────────────────────────────────────

    /// Sección "IA local" del panel completo (nada si no hay ningún proveedor instalado o la pestaña
    /// es remota).
    pub(super) fn render_local_ai(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.local_ai.providers.is_empty() || self.active_remote_agents().is_some() {
            return None;
        }
        let c = &self.colors;
        let mut col = v_flex().gap(px(2.)).child(
            div()
                .px(px(6.))
                .pt(px(2.))
                .text_size(px(11.))
                .text_color(c.muted)
                .child(t(T::LocalAi)),
        );
        for p in &self.local_ai.providers {
            col = col.child(self.render_provider(p, cx));
        }
        Some(col.into_any_element())
    }

    fn render_provider(&self, p: &ProviderStatus, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let provider = p.provider;
        let state: SharedString = if p.server {
            t(T::LocalServerOn).into()
        } else {
            t(T::LocalServerOff).into()
        };
        let hover = c.hover;
        let header = h_flex()
            .id(SharedString::from(format!("local-{}", provider.id())))
            .px(px(6.))
            .h(px(24.))
            .gap(px(7.))
            .items_center()
            .rounded(px(6.))
            .child(icon_badge(
                crate::agent_icons::for_agent(provider.id()),
                provider.letter(),
                provider.color(),
                16.,
                4.,
            ))
            .child(div().flex_1().min_w_0().text_size(px(12.)).child(provider.name()))
            .child(
                div()
                    .size(px(6.))
                    .rounded_full()
                    .bg(if p.server { c.ok } else { c.muted.opacity(0.5) }),
            )
            .tooltip(move |window, cx| Tooltip::new(format!("{} · {state}", provider.name())).build(window, cx))
            .when(!p.server && provider.start_server().is_some(), |d| {
                d.cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .on_click(cx.listener(move |this, _, _, cx| this.local_start_server(provider, cx)))
            });
        let mut rows: Vec<AnyElement> = p
            .models
            .iter()
            .take(MAX_MODELS)
            .map(|m| self.render_local_model(m, p.server, cx))
            .collect();
        if p.models.is_empty() {
            rows.push(
                div()
                    .pl(px(29.))
                    .text_size(px(11.))
                    .text_color(c.muted)
                    .child(t(T::LocalNoModels))
                    .into_any_element(),
            );
        } else if p.models.len() > MAX_MODELS {
            rows.push(
                div()
                    .pl(px(29.))
                    .text_size(px(11.))
                    .text_color(c.muted)
                    .child(format!("+{} {}", p.models.len() - MAX_MODELS, t(T::LocalMore)))
                    .into_any_element(),
            );
        }
        v_flex().child(header).children(rows).into_any_element()
    }

    fn render_local_model(&self, m: &LocalModel, server: bool, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let hover = c.hover;
        let size = m.size.map(local::format_size).unwrap_or_default();
        let mut tip = vec![m.name.clone(), m.provider.name().to_string()];
        if !size.is_empty() {
            tip.push(size.clone());
        }
        if m.loaded {
            tip.push(t(T::LocalLoaded).into());
        }
        let tip: SharedString = tip.join(" · ").into();
        let model = m.clone();
        h_flex()
            .id(SharedString::from(format!("local-model-{}-{}", m.provider.id(), m.id)))
            .pl(px(29.))
            .pr(px(6.))
            .h(px(22.))
            .gap(px(6.))
            .items_center()
            .rounded(px(6.))
            .cursor_pointer()
            .text_size(px(11.5))
            .hover(move |s| s.bg(hover))
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.local_ai.menu = Some(LocalMenu {
                        model: model.clone(),
                        server,
                        position: ev.position,
                    });
                    cx.notify();
                }),
            )
            .child(div().size(px(5.)).flex_shrink_0().rounded_full().bg(if m.loaded {
                c.accent
            } else {
                c.muted.opacity(0.35)
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(local::short_name(m)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(c.muted)
                    .text_size(px(10.5))
                    .child(size),
            )
            .into_any_element()
    }

    /// Panel compacto: una insignia por proveedor; clic = panel completo (donde están sus modelos).
    pub(super) fn render_local_ai_compact(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.active_remote_agents().is_some() {
            return Vec::new();
        }
        let c = &self.colors;
        self.local_ai
            .providers
            .iter()
            .map(|p| {
                let provider = p.provider;
                let hover = c.hover;
                let tip: SharedString = format!(
                    "{} · {} · {}",
                    provider.name(),
                    p.models.len(),
                    if p.server {
                        t(T::LocalServerOn)
                    } else {
                        t(T::LocalServerOff)
                    }
                )
                .into();
                div()
                    .id(SharedString::from(format!("local-compact-{}", provider.id())))
                    .size(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .on_click(cx.listener(|this, _, _, cx| this.set_sidebar(SidebarMode::Full, cx)))
                    .child(icon_badge(
                        crate::agent_icons::for_agent(provider.id()),
                        provider.letter(),
                        provider.color(),
                        22.,
                        11.,
                    ))
                    .into_any_element()
            })
            .collect()
    }

    /// Menú del modelo (flotante, se cierra con un clic fuera o Esc).
    pub(super) fn render_local_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.local_ai.menu.as_ref()?;
        let c = self.colors.clone();
        let hover = c.hover;
        let opencode = self.installed.iter().any(|id| id == "opencode") || self.local_ai.smoke;
        let model = menu.model.clone();
        let server = menu.server;
        let item = |id: &'static str, label: SharedString, enabled: bool| {
            h_flex()
                .id(id)
                .h(px(28.))
                .px(px(10.))
                .items_center()
                .rounded(px(6.))
                .text_size(px(12.5))
                .map(|d| {
                    if enabled {
                        d.cursor_pointer().hover(move |s| s.bg(hover))
                    } else {
                        d.text_color(c.muted.opacity(0.6))
                    }
                })
                .child(label)
        };
        let title = format!("{} · {}", local::short_name(&model), model.provider.name());
        let mut list = v_flex()
            .id("local-menu")
            .absolute()
            .left(menu.position.x)
            .top(menu.position.y)
            .w(px(MENU_W))
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
            .child(
                div()
                    .px(px(10.))
                    .py(px(5.))
                    .text_size(px(11.))
                    .text_color(c.muted)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(title),
            );
        let m1 = model.clone();
        let open = item(
            "local-opencode",
            t(T::LocalOpenCode).into(),
            opencode && !self.local_ai.busy,
        );
        list = list.child(if opencode {
            open.on_click(
                cx.listener(move |this, _, window, cx| this.open_opencode_with(m1.clone(), server, window, cx)),
            )
        } else {
            open.tooltip(|window, cx| Tooltip::new(t(T::LocalOpenCodeMissing)).build(window, cx))
        });
        if model.provider.chat_line(&model.id, ShellFlavor::PowerShell).is_some() {
            let m2 = model.clone();
            list = list.child(
                item("local-chat", t(T::LocalChat).into(), !self.local_ai.busy)
                    .on_click(cx.listener(move |this, _, window, cx| this.local_chat(m2.clone(), window, cx))),
            );
        }
        if !server && model.provider.start_server().is_some() {
            let provider = model.provider;
            list = list.child(
                item("local-start", t(T::LocalStartServer).into(), !self.local_ai.busy)
                    .on_click(cx.listener(move |this, _, _, cx| this.local_start_server(provider, cx))),
            );
        }
        Some(
            deferred(
                div()
                    .id("local-menu-backdrop")
                    .absolute()
                    .inset_0()
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.close_local_menu(cx)),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _, _, cx| this.close_local_menu(cx)),
                    )
                    .child(list),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// Esc con el menú abierto lo cierra.
    pub(super) fn local_menu_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if self.local_ai.menu.is_some() && ev.keystroke.key == "escape" {
            self.close_local_menu(cx);
            return true;
        }
        false
    }
}
