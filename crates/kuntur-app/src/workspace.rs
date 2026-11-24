//! Vista raíz de Kuntur (diseño v5): barra de título de 34 px con las sesiones como pestañas,
//! panel izquierdo de agentes / archivos (completo ↔ compacto, nunca oculto), la terminal activa y
//! una línea inferior de 22 px con la carpeta actual y la RAM. Del v4 se mantiene: una sola barra
//! arriba, jerarquía por peso y color de texto, íconos de un solo color, color solo para estado y lo
//! secundario al pasar el mouse.

mod agents_exact;
mod audit_view;
mod connections_view;
mod dock_view;
mod files_panel;
mod mcp_view;
mod preview_view;
mod side_panel;
mod title_bar;
mod transfers_view;
mod usage_view;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, TITLE_BAR_HEIGHT, h_flex, tooltip::Tooltip, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use kuntur_agents::{
    AgentKey, AgentKind, AlertKind, AlertTracker, Catalog, ExtraAgent, Observation, RunningAgent, ShellFlavor,
    launch_line, state_for,
};
use kuntur_config::{SessionConfig, Settings, SidebarMode, Theme, ThemeId, builtin};
use kuntur_term::{GridSize, PaneOptions};
use kuntur_usage::codex::Limits;

use crate::agents_panel::{AgentFilter, AgentRow};
use crate::command_palette::{PaletteAction, PaletteItem, PaletteMode, PaletteState};
use crate::dock::{DockState, is_toggle_key};
use crate::monitor::{Sample, Sampler, ShellRef, SlowSample, home_dir, slow_sample};
use crate::sessions::{SessionList, next_default_name, short_title};
use crate::terminal_view::{TerminalEvent, TerminalView};
use crate::theme_bridge::{FALLBACK_MONO, FALLBACK_UI, UiColors, apply_to_kit, palette_from, pick_font};
use agents_exact::ExactAgents;
use audit_view::AuditState;
use connections_view::{ConnectionsState, RemoteTab};
use dock_view::DockDrag;
use files_panel::FilesState;
use mcp_view::McpState;
use preview_view::PreviewState;
use usage_view::UsageState;

/// Panel izquierdo compacto (solo íconos) y completo.
const PANEL_COMPACT: f32 = 56.0;
const PANEL_FULL: f32 = 240.0;
const ICON_BUTTON: f32 = 28.0;
/// Margen izquierdo de la barra: centra el botón del panel sobre la columna compacta.
const TITLE_PAD_LEFT: f32 = (PANEL_COMPACT - ICON_BUTTON) / 2.0;
const STATUS_LINE: f32 = 22.0;
const UI_TEXT: f32 = 12.5;
/// Cada cuánto se leen procesos y memoria; las lecturas lentas (instalados, límites), cada
/// `SLOW_EVERY` vueltas.
const TICK: Duration = Duration::from_secs(2);
const SLOW_EVERY: u32 = 15;

const PALETTE_WIDTH: f32 = 520.0;
/// Paso de Ctrl+= / Ctrl+- (puntos).
const FONT_STEP: f32 = 1.0;

struct Session {
    view: Entity<TerminalView>,
    /// Programa del shell (`None` = el predeterminado); se guarda con la sesión.
    shell: Option<String>,
    /// Terminal acoplada (F4): existe desde que se abre por primera vez hasta que su shell termina;
    /// ocultarla no la destruye.
    dock: Option<Entity<TerminalView>>,
    dock_state: DockState,
    /// Sesión remota (F6): conexión y terminal principal; `None` = local.
    remote: Option<RemoteTab>,
    _events: Subscription,
    _dock_events: Option<Subscription>,
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
    /// Paleta de comandos abierta (Ctrl+Shift+P).
    palette: Option<PaletteState>,
    /// Foco del panel de la paleta: mientras está abierta, el teclado no llega a la terminal.
    focus: FocusHandle,
    /// Qué muestra el panel izquierdo.
    panel: PanelView,
    agent_filter: AgentFilter,
    agent_query: String,
    /// Foco del buscador de agentes.
    search_focus: FocusHandle,
    /// Agentes conocidos: los del catálogo y los de `config.toml`.
    catalog: Arc<Catalog>,
    /// Última lectura: (sesión, ¿acoplada?, agente) en el orden de las sesiones.
    running: Vec<(u64, bool, RunningAgent)>,
    /// Terminales (sesión, ¿acoplada?) con otro programa corriendo: no se les escribe un comando.
    busy: HashSet<(u64, bool)>,
    /// Ids de los agentes instalados.
    installed: Vec<String>,
    /// Alertas (§12): transiciones de los agentes y el contador de la insignia de cada pestaña.
    alerts: AlertTracker,
    badges: HashMap<u64, u32>,
    /// La ventana tiene el foco (si no, ninguna pestaña se está viendo).
    window_active: bool,
    codex_limits: Option<Limits>,
    /// Memoria de trabajo de Kuntur (bytes).
    ram: Option<u64>,
    files: FilesState,
    /// Vista "Uso de agentes" (ocupa el área central mientras está abierta).
    usage: UsageState,
    /// Vista "Auditoría" (también ocupa el área central).
    audit: AuditState,
    /// Gestor de conexiones SSH (panel flotante, F6).
    connections: ConnectionsState,
    /// Vista previa de archivos (F8): divide la zona central cuando tiene pestañas.
    preview: PreviewState,
    /// Gestor de MCP por sesión y comandos con atajo (F10).
    mcp: McpState,
    home: Option<PathBuf>,
    /// F5b: estado exacto por hooks / OSC 7501, worktrees, instalador de ganchos.
    exact: ExactAgents,
    _monitor: Task<()>,
    _activation: Subscription,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PanelView {
    Agents,
    Files,
}

/// Segundos Unix de ahora.
fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Catálogo con los agentes extra de la configuración.
fn catalog_from(settings: &Settings) -> Catalog {
    let extra: Vec<ExtraAgent> = settings
        .agents
        .iter()
        .map(|a| ExtraAgent {
            name: a.name.clone(),
            command: a.command.clone(),
            args: a.args.clone(),
            letter: a.letter,
            color: a.color.clone(),
        })
        .collect();
    Catalog::with_extra(&extra)
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, settings: Settings, config_path: Option<PathBuf>) -> Self {
        let theme_id = ThemeId::from_key(&settings.theme).unwrap_or(ThemeId::Dark);
        let theme = builtin(theme_id);
        let installed = window.text_system().all_font_names();
        let catalog = Arc::new(catalog_from(&settings));
        let mut this = Self {
            sessions: SessionList::new(),
            config_path,
            theme_id,
            colors: UiColors::from_theme(&theme),
            mono_font: pick_font(&installed, &theme.fonts.mono, FALLBACK_MONO),
            ui_font: pick_font(&installed, &theme.fonts.ui, FALLBACK_UI),
            theme,
            settings,
            palette: None,
            focus: cx.focus_handle(),
            panel: PanelView::Agents,
            agent_filter: AgentFilter::All,
            agent_query: String::new(),
            search_focus: cx.focus_handle(),
            catalog,
            running: Vec::new(),
            busy: HashSet::new(),
            installed: Vec::new(),
            alerts: AlertTracker::default(),
            badges: HashMap::new(),
            window_active: window.is_window_active(),
            codex_limits: None,
            ram: None,
            files: FilesState::default(),
            usage: UsageState::default(),
            audit: AuditState::new(cx),
            connections: ConnectionsState::default(),
            preview: PreviewState::default(),
            mcp: McpState::default(),
            home: home_dir(),
            exact: ExactAgents::default(),
            _monitor: Self::spawn_monitor(cx),
            _activation: cx.observe_window_activation(window, |this, window, cx| {
                this.window_active = window.is_window_active();
                this.clear_visible_badge();
                cx.notify();
            }),
        };
        apply_to_kit(&this.theme, cx);
        this.start_exact(window, cx);

        // Prueba de humo: n sesiones sin tocar la configuración guardada.
        let smoke_sessions = std::env::var("KUNTUR_SMOKE_SESSIONS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .map(|n| n.clamp(1, 9));
        if let Some(n) = smoke_sessions {
            this.config_path = None;
            for _ in 0..n {
                this.add_session(None, None, None, window, cx);
            }
        } else {
            let saved = this.settings.sessions.clone();
            for s in saved {
                // Remotas: se reconectan solas; si su conexión ya no existe, se abren locales.
                if let Some(conn) = &s.connection
                    && this.open_remote(conn, s.cwd.clone(), Some(s.name.clone()), s.dock_ratio, window, cx)
                {
                    this.mcp_restore_last(&s);
                    continue;
                }
                this.add_session(Some(s.name.clone()), s.shell.clone(), s.dock_ratio, window, cx);
                this.mcp_restore_last(&s);
            }
            if this.sessions.is_empty() {
                this.add_session(None, None, None, window, cx);
            }
        }

        // KUNTUR_SMOKE_INPUT escribe comandos solo en la primera sesión.
        let smoke_input = std::env::var("KUNTUR_SMOKE_INPUT").ok();
        let smoke_commands = smoke_input.as_ref().map_or(0, |i| i.split("\\n").count()) as u64;
        if let (Some(input), Some(first)) = (smoke_input, this.sessions.iter().next()) {
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

        // Prueba de humo: paleta abierta (con texto, o ya en "renombrar") para poder capturarla.
        if let Ok(mode) = std::env::var("KUNTUR_SMOKE_PALETTE") {
            this.open_palette(window, cx);
            if mode == "rename" {
                this.run_action(PaletteAction::RenameSession, window, cx);
            }
            if let (Ok(q), Some(p)) = (std::env::var("KUNTUR_SMOKE_PALETTE_QUERY"), this.palette.as_mut()) {
                p.insert(&q);
            }
        }

        // Prueba de humo: panel completo, compacto o en "Archivos" (sin guardar).
        if let Ok(panel) = std::env::var("KUNTUR_SMOKE_PANEL") {
            this.config_path = None;
            match panel.as_str() {
                "compact" => this.settings.sidebar = SidebarMode::Icons,
                "files" => {
                    this.settings.sidebar = SidebarMode::Full;
                    this.panel = PanelView::Files;
                }
                _ => this.settings.sidebar = SidebarMode::Full,
            }
        }

        // Prueba de humo: vista de uso abierta con datos SINTÉTICOS (nunca el historial real) y el
        // período pedido ("hoy", "7", "30", "ano", "todo").
        if let Ok(period) = std::env::var("KUNTUR_SMOKE_USAGE") {
            this.config_path = None;
            this.usage.set_initial_period(&period);
            this.open_usage(window, cx);
        }

        // Prueba de humo: vista de auditoría con datos SINTÉTICOS (nunca el historial real); el valor
        // es el texto de búsqueda inicial (vacío = sin búsqueda).
        if let Ok(query) = std::env::var("KUNTUR_SMOKE_AUDIT") {
            this.config_path = None;
            this.audit.set_initial_query(&query);
            this.open_audit(window, cx);
        }

        // Prueba de humo: "clic" en el agente instalado con ese id (como el usuario), a los 4 s.
        if let Ok(id) = std::env::var("KUNTUR_SMOKE_LAUNCH") {
            this.config_path = None;
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(4)).await;
                let _ = this.update_in(cx, |w, window, cx| w.launch_agent(&id, window, cx));
            })
            .detach();
        }

        // Prueba de humo: insignias de alerta en las pestañas no activas (sin notificación).
        if std::env::var_os("KUNTUR_SMOKE_BADGES").is_some() {
            let ids: Vec<u64> = this.sessions.iter().skip(1).map(|e| e.id).collect();
            for (i, id) in ids.into_iter().enumerate() {
                this.badges.insert(id, i as u32 + 1);
            }
        }

        // Prueba de humo: terminal acoplada ("open") de la primera sesión, abierta después de los
        // comandos de la principal; "reopen" además la oculta y la vuelve a mostrar.
        if let Ok(mode) = std::env::var("KUNTUR_SMOKE_DOCK") {
            this.config_path = None;
            let delay = Duration::from_millis(2_500 + 600 * smoke_commands + 1_500);
            let input = std::env::var("KUNTUR_SMOKE_DOCK_INPUT").ok();
            this.smoke_dock(delay, input, mode == "reopen", window, cx);
        }
        // Prueba de humo del gestor de conexiones (conexiones sintéticas).
        this.smoke_connections(window, cx);

        // Prueba de humo de la vista previa (F8): archivos abiertos y enlaces subrayados.
        this.smoke_preview(Duration::from_millis(2_500 + 600 * smoke_commands + 1_500), window, cx);

        // Prueba de humo del árbol remoto y la cola de transferencias (datos sintéticos, F7).
        this.smoke_files(cx);

        // F10: MCP por sesión (limpieza de temporales, capturas → vista previa, prueba de humo).
        this.start_mcp(window, cx);
        this
    }

    // ── Lecturas en segundo plano ───────────────────────────────────────────

    /// Cada 2 s, fuera del hilo de la UI: agentes de cada sesión y RAM; cada 30 s, agentes
    /// instalados y límites de Codex. La UI solo recibe el resultado.
    fn spawn_monitor(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let mut sampler = Sampler::new();
            let mut tick: u32 = 0;
            loop {
                let Ok((shells, catalog)) = this.update(cx, |w, cx| (w.shell_pids(cx), w.catalog.clone())) else {
                    break;
                };
                let slow = tick.is_multiple_of(SLOW_EVERY);
                let (back, sample, slow) = cx
                    .background_executor()
                    .spawn(async move {
                        let sample = sampler.sample(&shells, &catalog);
                        (sampler, sample, slow.then(|| slow_sample(&catalog)))
                    })
                    .await;
                sampler = back;
                if this.update(cx, |w, cx| w.apply_sample(sample, slow, cx)).is_err() {
                    break;
                }
                cx.background_executor().timer(TICK).await;
                tick = tick.wrapping_add(1);
            }
        })
    }

    /// Shells de las terminales abiertas: la principal de cada sesión y su acoplada (aunque esté
    /// oculta: sus agentes siguen corriendo).
    fn shell_pids(&self, cx: &App) -> Vec<ShellRef> {
        let mut out = Vec::new();
        for e in self.sessions.iter() {
            if let Some(pid) = e.value.view.read(cx).pid() {
                out.push(ShellRef {
                    session: e.id,
                    docked: false,
                    pid,
                });
            }
            if let Some(pid) = e.value.dock.as_ref().and_then(|d| d.read(cx).pid()) {
                out.push(ShellRef {
                    session: e.id,
                    docked: true,
                    pid,
                });
            }
        }
        out
    }

    fn apply_sample(&mut self, sample: Sample, slow: Option<SlowSample>, cx: &mut Context<Self>) {
        self.running = sample.agents;
        self.busy = sample.busy;
        self.ram = sample.ram;
        self.poll_exact(cx);
        if let Some(slow) = slow {
            self.installed = slow.installed;
            self.codex_limits = slow.codex_limits;
        }
        self.check_alerts(cx);
        self.sync_files(false, cx);
        cx.notify();
    }

    /// La terminal (principal o acoplada) de una sesión.
    fn terminal(&self, session: u64, docked: bool) -> Option<&Entity<TerminalView>> {
        let e = self.sessions.get(session)?;
        if docked {
            e.value.dock.as_ref()
        } else {
            Some(&e.value.view)
        }
    }

    /// Agentes corriendo, con el nombre de su sesión y el estado por actividad de su terminal.
    fn agent_rows(&self, cx: &App) -> Vec<AgentRow> {
        let rows = self
            .running
            .iter()
            .filter_map(|(sid, docked, agent)| {
                let entry = self.sessions.get(*sid)?;
                let view = self.terminal(*sid, *docked)?;
                Some(AgentRow {
                    session_id: *sid,
                    session_name: entry.name.clone(),
                    docked: *docked,
                    agent: agent.agent.clone(),
                    name: self.agent_name(&agent.agent),
                    pid: agent.pid,
                    state: state_for(view.read(cx).last_output_age()),
                    started_at: agent.started_at,
                    exact: None,
                })
            })
            .collect();
        self.with_exact(rows)
    }

    fn agent_name(&self, id: &str) -> String {
        self.catalog
            .get(id)
            .map_or_else(|| "Agente".to_string(), |a| a.name.clone())
    }

    // ── Alertas (§12) ───────────────────────────────────────────────────────

    /// ¿Se está viendo la pestaña? Activa, sin otra vista encima y con la ventana enfocada.
    fn session_visible(&self, id: u64) -> bool {
        self.window_active && !self.usage.open && !self.audit.open && self.sessions.active_id() == Some(id)
    }

    /// Compara la lectura nueva con la anterior: insignia en la pestaña y notificación de Windows
    /// para cada agente que quedó esperando o terminó en una pestaña que no se ve.
    fn check_alerts(&mut self, cx: &App) {
        let seen: Vec<Observation> = self
            .agent_rows(cx)
            .into_iter()
            .map(|r| Observation {
                key: AgentKey {
                    session: r.session_id,
                    docked: r.docked,
                    pid: r.pid,
                },
                exact: r.exact_state(),
                agent: r.agent,
                state: r.state,
            })
            .collect();
        let open: HashSet<u64> = self.sessions.iter().map(|e| e.id).collect();
        let visible: Vec<u64> = open.iter().copied().filter(|&id| self.session_visible(id)).collect();
        let alerts = self
            .alerts
            .update(&seen, |id| visible.contains(&id), |id| open.contains(&id));
        if alerts.is_empty() {
            return;
        }
        let toasts = crate::notify::toasts_allowed_now(self.settings.alerts);
        for alert in alerts {
            *self.badges.entry(alert.key.session).or_default() += 1;
            if toasts {
                let session = self
                    .sessions
                    .get(alert.key.session)
                    .map(|e| e.name.clone())
                    .unwrap_or_default();
                let place = crate::agents_panel::place(&session, alert.key.docked);
                let name = self.agent_name(&alert.agent);
                let title = match alert.kind {
                    AlertKind::Waiting => format!("{name} espera tu respuesta"),
                    AlertKind::Permission => format!("{name} pide permiso"),
                    AlertKind::Finished => format!("{name} terminó"),
                };
                crate::notify::show_toast(title, format!("Sesión «{place}»"));
            }
        }
    }

    /// Al ver una pestaña, su insignia se limpia.
    fn clear_visible_badge(&mut self) {
        if let Some(id) = self.sessions.active_id().filter(|&id| self.session_visible(id)) {
            self.badges.remove(&id);
        }
    }

    // ── Abrir agentes (§3) ──────────────────────────────────────────────────

    /// Clic en un agente instalado (o "Agentes: abrir …" en la paleta): escribe su comando y Enter en
    /// la terminal con foco de la sesión activa. Si esa terminal tiene otro programa corriendo (por
    /// ejemplo, otro agente), se abre una sesión nueva: nunca se le manda texto a algo que no es el
    /// shell.
    fn launch_agent(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(agent) = self.catalog.get(id).cloned() else {
            return;
        };
        let target = self.sessions.active_id().map(|sid| (sid, self.dock_has_focus(sid)));
        let free = target.filter(|t| !self.busy.contains(t));
        let (session, docked, delay) = match free {
            Some((sid, docked)) => (sid, docked, Duration::ZERO),
            None => {
                self.new_session(window, cx);
                let Some(sid) = self.sessions.active_id() else { return };
                (sid, false, Duration::from_millis(1_500))
            }
        };
        let shell = self.sessions.get(session).and_then(|e| e.value.shell.clone());
        // F10: con la configuración MCP y las opciones del perfil de la sesión.
        let line = self.agent_launch_line(session, &agent, ShellFlavor::from_program(shell.as_deref()), cx);
        if let Some(view) = self.terminal(session, docked) {
            view.read(cx).send_line(line, delay);
        }
        // Hasta la próxima lectura, la terminal cuenta como ocupada (un segundo clic no escribe encima).
        self.busy.insert((session, docked));
        self.usage.open = false;
        self.audit.open = false;
        self.sessions.activate(session);
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Vuelve a leer los agentes extra de `config.toml` y busca los instalados (en segundo plano).
    fn refresh_agents(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = &self.config_path {
            let saved = Settings::load(path);
            self.settings.agents = saved.agents;
            self.settings.alerts = saved.alerts;
        }
        self.catalog = Arc::new(catalog_from(&self.settings));
        let catalog = self.catalog.clone();
        cx.spawn(async move |this, cx| {
            let slow = cx
                .background_executor()
                .spawn(async move { slow_sample(&catalog) })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.installed = slow.installed;
                w.codex_limits = slow.codex_limits;
                cx.notify();
            });
        })
        .detach();
    }

    /// Carpeta actual de la sesión activa (OSC 7 / OSC 9;9). Las remotas no: el panel "Archivos" lee
    /// el disco local y nunca debe mezclar local con remoto (el árbol remoto llega en F7).
    fn active_cwd(&self, cx: &App) -> Option<PathBuf> {
        self.sessions
            .active()
            .filter(|e| e.value.remote.is_none())
            .and_then(|e| e.value.view.read(cx).cwd())
    }

    // ── Sesiones ────────────────────────────────────────────────────────────

    fn add_session(
        &mut self,
        name: Option<String>,
        shell: Option<String>,
        dock_ratio: Option<f32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let opts = Self::pane_options(shell.clone(), self.exact.next_session_dir.clone());
        let name = name.unwrap_or_else(|| {
            let existing: Vec<&str> = self.sessions.iter().map(|e| e.name.as_str()).collect();
            next_default_name("PowerShell", &existing)
        });
        let view = self.new_terminal(opts, window, cx);
        let events = cx.subscribe_in(&view, window, |this, view, ev: &TerminalEvent, window, cx| match ev {
            TerminalEvent::TitleChanged => cx.notify(),
            TerminalEvent::Exited => {
                let id = this.sessions.iter().find(|e| &e.value.view == view).map(|e| e.id);
                if let Some(id) = id {
                    this.close_session(id, window, cx);
                }
            }
        });
        let id = self.sessions.push(
            name,
            Session {
                view,
                shell,
                dock: None,
                dock_state: DockState::with_ratio(dock_ratio),
                remote: None,
                _events: events,
                _dock_events: None,
            },
        );
        self.mcp_new_session(id);
        cx.notify();
    }

    fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_session(None, None, None, window, cx);
        self.persist();
        self.usage.open = false;
        self.audit.open = false;
        self.focus_active(window, cx);
    }

    fn rename_session(&mut self, id: u64, name: String, cx: &mut Context<Self>) {
        self.sessions.rename(id, name);
        self.persist();
        cx.notify();
    }

    /// Cierra la sesión (su PTY y el de su acoplada terminan al soltar las vistas). Cerrar la última cierra la ventana.
    fn close_session(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.sessions.close(id).is_none() {
            return;
        }
        self.badges.remove(&id);
        self.mcp_close_session(id);
        self.persist();
        if self.sessions.is_empty() {
            window.remove_window();
            return;
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Activa una sesión y la muestra: si la vista de uso o de auditoría estaba abierta, vuelve a la
    /// terminal (también al hacer clic en la pestaña que ya estaba activa).
    fn activate(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let was_usage = std::mem::take(&mut self.usage.open);
        let was_audit = std::mem::take(&mut self.audit.open);
        if self.sessions.activate(id) || was_usage || was_audit {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    /// Lleva el teclado a la terminal activa —la principal o la acoplada, la que lo tuvo por última
    /// vez— (y el panel Archivos a su carpeta). Con la vista de uso o de auditoría abierta, el teclado
    /// queda en la ventana (Esc la cierra) y no en la terminal oculta. Ver la pestaña limpia su
    /// insignia.
    fn focus_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_visible_badge();
        if self.usage.open {
            window.focus(&self.focus, cx);
            return;
        }
        if self.audit.open {
            window.focus(&self.audit.search_focus, cx);
            return;
        }
        if let Some(view) = self.sessions.active_id().and_then(|id| self.focus_target(id)) {
            view.update(cx, |v, cx| v.focus(window, cx));
        }
        self.sync_files(false, cx);
    }

    /// Doble clic en una pestaña: el campo de la paleta pasa a pedir el nombre nuevo.
    fn start_rename(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.sessions.iter().find(|e| e.id == id).map(|e| e.name.clone()) else {
            return;
        };
        self.open_palette(window, cx);
        if let Some(p) = self.palette.as_mut() {
            p.start_rename(id, &name);
        }
        cx.notify();
    }

    // ── Panel izquierdo ─────────────────────────────────────────────────────

    fn set_panel(&mut self, view: PanelView, cx: &mut Context<Self>) {
        self.panel = view;
        if view == PanelView::Files {
            self.sync_files(true, cx);
        }
        cx.notify();
    }

    /// "Archivos" alterna el contenido del panel; en el modo compacto, además lo abre completo.
    fn toggle_files(&mut self, cx: &mut Context<Self>) {
        if self.settings.sidebar == SidebarMode::Icons {
            self.set_sidebar(SidebarMode::Full, cx);
            self.set_panel(PanelView::Files, cx);
        } else if self.panel == PanelView::Files {
            self.set_panel(PanelView::Agents, cx);
        } else {
            self.set_panel(PanelView::Files, cx);
        }
    }

    /// Lleva el teclado al buscador de agentes (abre el panel completo si hace falta).
    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.sidebar == SidebarMode::Icons {
            self.set_sidebar(SidebarMode::Full, cx);
        }
        self.set_panel(PanelView::Agents, cx);
        window.focus(&self.search_focus, cx);
        cx.notify();
    }

    /// Teclado con el buscador enfocado. Devuelve `false` para lo que no le toca (atajos Ctrl+Shift).
    fn on_search_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let ks = &ev.keystroke;
        let m = ks.modifiers;
        match ks.key.to_ascii_lowercase().as_str() {
            "escape" => {
                if self.agent_query.is_empty() {
                    self.focus_active(window, cx);
                } else {
                    self.agent_query.clear();
                }
            }
            "enter" => {
                let rows = self.agent_rows(cx);
                let first = crate::agents_panel::filter_rows(&rows, self.agent_filter, &self.agent_query)
                    .first()
                    .map(|r| r.session_id);
                if let Some(id) = first {
                    self.activate(id, window, cx);
                }
            }
            "backspace" if m.control => self.agent_query.clear(),
            "backspace" => {
                self.agent_query.pop();
            }
            "v" if m.control && !m.alt && !m.shift => {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    let line = text.lines().next().unwrap_or("");
                    self.agent_query.extend(line.chars().filter(|c| !c.is_control()));
                }
            }
            _ if m.control && !m.alt => return false,
            _ => {
                if let Some(text) = ks.key_char.as_deref() {
                    if self.agent_query.chars().count() < 60 {
                        self.agent_query.extend(text.chars().filter(|c| !c.is_control()));
                    }
                } else {
                    return false;
                }
            }
        }
        cx.notify();
        true
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        let mode = match self.settings.sidebar {
            SidebarMode::Icons => SidebarMode::Full,
            SidebarMode::Full => SidebarMode::Icons,
        };
        self.set_sidebar(mode, cx);
    }

    fn set_sidebar(&mut self, mode: SidebarMode, cx: &mut Context<Self>) {
        self.settings.sidebar = mode;
        self.persist();
        cx.notify();
    }

    // ── Fuente ──────────────────────────────────────────────────────────────

    /// Cambia el tamaño de letra de todas las terminales (9–28 pt) y lo guarda.
    fn set_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        let size = Settings::clamp_font(size);
        if (size - self.settings.font_size).abs() < f32::EPSILON {
            return;
        }
        self.settings.font_size = size;
        for view in self.all_terminals() {
            view.update(cx, |v, cx| v.set_font(self.mono_font.clone(), size, cx));
        }
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
        for view in self.all_terminals() {
            view.update(cx, |v, cx| {
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

    /// Todas las terminales: la principal y la acoplada (aunque esté oculta) de cada sesión.
    fn all_terminals(&self) -> Vec<Entity<TerminalView>> {
        self.sessions
            .iter()
            .flat_map(|e| std::iter::once(e.value.view.clone()).chain(e.value.dock.clone()))
            .collect()
    }

    /// Guarda la configuración (tema, riel, fuente y sesiones con nombre, shell y alto acoplado). En las pruebas
    /// de humo `config_path` es `None` y no se toca nada.
    fn persist(&mut self) {
        self.settings.sessions = self
            .sessions
            .iter()
            .map(|e| {
                let (mcp, profile) = self.mcp_saved(e.id);
                SessionConfig {
                    name: e.name.clone(),
                    shell: e.value.shell.clone(),
                    dock_ratio: e.value.dock_state.saved_ratio(),
                    connection: e.value.remote.as_ref().map(|r| r.connection.clone()),
                    cwd: e.value.remote.as_ref().and_then(|r| r.main.cwd()),
                    mcp,
                    profile,
                }
            })
            .collect();
        if let Some(path) = &self.config_path {
            let _ = self.settings.save(path);
        }
    }

    // ── Paleta de comandos ──────────────────────────────────────────────────

    fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let agents = self
            .installed
            .iter()
            .filter_map(|id| self.catalog.get(id).map(|a| (a.id.clone(), a.name.clone())))
            .collect();
        self.connections.open = false;
        self.mcp.open = false;
        self.palette = Some(PaletteState::with_agents(agents));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        self.focus_active(window, cx);
        cx.notify();
    }

    fn toggle_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            self.close_palette(window, cx);
        } else {
            self.open_palette(window, cx);
        }
    }

    /// Ejecuta la acción elegida. "Renombrar" no cierra: el mismo campo pasa a pedir el nombre.
    fn run_action(&mut self, action: PaletteAction, window: &mut Window, cx: &mut Context<Self>) {
        if action == PaletteAction::RenameSession {
            if let (Some(e), Some(p)) = (self.sessions.active(), self.palette.as_mut()) {
                p.start_rename(e.id, &e.name);
            }
            cx.notify();
            return;
        }
        let launch = match action {
            PaletteAction::LaunchAgent(i) => self.palette.as_ref().and_then(|p| p.agents.get(i).cloned()),
            _ => None,
        };
        let worktree_agent = match action {
            PaletteAction::LaunchInWorktree(i) => self.palette.as_ref().and_then(|p| p.agents.get(i).cloned()),
            _ => None,
        };
        self.close_palette(window, cx);
        match action {
            PaletteAction::NewSession => self.new_session(window, cx),
            PaletteAction::CloseSession => {
                if let Some(id) = self.sessions.active_id() {
                    self.close_session(id, window, cx);
                }
            }
            PaletteAction::NextSession => self.cycle(true, window, cx),
            PaletteAction::PrevSession => self.cycle(false, window, cx),
            PaletteAction::ToggleDock => self.toggle_dock(window, cx),
            PaletteAction::Theme(id) => self.set_theme(id, window, cx),
            PaletteAction::RailFull => self.set_sidebar(SidebarMode::Full, cx),
            PaletteAction::RailIcons => self.set_sidebar(SidebarMode::Icons, cx),
            PaletteAction::PanelAgents => self.set_panel(PanelView::Agents, cx),
            PaletteAction::PanelFiles => {
                if self.settings.sidebar == SidebarMode::Icons {
                    self.set_sidebar(SidebarMode::Full, cx);
                }
                self.set_panel(PanelView::Files, cx);
            }
            PaletteAction::FontBigger => self.set_font_size(self.settings.font_size + FONT_STEP, cx),
            PaletteAction::FontSmaller => self.set_font_size(self.settings.font_size - FONT_STEP, cx),
            PaletteAction::FontReset => self.set_font_size(Settings::default().font_size, cx),
            PaletteAction::Usage => self.toggle_usage(window, cx),
            PaletteAction::ExportUsage => {
                self.open_usage(window, cx);
                self.export_usage(cx);
            }
            PaletteAction::EditPrices => {
                self.open_usage(window, cx);
                self.edit_prices(cx);
            }
            PaletteAction::Audit => self.toggle_audit(window, cx),
            PaletteAction::ExportAudit => {
                self.open_audit(window, cx);
                self.export_audit(cx);
            }
            PaletteAction::RefreshAgents => self.refresh_agents(cx),
            PaletteAction::LaunchAgent(_) => {
                if let Some(id) = launch {
                    self.launch_agent(&id, window, cx);
                }
            }
            PaletteAction::LaunchInWorktree(_) => {
                if let Some(id) = worktree_agent {
                    self.launch_in_worktree(&id, window, cx);
                }
            }
            PaletteAction::SendSelectionToDock => self.send_selection_to_dock(window, cx),
            PaletteAction::SendSuggestion => self.send_suggestion(window, cx),
            PaletteAction::InstallClaudeHooks => self.open_installer(cx),
            PaletteAction::Connections => self.open_connections(window, cx),
            PaletteAction::Mcp => self.open_mcp(window, cx),
            PaletteAction::NewConnection => self.new_connection_form(window, cx),
            PaletteAction::Reconnect => self.reconnect_active(cx),
            PaletteAction::RenameSession => {}
        }
    }

    /// Botón de tema (§4 de la especificación): abre la paleta filtrada en los temas, con el actual
    /// marcado; volver a pulsarlo la cierra. Elegir uno lo aplica y lo guarda.
    fn toggle_theme_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            self.close_palette(window, cx);
            return;
        }
        self.open_palette(window, cx);
        if let Some(p) = self.palette.as_mut() {
            p.insert("Tema");
        }
        cx.notify();
    }

    /// ¿La acción describe el estado actual? (tema y panel en uso: se marcan con ✓).
    fn is_current(&self, action: PaletteAction) -> bool {
        match action {
            PaletteAction::Theme(id) => id == self.theme_id,
            PaletteAction::RailFull => self.settings.sidebar == SidebarMode::Full,
            PaletteAction::RailIcons => self.settings.sidebar == SidebarMode::Icons,
            PaletteAction::PanelAgents => self.panel == PanelView::Agents,
            PaletteAction::PanelFiles => self.panel == PanelView::Files,
            PaletteAction::Usage => self.usage.open,
            PaletteAction::Audit => self.audit.open,
            PaletteAction::ToggleDock => self.dock_open(),
            _ => false,
        }
    }

    /// Teclado con la paleta abierta: todo se queda en el campo (nada llega a la terminal).
    fn on_palette_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.palette.as_mut() else {
            return;
        };
        let ks = &ev.keystroke;
        let m = ks.modifiers;
        match ks.key.to_ascii_lowercase().as_str() {
            "escape" => self.close_palette(window, cx),
            "enter" => match p.mode.clone() {
                PaletteMode::Search => {
                    if let Some(action) = p.selected_action() {
                        self.run_action(action, window, cx);
                    }
                }
                PaletteMode::Rename { id, .. } => {
                    if let Some(name) = p.rename_value() {
                        self.close_palette(window, cx);
                        self.rename_session(id, name, cx);
                    }
                }
            },
            "up" => p.select_prev(),
            "down" => p.select_next(),
            "tab" if m.shift => p.select_prev(),
            "tab" => p.select_next(),
            "backspace" if m.control => p.clear(),
            "backspace" => p.backspace(),
            "p" if m.control && m.shift => self.close_palette(window, cx),
            "v" if m.control && !m.alt => {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    p.insert(&text);
                }
            }
            // AltGr llega como Ctrl+Alt: también escribe (por ejemplo "@").
            _ if !m.control || m.alt => {
                if let Some(text) = ks.key_char.as_deref() {
                    p.insert(text);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    fn cycle(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.usage.open = false;
        self.audit.open = false;
        if forward {
            self.sessions.next();
        } else {
            self.sessions.prev();
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    // ── Teclado ─────────────────────────────────────────────────────────────

    /// Atajos de Kuntur. Se capturan antes que la terminal: con Ctrl+Shift no chocan con el shell.
    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            self.on_palette_key(ev, window, cx);
            cx.stop_propagation();
            return;
        }
        if self.connections.open {
            self.on_connections_key(ev, window, cx);
            cx.stop_propagation();
            return;
        }
        if self.mcp.open {
            self.on_mcp_key(ev, window, cx);
            cx.stop_propagation();
            return;
        }
        if self.search_focus.is_focused(window) && self.on_search_key(ev, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.audit.open && self.on_audit_key(ev, window, cx) {
            cx.stop_propagation();
            return;
        }
        let m = ev.keystroke.modifiers;
        // F8: Esc cierra el menú de enlaces; sin menú sigue a la terminal (y al autocompletado, F9).
        if ev.keystroke.key == "escape" && !m.modified() && self.close_link_menu(cx) {
            cx.stop_propagation();
            return;
        }
        if self.usage.open && ev.keystroke.key == "escape" && !m.modified() {
            self.close_usage(window, cx);
            cx.stop_propagation();
            return;
        }
        if is_toggle_key(&ev.keystroke.key, m.control, m.shift, m.alt) && !m.platform {
            self.toggle_dock(window, cx);
            cx.stop_propagation();
            return;
        }
        // F10: comandos con atajo (de la sesión o globales).
        if !m.platform && self.run_shortcut_command(ev, window, cx) {
            cx.stop_propagation();
            return;
        }
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
            (false, "tab") => self.cycle(true, window, cx),
            (true, "tab") => self.cycle(false, window, cx),
            (true, "b") => self.toggle_sidebar(cx),
            (true, "e") => self.toggle_files(cx),
            (true, "p") => self.toggle_palette(window, cx),
            (true, "u") => self.toggle_usage(window, cx),
            (true, "a") => self.toggle_audit(window, cx),
            (true, "o") => self.toggle_connections(window, cx),
            (true, "m") => self.toggle_mcp(window, cx),
            (true, "r") => self.reconnect_active(cx),
            (_, "=" | "+") => self.set_font_size(self.settings.font_size + FONT_STEP, cx),
            (false, "-") => self.set_font_size(self.settings.font_size - FONT_STEP, cx),
            (false, "0") => self.set_font_size(Settings::default().font_size, cx),
            (false, d) if d.len() == 1 && ('1'..='9').contains(&d.chars().next().unwrap_or('0')) => {
                let n = d.parse::<usize>().unwrap_or(1);
                if self.sessions.activate_index(n - 1) {
                    self.usage.open = false;
                    self.audit.open = false;
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

/// Barra de acento de 3 px pegada al borde izquierdo (elemento activo).
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

impl Workspace {
    /// Paleta flotante centrada arriba: campo de filtro y lista de acciones. Un clic fuera la cierra.
    fn render_palette(&self, p: &PaletteState, cx: &mut Context<Self>) -> AnyElement {
        let c = self.colors.clone();
        let renaming = match &p.mode {
            PaletteMode::Rename { current, .. } => Some(current.clone()),
            PaletteMode::Search => None,
        };
        let placeholder = if renaming.is_some() {
            "Nombre de la sesión"
        } else {
            "Buscar una acción"
        };
        let caret = div().w(px(1.5)).h(px(17.)).flex_shrink_0().bg(c.accent);

        // Campo: ícono, texto (o guía tenue) y cursor.
        let field = h_flex()
            .h(px(46.))
            .px(px(16.))
            .gap(px(10.))
            .items_center()
            .border_b_1()
            .border_color(c.border)
            .child(
                Icon::new(if renaming.is_some() {
                    IconName::SquareTerminal
                } else {
                    IconName::Search
                })
                .size(px(16.))
                .text_color(c.muted),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .text_size(px(14.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .map(|this| {
                        if p.query.is_empty() {
                            this.child(caret)
                                .child(div().ml(px(1.)).text_color(c.muted).child(placeholder))
                        } else if p.replace_on_type {
                            // Nombre propuesto, "seleccionado": lo que se escriba lo reemplaza.
                            this.child(
                                div()
                                    .px(px(2.))
                                    .rounded(px(3.))
                                    .bg(c.accent.opacity(0.28))
                                    .text_color(c.text)
                                    .child(p.query.clone()),
                            )
                            .child(caret)
                        } else {
                            this.child(div().text_color(c.text).child(p.query.clone())).child(caret)
                        }
                    }),
            );

        let body: AnyElement = match renaming {
            Some(current) => h_flex()
                .h(px(40.))
                .px(px(16.))
                .items_center()
                .justify_between()
                .text_size(px(12.))
                .text_color(c.muted)
                .child(format!("Renombrar «{current}»"))
                .child(div().text_size(px(11.5)).child("Enter guarda · Esc cancela"))
                .into_any_element(),
            None => {
                let items = p.filtered();
                if items.is_empty() {
                    div()
                        .py(px(18.))
                        .text_center()
                        .text_size(px(12.5))
                        .text_color(c.muted)
                        .child("Sin resultados")
                        .into_any_element()
                } else {
                    let rows: Vec<AnyElement> = items
                        .iter()
                        .enumerate()
                        .map(|(i, item)| self.palette_row(i, item, i == p.selected, cx))
                        .collect();
                    v_flex()
                        .id("palette-list")
                        .max_h(px(430.))
                        .overflow_y_scroll()
                        .p(px(6.))
                        .gap(px(1.))
                        .children(rows)
                        .into_any_element()
                }
            }
        };

        // El foco vive en el panel (no en la raíz): un clic en la barra o el riel no le quita el
        // teclado a la terminal.
        let panel = v_flex()
            .id("palette")
            .track_focus(&self.focus)
            .w(px(PALETTE_WIDTH))
            .max_w(relative(0.92))
            .bg(c.popover)
            .border_1()
            .border_color(c.border)
            .rounded(px(10.))
            .shadow_lg()
            .overflow_hidden()
            .occlude()
            .font_family(self.ui_font.clone())
            .child(field)
            .child(body);

        deferred(
            div()
                .id("palette-backdrop")
                .absolute()
                .top(TITLE_BAR_HEIGHT)
                .left_0()
                .right_0()
                .bottom_0()
                .flex()
                .items_start()
                .justify_center()
                .pt(px(10.))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.close_palette(window, cx)),
                )
                .child(panel),
        )
        .with_priority(1)
        .into_any_element()
    }

    /// Fila de la paleta: "Grupo:" en tenue + acción; a la derecha ✓ (si es lo actual) y el atajo.
    fn palette_row(&self, index: usize, item: &PaletteItem, selected: bool, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let action = item.action;
        let (group, label) = match item.label.split_once(": ") {
            Some((g, l)) => (Some(format!("{g}:")), l.to_string()),
            None => (None, item.label.to_string()),
        };
        let hover = c.hover;
        h_flex()
            .id(("palette-item", index))
            .h(px(32.))
            .px(px(10.))
            .gap(px(5.))
            .items_center()
            .rounded(px(6.))
            .cursor_pointer()
            .text_size(px(13.))
            .map(|this| {
                if selected {
                    this.bg(c.accent.opacity(0.16))
                } else {
                    this.hover(move |s| s.bg(hover))
                }
            })
            .on_click(cx.listener(move |this, _, window, cx| this.run_action(action, window, cx)))
            .when_some(group, |this, g| this.child(div().text_color(c.muted).child(g)))
            .child(div().flex_1().min_w_0().text_color(c.text).child(label))
            .when(self.is_current(action), |this| {
                this.child(Icon::new(IconName::Check).size(px(14.)).text_color(c.accent))
            })
            .when_some(item.shortcut, |this, s| {
                this.child(
                    div()
                        .ml(px(8.))
                        .text_size(px(11.5))
                        .font_family(self.mono_font.clone())
                        .text_color(c.muted)
                        .child(s),
                )
            })
            .into_any_element()
    }
}

impl Workspace {
    /// Línea inferior de 22 px: carpeta actual a la izquierda y la RAM de Kuntur a la derecha.
    fn render_status_line(&self, cx: &App) -> impl IntoElement {
        let c = &self.colors;
        let remote = self.sessions.active_id().and_then(|id| self.remote_label(id));
        let cwd = self.focused_cwd(cx).map(|p| match &remote {
            Some(r) => format!("{r}:{}", p.display()),
            None => kuntur_files::display_path(&p, self.home.as_deref()),
        });
        let ram = self
            .ram
            .map(|b| format!("Kuntur · {} MB", (b as f64 / (1024.0 * 1024.0)).round() as u64));
        h_flex()
            .h(px(STATUS_LINE))
            .flex_shrink_0()
            .px(px(12.))
            .gap(px(12.))
            .items_center()
            // Mismo fondo que la terminal: sin él se ve el degradado de la ventana sin oscurecer
            // (en Blue, una franja más clara que el resto).
            .bg(c.terminal)
            .border_t_1()
            .border_color(c.border)
            .text_size(px(11.))
            .text_color(c.muted)
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(6.))
                    .items_center()
                    .when_some(cwd, |this, cwd| {
                        this.child(Icon::new(IconName::Folder).size(px(12.)).flex_shrink_0())
                            .child(
                                div()
                                    .min_w_0()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(cwd),
                            )
                    }),
            )
            .when_some(ram, |this, ram| {
                this.child(
                    div()
                        .id("ram")
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .tooltip(|window, cx| Tooltip::new("Memoria de trabajo de Kuntur").build(window, cx))
                        .child(ram),
                )
            })
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = self.colors.clone();
        let palette = self.palette.as_ref().map(|p| self.render_palette(p, cx));
        let connections = self.connections.open.then(|| self.render_connections(cx));
        let mcp = self.mcp.open.then(|| self.render_mcp(cx));
        let link_menu = self.render_link_menu(cx);
        self.sync_web_visibility();
        v_flex()
            .relative()
            .size_full()
            .bg(c.window)
            .text_color(c.text)
            .font_family(self.ui_font.clone())
            .text_size(px(UI_TEXT))
            .capture_key_down(cx.listener(Self::on_key))
            // Soltar el separador de la acoplada: se guarda su alto.
            .on_drop(cx.listener(|this, _: &DockDrag, _, cx| {
                this.persist();
                cx.notify();
            }))
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_side_panel(window, cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(div().flex_1().min_h_0().map(|this| {
                                if self.usage.open {
                                    this.child(self.render_usage(cx))
                                } else if self.audit.open {
                                    this.child(self.render_audit(window, cx))
                                } else {
                                    let terminals = self.render_terminals(cx);
                                    this.child(self.render_with_preview(terminals, cx))
                                }
                            }))
                            .child(self.render_status_line(cx)),
                    ),
            )
            .children(palette)
            .children(self.render_exact_overlay(cx))
            .children(connections)
            .children(mcp)
            .children(link_menu)
    }
}
