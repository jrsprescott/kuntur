//! F5b · Agentes exactos: el named pipe de los ganchos (`kuntur-hook.exe`), el OSC 7501, el estado
//! exacto en las filas del panel (acción actual, subagentes, Permitir / Rechazar, comando sugerido),
//! "Enviar a terminal", el worktree por agente, el anillo de Claude (línea de estado) y el instalador
//! de los ganchos de Claude Code. La lógica pura vive en `kuntur-agents` y `kuntur-hook`.

use std::time::Instant;

use kuntur_agents::worktree::{self, WorktreePlan};
use kuntur_agents::{AgentState, Answer, ExactState, ExactTracker, answer_keys, can_answer, is_risky, one_line};
use kuntur_hook::install::{self, InstallPlan, StatusLineChange};
use kuntur_hook::{ENV_PIPE, ENV_SESSION, EventKind, HookEvent, StatusLimits};
use kuntur_term::SignalState;

use super::*;
use crate::dock::Slot;

/// Cuánto se ve un aviso abajo a la derecha.
const NOTICE_SECS: u64 = 6;
/// Espera antes de escribir en una terminal recién abierta (que su shell arranque).
const NEW_TERMINAL_DELAY: Duration = Duration::from_millis(1_500);
/// Subagentes que se muestran bajo un agente (los demás se cuentan).
const MAX_SUB_ROWS: usize = 8;

/// Instalador de los ganchos de Claude Code abierto.
struct Installer {
    settings: PathBuf,
    plan: Result<InstallPlan, String>,
    /// Resultado de "Instalar": la copia de seguridad, o el error.
    done: Option<Result<Option<PathBuf>, String>>,
}

/// Estado de F5b dentro del `Workspace`.
#[derive(Default)]
pub(super) struct ExactAgents {
    tracker: ExactTracker,
    /// Terminal (principal o acoplada) → su identificador `KUNTUR_SESSION`.
    tokens: HashMap<EntityId, String>,
    next_token: u64,
    /// Nombre del named pipe de esta instancia (`KUNTUR_PIPE`).
    pipe: String,
    /// Terminales donde se vio el proceso del agente: si desaparece, su estado exacto se olvida.
    seen_process: HashSet<String>,
    /// Límites de Claude que llegan por su línea de estado (anillo de "instalados").
    pub(super) claude_limits: Option<Limits>,
    /// Worktree de cada sesión abierta con "en un worktree nuevo": su acoplada arranca ahí.
    worktrees: HashMap<u64, PathBuf>,
    /// Carpeta de la próxima sesión que se abra.
    pub(super) next_session_dir: Option<PathBuf>,
    installer: Option<Installer>,
    notice: Option<(String, Instant)>,
    #[cfg(windows)]
    _server: Option<kuntur_hook::PipeServer>,
    _events: Option<Task<()>>,
}

impl ExactAgents {
    /// Diálogo "Instalar ganchos" abierto (la vista previa oculta el visor web mientras tanto).
    pub(super) fn installer_open(&self) -> bool {
        self.installer.is_some()
    }
}

/// Límites de la línea de estado de Claude en el formato del anillo (ventanas de 5 h y semanal).
fn limits_from(l: &StatusLimits) -> Limits {
    Limits {
        primary_pct: l.five_hour_pct,
        primary_window_mins: Some(300),
        primary_resets_at: l.five_hour_resets_at,
        secondary_pct: l.seven_day_pct,
        secondary_window_mins: Some(10_080),
        secondary_resets_at: l.seven_day_resets_at,
        ts: now_secs() as i64,
    }
}

fn signal_state(s: SignalState) -> Option<ExactState> {
    match s {
        SignalState::Working => Some(ExactState::Working),
        SignalState::Permission => Some(ExactState::Permission),
        SignalState::Waiting => Some(ExactState::Reply),
        SignalState::Done => Some(ExactState::Done),
        SignalState::End => None,
    }
}

/// `settings.json` de Claude Code que usa el instalador. `KUNTUR_CLAUDE_SETTINGS` lo redirige
/// (pruebas). En las pruebas de humo sin esa variable no hay ruta: nunca se toca el real.
fn claude_settings_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KUNTUR_CLAUDE_SETTINGS") {
        return Some(PathBuf::from(p));
    }
    let smoke = std::env::vars_os().any(|(k, _)| k.to_string_lossy().starts_with("KUNTUR_SMOKE_"));
    if smoke || cfg!(test) {
        return None;
    }
    home_dir().map(|h| h.join(".claude").join("settings.json"))
}

impl Workspace {
    // ── Pipe y eventos ──────────────────────────────────────────────────────

    /// Abre el named pipe de los ganchos y empieza a recibir sus eventos. Se llama antes de abrir
    /// las terminales (que reciben `KUNTUR_PIPE`).
    pub(super) fn start_exact(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.exact.pipe = kuntur_hook::pipe_name(std::process::id());
        let (tx, rx) = smol::channel::unbounded::<Vec<u8>>();
        #[cfg(windows)]
        {
            self.exact._server = kuntur_hook::PipeServer::start(&self.exact.pipe, move |m| {
                let _ = tx.try_send(m);
            })
            .ok();
        }
        #[cfg(not(windows))]
        drop(tx);
        self.exact._events = Some(cx.spawn(async move |this, cx| {
            while let Ok(msg) = rx.recv().await {
                let events = kuntur_hook::decode(&msg);
                if this.update(cx, |w, cx| w.apply_hook_events(events, cx)).is_err() {
                    break;
                }
            }
        }));

        // Prueba de humo: eventos SINTÉTICOS (nunca un agente real) que se mandan por el pipe real.
        if let Ok(path) = std::env::var("KUNTUR_SMOKE_AGENT_EVENTS") {
            self.config_path = None;
            self.smoke_agent_events(PathBuf::from(path), window, cx);
        }
        // Prueba de humo: "Enviar a terminal" del comando sugerido en la sesión n (como el clic), a los 7 s.
        if let Some(n) = std::env::var("KUNTUR_SMOKE_SEND_SUGGESTION")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
        {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(7)).await;
                let _ = this.update_in(cx, |w, window, cx| {
                    if w.sessions.activate_index(n.saturating_sub(1)) {
                        w.send_suggestion(window, cx);
                    }
                });
            })
            .detach();
        }
        // Prueba de humo: el instalador abierto contra un settings.json temporal.
        if std::env::var_os("KUNTUR_SMOKE_HOOKS_INSTALL").is_some() {
            self.config_path = None;
            self.open_installer(cx);
        }
    }

    /// Variables de una terminal nueva: el pipe y su identificador.
    pub(super) fn terminal_env(&mut self, opts: &mut PaneOptions) -> String {
        self.exact.next_token += 1;
        let token = format!("k{}", self.exact.next_token);
        if !self.exact.pipe.is_empty() {
            opts.env.push((ENV_PIPE.into(), self.exact.pipe.clone()));
            opts.env.push((ENV_SESSION.into(), token.clone()));
        }
        token
    }

    pub(super) fn register_terminal(&mut self, view: &Entity<TerminalView>, token: String) {
        self.exact.tokens.insert(view.entity_id(), token);
    }

    fn token_of(&self, session: u64, docked: bool) -> Option<&String> {
        let view = self.terminal(session, docked)?;
        self.exact.tokens.get(&view.entity_id())
    }

    /// Terminales abiertas con su identificador: `token → (sesión, ¿acoplada?)`.
    fn live_terminals(&self) -> HashMap<String, (u64, bool)> {
        let mut live = HashMap::new();
        for e in self.sessions.iter() {
            for (docked, view) in [(false, Some(&e.value.view)), (true, e.value.dock.as_ref())] {
                if let Some(token) = view.and_then(|v| self.exact.tokens.get(&v.entity_id())) {
                    live.insert(token.clone(), (e.id, docked));
                }
            }
        }
        live
    }

    fn apply_hook_events(&mut self, events: Vec<HookEvent>, cx: &mut Context<Self>) {
        let now = now_secs();
        let mut changed = false;
        for ev in events {
            if ev.kind == EventKind::StatusLine {
                if let Some(l) = &ev.limits {
                    self.exact.claude_limits = Some(limits_from(l));
                    changed = true;
                }
                continue;
            }
            changed |= self.exact.tracker.apply(&ev, now);
        }
        if changed {
            cx.notify();
        }
    }

    /// En cada lectura del monitor: OSC 7501 de cada terminal, olvidar terminales cerradas y el
    /// estado de los agentes cuyo proceso desapareció (se cerraron sin `SessionEnd`).
    pub(super) fn poll_exact(&mut self, cx: &App) {
        let now = now_secs();
        let live = self.live_terminals();
        for e in self.sessions.iter() {
            for view in [Some(&e.value.view), e.value.dock.as_ref()].into_iter().flatten() {
                let Some(token) = self.exact.tokens.get(&view.entity_id()) else {
                    continue;
                };
                if let Some(sig) = view.read(cx).take_agent_signal() {
                    self.exact.tracker.apply_signal(
                        token,
                        signal_state(sig.state),
                        sig.agent.as_deref(),
                        sig.action,
                        now,
                    );
                }
            }
        }
        self.exact.tracker.retain(|t| live.contains_key(t));
        self.exact.tokens.retain(|_, t| live.contains_key(t));
        self.exact.worktrees.retain(|sid, _| self.sessions.get(*sid).is_some());
        for (token, (sid, docked)) in &live {
            let has = self.running.iter().any(|(s, d, _)| s == sid && d == docked);
            if has {
                self.exact.seen_process.insert(token.clone());
            } else if self.exact.seen_process.remove(token) {
                self.exact.tracker.forget(token);
            }
        }
        self.exact.seen_process.retain(|t| live.contains_key(t));
    }

    /// Agrega el estado exacto a las filas del monitor y las filas de agentes que solo se conocen por
    /// eventos (sin proceso reconocido; p. ej. por OSC 7501).
    pub(super) fn with_exact(&self, mut rows: Vec<AgentRow>) -> Vec<AgentRow> {
        let mut used: HashSet<&String> = HashSet::new();
        for r in &mut rows {
            let Some(token) = self.token_of(r.session_id, r.docked) else {
                continue;
            };
            if used.contains(token) {
                continue;
            }
            if let Some(p) = self.exact.tracker.get(token)
                && (p.agent == r.agent || self.catalog.get(&p.agent).is_none())
            {
                r.state = p.state.coarse();
                r.exact = Some(p.clone());
                used.insert(token);
            }
        }
        let live = self.live_terminals();
        let order: HashMap<u64, usize> = self.sessions.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
        let mut extra: Vec<AgentRow> = self
            .exact
            .tracker
            .panes()
            .filter(|(token, _)| !used.contains(token))
            .filter_map(|(token, p)| {
                let &(sid, docked) = live.get(token)?;
                if rows
                    .iter()
                    .any(|r| r.session_id == sid && r.docked == docked && r.agent == p.agent)
                {
                    return None;
                }
                Some(AgentRow {
                    session_id: sid,
                    session_name: self.sessions.get(sid)?.name.clone(),
                    docked,
                    agent: p.agent.clone(),
                    name: self.agent_name(&p.agent),
                    pid: 0,
                    state: p.state.coarse(),
                    started_at: p.since,
                    exact: Some(p.clone()),
                })
            })
            .collect();
        extra.sort_by_key(|r| (order.get(&r.session_id).copied().unwrap_or(usize::MAX), r.docked));
        rows.extend(extra);
        rows
    }

    /// Estado de la pestaña: esperando si algún agente de la sesión espera; si no, trabajando.
    pub(super) fn exact_session_state(&self, id: u64, cx: &App) -> Option<AgentState> {
        let rows: Vec<AgentRow> = self.agent_rows(cx).into_iter().filter(|r| r.session_id == id).collect();
        if rows.is_empty() {
            None
        } else if rows.iter().any(|r| r.state == AgentState::Waiting) {
            Some(AgentState::Waiting)
        } else {
            Some(AgentState::Working)
        }
    }

    // ── Permitir / Rechazar ─────────────────────────────────────────────────

    /// Clic en Permitir / Rechazar de una fila: escribe la respuesta en la terminal del agente.
    fn answer_permission(&mut self, session: u64, docked: bool, agent: &str, answer: Answer, cx: &mut Context<Self>) {
        let Some(keys) = answer_keys(agent, answer) else {
            return;
        };
        if let Some(view) = self.terminal(session, docked) {
            view.read(cx).write_bytes(keys);
        }
        if let Some(token) = self.token_of(session, docked).cloned() {
            match answer {
                Answer::Allow => self.exact.tracker.answered(&token, now_secs()),
                Answer::Deny => {
                    self.exact
                        .tracker
                        .apply_signal(&token, Some(ExactState::Reply), None, None, now_secs())
                }
            }
        }
        cx.notify();
    }

    // ── Enviar a terminal ───────────────────────────────────────────────────

    /// Escribe `text` en la terminal acoplada de la sesión (la abre si hace falta), **sin Enter**.
    fn send_to_dock(&mut self, session: u64, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let shell = self.sessions.get(session).and_then(|e| e.value.shell.clone());
        let line = one_line(text, ShellFlavor::from_program(shell.as_deref()));
        if line.is_empty() {
            return;
        }
        if self.busy.contains(&(session, true)) {
            self.show_notice(
                "La terminal acoplada tiene otro programa corriendo: no se le escribe encima",
                cx,
            );
            return;
        }
        self.usage.open = false;
        self.audit.open = false;
        self.sessions.activate(session);
        let existed = self.sessions.get(session).is_some_and(|e| e.value.dock.is_some());
        if let Some(e) = self.sessions.get_mut(session) {
            if e.value.dock_state.open {
                e.value.dock_state.set_focus(Slot::Dock);
            } else {
                e.value.dock_state.toggle(Slot::Main);
            }
        }
        self.ensure_dock(session, window, cx);
        let delay = if existed { Duration::ZERO } else { NEW_TERMINAL_DELAY };
        if let Some(dock) = self.sessions.get(session).and_then(|e| e.value.dock.clone()) {
            dock.read(cx).paste_text(line, delay);
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Paleta: la selección de la terminal principal de la sesión activa, a su acoplada.
    pub(super) fn send_selection_to_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.sessions.active() else {
            return;
        };
        let (id, text) = (entry.id, entry.value.view.read(cx).selection_text());
        match text.filter(|t| !t.trim().is_empty()) {
            Some(text) => self.send_to_dock(id, &text, window, cx),
            None => self.show_notice("No hay texto seleccionado en la terminal", cx),
        }
    }

    /// Paleta: el último comando que sugirió un agente de la sesión activa, a su acoplada.
    pub(super) fn send_suggestion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.sessions.active_id() else {
            return;
        };
        let cmd = self
            .agent_rows(cx)
            .into_iter()
            .filter(|r| r.session_id == id)
            .filter_map(|r| r.exact.and_then(|e| e.suggestions.last().cloned()))
            .next();
        match cmd {
            Some(cmd) => self.send_to_dock(id, &cmd, window, cx),
            None => self.show_notice("El agente de esta sesión no sugirió comandos", cx),
        }
    }

    // ── Worktree por agente ─────────────────────────────────────────────────

    /// "Agentes: abrir X en un worktree nuevo": crea el worktree (en segundo plano), abre una sesión
    /// ahí y lanza el agente.
    pub(super) fn launch_in_worktree(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(agent) = self.catalog.get(id).cloned() else {
            return;
        };
        let Some(dir) = self.active_cwd(cx) else {
            self.show_notice("Todavía no se conoce la carpeta de la sesión", cx);
            return;
        };
        self.show_notice(format!("Creando un worktree para {}…", agent.name), cx);
        let agent_id = agent.id.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { worktree::create_for(&dir, &agent_id) })
                .await;
            let _ = this.update_in(cx, |w, window, cx| match result {
                Ok(plan) => w.open_worktree_session(plan, &agent.id, window, cx),
                Err(e) => w.show_notice(e, cx),
            });
        })
        .detach();
    }

    fn open_worktree_session(&mut self, plan: WorktreePlan, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(def) = self.catalog.get(agent).cloned() else {
            return;
        };
        self.exact.next_session_dir = Some(plan.path.clone());
        self.new_session(window, cx);
        self.exact.next_session_dir = None;
        let Some(sid) = self.sessions.active_id() else {
            return;
        };
        let name = plan
            .path
            .file_name()
            .map_or_else(|| def.name.clone(), |n| n.to_string_lossy().into_owned());
        self.sessions.rename(sid, name);
        self.exact.worktrees.insert(sid, plan.path.clone());
        self.persist();
        let line = launch_line(&def.command, &def.args, ShellFlavor::from_program(None));
        if let Some(view) = self.terminal(sid, false) {
            view.read(cx).send_line(line, NEW_TERMINAL_DELAY);
        }
        self.busy.insert((sid, false));
        self.show_notice(format!("Worktree en la rama {}", plan.branch), cx);
    }

    /// Carpeta del worktree de la sesión (la acoplada arranca ahí).
    pub(super) fn session_worktree(&self, id: u64) -> Option<PathBuf> {
        self.exact.worktrees.get(&id).cloned()
    }

    // ── Avisos ──────────────────────────────────────────────────────────────

    fn show_notice(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.exact.notice = Some((text.into(), Instant::now()));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(NOTICE_SECS)).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
        cx.notify();
    }

    // ── Instalador de ganchos ───────────────────────────────────────────────

    /// Paleta "Claude Code: instalar ganchos de Kuntur": muestra el plan; no escribe nada.
    pub(super) fn open_installer(&mut self, cx: &mut Context<Self>) {
        let Some(settings) = claude_settings_path() else {
            self.show_notice("No se encontró la carpeta personal", cx);
            return;
        };
        let exe = std::env::current_exe()
            .ok()
            .map(|e| e.with_file_name("kuntur-hook.exe"))
            .filter(|p| p.is_file());
        let existing = std::fs::read_to_string(&settings).ok();
        let plan = match exe {
            Some(exe) => install::plan(existing.as_deref(), &exe),
            None => Err("No se encontró kuntur-hook.exe junto a kuntur.exe".into()),
        };
        self.exact.installer = Some(Installer {
            settings,
            plan,
            done: None,
        });
        cx.notify();
    }

    fn close_installer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.exact.installer = None;
        self.focus_active(window, cx);
        cx.notify();
    }

    /// "Instalar": solo por clic del usuario. Copia de seguridad y luego escribe.
    fn confirm_install(&mut self, cx: &mut Context<Self>) {
        let Some(inst) = self.exact.installer.as_mut() else {
            return;
        };
        let Ok(plan) = &inst.plan else {
            return;
        };
        let stamp = jiff::Timestamp::now().strftime("%Y%m%d-%H%M%S").to_string();
        inst.done = Some(install::apply(&inst.settings, plan, &stamp).map_err(|e| e.to_string()));
        cx.notify();
    }

    // ── Dibujo ──────────────────────────────────────────────────────────────

    /// Filas extra bajo un agente: subagentes, permiso pendiente y comando sugerido.
    pub(super) fn render_row_extras(&self, r: &AgentRow, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(e) = &r.exact else {
            return Vec::new();
        };
        let c = &self.colors;
        let mut out = Vec::new();
        let indent = px(44.);
        for (i, s) in e.subagents.iter().take(MAX_SUB_ROWS).enumerate() {
            let tip: SharedString = format!(
                "{} · {}",
                s.kind,
                s.action
                    .clone()
                    .unwrap_or_else(|| if s.working { "trabajando" } else { "terminó" }.into())
            )
            .into();
            out.push(
                h_flex()
                    .id(SharedString::from(format!("sub-{}-{i}", r.key())))
                    .h(px(22.))
                    .pl(indent)
                    .pr(px(8.))
                    .gap(px(6.))
                    .items_center()
                    .text_size(px(11.))
                    .text_color(c.muted)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .child(div().size(px(5.)).flex_shrink_0().rounded_full().bg(if s.working {
                        c.accent
                    } else {
                        c.muted.opacity(0.5)
                    }))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(if s.working { c.text } else { c.muted })
                            .child(s.kind.clone()),
                    )
                    .when_some(s.action.clone(), |this, a| {
                        this.child(div().min_w_0().overflow_hidden().text_ellipsis().child(a))
                    })
                    .into_any_element(),
            );
        }
        if e.subagents.len() > MAX_SUB_ROWS {
            out.push(
                div()
                    .h(px(18.))
                    .pl(indent)
                    .text_size(px(11.))
                    .text_color(c.muted)
                    .child(format!("y {} más", e.subagents.len() - MAX_SUB_ROWS))
                    .into_any_element(),
            );
        }
        if e.state == ExactState::Permission {
            out.push(self.render_permission_bar(r, e.permission.clone(), cx));
        }
        if e.state != ExactState::Working
            && let Some(cmd) = e.suggestions.last()
        {
            out.push(self.render_suggestion_bar(r, cmd.clone(), cx));
        }
        out
    }

    fn small_button(&self, id: SharedString, label: &'static str, primary: bool) -> Stateful<Div> {
        let c = &self.colors;
        let hover = c.hover;
        div()
            .id(id)
            .flex_shrink_0()
            .h(px(20.))
            .px(px(7.))
            .rounded(px(5.))
            .flex()
            .items_center()
            .text_size(px(11.))
            .cursor_pointer()
            .map(|this| {
                if primary {
                    let accent = c.accent;
                    this.bg(accent.opacity(0.85))
                        .text_color(white())
                        .font_weight(FontWeight::MEDIUM)
                        .hover(move |s| s.bg(accent))
                } else {
                    this.bg(c.hover.opacity(0.6))
                        .text_color(c.text)
                        .hover(move |s| s.bg(hover))
                }
            })
            .child(label)
    }

    fn render_permission_bar(&self, r: &AgentRow, what: Option<String>, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let (sid, docked, key) = (r.session_id, r.docked, r.key());
        let agent = r.agent.clone();
        let agent2 = r.agent.clone();
        let tip: SharedString = what.clone().unwrap_or_else(|| "Pide permiso".into()).into();
        v_flex()
            .id(SharedString::from(format!("perm-{key}")))
            .pl(px(44.))
            .pr(px(8.))
            .pb(px(6.))
            .gap(px(4.))
            .child(
                div()
                    .id(SharedString::from(format!("perm-what-{key}")))
                    .text_size(px(11.))
                    .font_family(self.mono_font.clone())
                    .text_color(c.warn)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .child(what.unwrap_or_else(|| "pide permiso".into())),
            )
            .when(can_answer(&r.agent), |this| {
                this.child(
                    h_flex()
                        .gap(px(5.))
                        .child(
                            self.small_button(format!("allow-{key}").into(), "Permitir", true)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.answer_permission(sid, docked, &agent, Answer::Allow, cx);
                                })),
                        )
                        .child(
                            self.small_button(format!("deny-{key}").into(), "Rechazar", false)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.answer_permission(sid, docked, &agent2, Answer::Deny, cx);
                                })),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_suggestion_bar(&self, r: &AgentRow, cmd: String, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let (sid, key) = (r.session_id, r.key());
        let risky = is_risky(&cmd);
        let first = cmd.lines().next().unwrap_or("").to_string();
        let tip: SharedString = if risky {
            format!("Revisa antes de ejecutar: {cmd}").into()
        } else {
            format!("Sugerido: {cmd}").into()
        };
        let (copy, send) = (cmd.clone(), cmd);
        v_flex()
            .id(SharedString::from(format!("suggest-{key}")))
            .pl(px(44.))
            .pr(px(8.))
            .pb(px(6.))
            .gap(px(4.))
            .child(
                div()
                    .id(SharedString::from(format!("suggest-cmd-{key}")))
                    .text_size(px(11.))
                    .font_family(self.mono_font.clone())
                    .text_color(if risky { c.warn } else { c.text })
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .child(format!("$ {first}")),
            )
            .child(
                h_flex()
                    .gap(px(5.))
                    .child(
                        self.small_button(format!("send-{key}").into(), "Enviar a terminal", true)
                            .tooltip(|window, cx| {
                                Tooltip::new("Escribirlo en la terminal acoplada, sin ejecutarlo").build(window, cx)
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.send_to_dock(sid, &send, window, cx);
                            })),
                    )
                    .child(
                        self.small_button(format!("copy-{key}").into(), "Copiar", false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                                this.show_notice("Comando copiado", cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    /// Lo que flota sobre la ventana: el instalador de ganchos y el aviso de abajo a la derecha.
    pub(super) fn render_exact_overlay(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        if let Some(inst) = &self.exact.installer {
            out.push(self.render_installer(inst, cx));
        }
        if let Some((text, at)) = &self.exact.notice
            && at.elapsed() < Duration::from_secs(NOTICE_SECS)
        {
            let c = &self.colors;
            out.push(
                deferred(
                    div()
                        .absolute()
                        .right(px(14.))
                        .bottom(px(STATUS_LINE + 10.))
                        .max_w(px(420.))
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(8.))
                        .bg(c.popover)
                        .border_1()
                        .border_color(c.border)
                        .shadow_lg()
                        .text_size(px(12.))
                        .text_color(c.text)
                        .child(text.clone()),
                )
                .with_priority(2)
                .into_any_element(),
            );
        }
        out
    }

    fn render_installer(&self, inst: &Installer, cx: &mut Context<Self>) -> AnyElement {
        let c = self.colors.clone();
        let path = kuntur_files::display_path(&inst.settings, self.home.as_deref());
        let mono = self.mono_font.clone();
        let note = |text: String, color: Hsla| div().text_size(px(12.)).text_color(color).child(text);
        let mut body = v_flex().gap(px(10.)).px(px(18.)).py(px(14.));
        let mut can_install = false;
        match (&inst.plan, &inst.done) {
            (_, Some(Ok(backup))) => {
                body = body
                    .child(note("Ganchos instalados.".into(), c.text))
                    .when_some(backup.as_ref(), |this, b| {
                        this.child(note(
                            format!(
                                "Copia de seguridad: {}",
                                kuntur_files::display_path(b, self.home.as_deref())
                            ),
                            c.muted,
                        ))
                    })
                    .child(note(
                        "Las sesiones de Claude Code que abras desde ahora avisarán a Kuntur.".into(),
                        c.muted,
                    ));
            }
            (_, Some(Err(e))) => body = body.child(note(format!("No se pudo escribir: {e}"), c.error)),
            (Err(e), None) => body = body.child(note(e.clone(), c.error)),
            (Ok(plan), None) if !plan.changes() => {
                body = body.child(note("Ya están instalados: no hay nada que agregar.".into(), c.text));
            }
            (Ok(plan), None) => {
                can_install = true;
                let lines: Vec<AnyElement> = plan
                    .preview
                    .lines()
                    .map(|l| div().whitespace_nowrap().child(l.to_string()).into_any_element())
                    .collect();
                body = body
                    .child(note(format!("Se agregará a {path}:"), c.text))
                    .child(
                        v_flex()
                            .id("installer-preview")
                            .max_h(px(300.))
                            .overflow_scroll()
                            .p(px(10.))
                            .rounded(px(6.))
                            .bg(c.terminal)
                            .border_1()
                            .border_color(c.border)
                            .font_family(mono.clone())
                            .text_size(px(11.5))
                            .text_color(c.text)
                            .children(lines),
                    )
                    .child(note(
                        format!(
                            "Se agrega un gancho en {} eventos; no se borra ni cambia nada de lo que ya tienes.",
                            plan.added_events.len()
                        ),
                        c.muted,
                    ))
                    .child(note(
                        match &plan.statusline {
                            StatusLineChange::Added => {
                                "La línea de estado de Claude Code mostrará modelo · contexto · límites y pasará los límites al anillo de Claude.".to_string()
                            }
                            StatusLineChange::KeptExisting(cmd) => format!(
                                "Ya tienes una línea de estado ({cmd}): no se toca; el anillo de Claude queda sin datos."
                            ),
                            StatusLineChange::AlreadyKuntur => {
                                "La línea de estado de Kuntur ya está instalada.".to_string()
                            }
                        },
                        c.muted,
                    ))
                    .when(inst.settings.exists(), |this| {
                        this.child(note(
                            "Antes de escribir se guarda una copia: settings.json.kuntur-<fecha>.bak".into(),
                            c.muted,
                        ))
                    });
            }
        }
        let done = inst.done.is_some();
        let footer = h_flex()
            .px(px(18.))
            .py(px(12.))
            .gap(px(8.))
            .justify_end()
            .border_t_1()
            .border_color(c.border)
            .child(
                self.small_button(
                    "installer-cancel".into(),
                    if done { "Cerrar" } else { "Cancelar" },
                    false,
                )
                .h(px(28.))
                .px(px(12.))
                .text_size(px(12.5))
                .on_click(cx.listener(|this, _, window, cx| this.close_installer(window, cx))),
            )
            .when(can_install && !done, |this| {
                this.child(
                    self.small_button("installer-ok".into(), "Instalar", true)
                        .h(px(28.))
                        .px(px(12.))
                        .text_size(px(12.5))
                        .on_click(cx.listener(|this, _, _, cx| this.confirm_install(cx))),
                )
            });
        let panel = v_flex()
            .id("installer")
            .w(px(620.))
            .max_w(relative(0.94))
            .bg(c.popover)
            .border_1()
            .border_color(c.border)
            .rounded(px(10.))
            .shadow_lg()
            .overflow_hidden()
            .occlude()
            .child(
                v_flex()
                    .px(px(18.))
                    .pt(px(16.))
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c.text)
                            .child("Instalar ganchos de Claude Code"),
                    )
                    .child(div().text_size(px(12.)).text_color(c.muted).child(
                        "Con los ganchos, Kuntur sabe al instante si cada agente trabaja, pide permiso o terminó, y ve sus subagentes.",
                    )),
            )
            .child(body)
            .child(footer);
        deferred(
            div()
                .id("installer-backdrop")
                .absolute()
                .top(TITLE_BAR_HEIGHT)
                .left_0()
                .right_0()
                .bottom_0()
                .flex()
                .items_start()
                .justify_center()
                .pt(px(40.))
                .bg(hsla(0., 0., 0., 0.25))
                .occlude()
                .child(panel),
        )
        .with_priority(2)
        .into_any_element()
    }

    // ── Pruebas de humo ─────────────────────────────────────────────────────

    /// Manda por el pipe real los eventos sintéticos del archivo (una línea JSON por evento, con
    /// `"pane": "#1"` = principal de la sesión 1, `"#1d"` = su acoplada, y `"delay_ms"` opcional).
    fn smoke_agent_events(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;
            let Ok((pipe, tokens)) = this.update(cx, |w, _| {
                let tokens: Vec<(Option<String>, Option<String>)> = w
                    .sessions
                    .iter()
                    .map(|e| (w.token_of(e.id, false).cloned(), w.token_of(e.id, true).cloned()))
                    .collect();
                (w.exact.pipe.clone(), tokens)
            }) else {
                return;
            };
            std::thread::spawn(move || {
                for line in text.lines().filter(|l| !l.trim().is_empty()) {
                    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(line) else {
                        continue;
                    };
                    if let Some(ms) = v.get("delay_ms").and_then(|d| d.as_u64()) {
                        std::thread::sleep(Duration::from_millis(ms));
                    }
                    let pane = v.get("pane").and_then(|p| p.as_str()).unwrap_or("#1").to_string();
                    let docked = pane.ends_with('d');
                    let n: usize = pane.trim_start_matches('#').trim_end_matches('d').parse().unwrap_or(1);
                    let token = tokens
                        .get(n.saturating_sub(1))
                        .and_then(|(m, d)| if docked { d.clone() } else { m.clone() });
                    let Some(token) = token else { continue };
                    v["pane"] = serde_json::Value::String(token);
                    v["ts"] = serde_json::Value::from(kuntur_hook::event::now_micros());
                    if let Ok(ev) = serde_json::from_value::<HookEvent>(v) {
                        let _ = kuntur_hook::send(&pipe, &ev.encode());
                    }
                }
            });
        })
        .detach();
    }
}
