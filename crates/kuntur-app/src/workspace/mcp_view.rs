//! Gestor de MCP por sesión (§4 y §11) y comandos con atajo (F10).
//!
//! - Botón de ícono arriba a la derecha (y Ctrl+Shift+M o la paleta): panel flotante con el
//!   catálogo (`[[mcp]]` de `config.toml`), un interruptor por servidor para **esta** sesión, los
//!   perfiles (`[[mcp_perfiles]]`: servidores + modelo/esfuerzo/permisos), "Probar" con su estado y
//!   los secretos que faltan (se guardan en el Administrador de credenciales de Windows).
//! - Al lanzar un agente (clic, paleta, worktree) se genera la configuración MCP de su CLI en
//!   `%LOCALAPPDATA%\Kuntur\mcp\<pid>-<sesión>\` y se borra al cerrar la sesión.
//! - Lo que los MCP dejen en `…\salida\` se abre en la vista previa (sondeo cada 2 s).
//! - Comandos con atajo (`[[comandos]]`): texto a la terminal de la sesión o a la acoplada.
//!
//! La lógica vive en `kuntur-mcp`, con pruebas.

use std::collections::HashMap;

use kuntur_agents::AgentDef;
use kuntur_config::{CommandTarget, McpServerConfig, McpTransport, PermissionMode, ShortcutCommand};
use kuntur_mcp::catalog::{TOKEN_KEY, active_servers, missing_secrets};
use kuntur_mcp::generate::Cli;
use kuntur_mcp::launch::LaunchInput;
use kuntur_mcp::{
    AgentOptions, MemorySecrets, OutputWatcher, ProbeStatus, SecretStore, SessionDirs, apply_profile, plan_launch,
    probe, resolve, secret_account, shortcuts, system_store, toggle,
};

use super::*;
use crate::dock::Slot;

const PANEL_W: f32 = 500.0;
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const OUTPUT_POLL: Duration = Duration::from_secs(2);

/// Secreto que se está escribiendo en el panel (servidor, variable, texto).
struct SecretInput {
    server: String,
    key: String,
    text: String,
}

pub(super) struct McpState {
    pub(super) open: bool,
    /// Servidores activos de cada sesión.
    lists: HashMap<u64, Vec<String>>,
    /// Perfil (opciones del agente) de cada sesión.
    profiles: HashMap<u64, String>,
    /// Estado de "Probar", por servidor.
    status: HashMap<String, ProbeStatus>,
    dirs: SessionDirs,
    secrets: Option<Arc<dyn SecretStore>>,
    watchers: HashMap<u64, OutputWatcher>,
    /// Abrir solas en la vista previa las capturas nuevas de los MCP.
    pub(super) auto_open: bool,
    input: Option<SecretInput>,
    smoke: bool,
    _poll: Option<Task<()>>,
}

impl Default for McpState {
    fn default() -> Self {
        Self {
            open: false,
            lists: HashMap::new(),
            profiles: HashMap::new(),
            status: HashMap::new(),
            dirs: SessionDirs::new(SessionDirs::default_root(), std::process::id()),
            secrets: None,
            watchers: HashMap::new(),
            auto_open: true,
            input: None,
            smoke: false,
            _poll: None,
        }
    }
}

fn smoke_mode() -> Option<String> {
    std::env::var("KUNTUR_SMOKE_MCP").ok()
}

fn permissions_label(p: PermissionMode) -> &'static str {
    match p {
        PermissionMode::Preguntar => "preguntar",
        PermissionMode::Ediciones => "aceptar ediciones",
        PermissionMode::Plan => "plan",
        PermissionMode::Total => "sin preguntas",
    }
}

fn server_detail(s: &McpServerConfig) -> String {
    match s.transport {
        McpTransport::Stdio => {
            let mut parts = vec![s.command.clone()];
            parts.extend(s.args.iter().cloned());
            let text = parts.join(" ");
            if text.chars().count() > 60 {
                format!("{}…", text.chars().take(59).collect::<String>())
            } else {
                text
            }
        }
        McpTransport::Http => s.url.clone(),
    }
}

impl Workspace {
    // ── Arranque, sesiones y persistencia ───────────────────────────────────

    /// Al abrir Kuntur: borra carpetas temporales de procesos muertos y empieza a mirar las
    /// carpetas de salida.
    pub(super) fn start_mcp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(mode) = smoke_mode() {
            self.mcp.smoke = true;
            // Nada del usuario: carpeta propia de la prueba y secretos en memoria.
            self.mcp.dirs = SessionDirs::new(std::env::temp_dir().join("kuntur-smoke-mcp"), std::process::id());
            let store: Arc<dyn SecretStore> = Arc::new(MemorySecrets::new());
            self.mcp.secrets = Some(store);
            self.smoke_mcp(&mode, window, cx);
        }
        let dirs = self.mcp.dirs.clone();
        cx.background_executor()
            .spawn(async move {
                let Ok(sys) = sysinfo::System::new_with_specifics(
                    sysinfo::RefreshKind::nothing().with_processes(sysinfo::ProcessRefreshKind::nothing()),
                ) else {
                    return;
                };
                dirs.clean_stale(|pid| sys.process(sysinfo::Pid::from_u32(pid)).is_some());
            })
            .detach();
        self.mcp._poll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(OUTPUT_POLL).await;
                if this
                    .update_in(cx, |w, window, cx| w.poll_mcp_outputs(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    fn mcp_secrets(&mut self) -> Arc<dyn SecretStore> {
        self.mcp.secrets.get_or_insert_with(system_store).clone()
    }

    /// Sesión nueva: el perfil inicial de la configuración (si hay).
    pub(super) fn mcp_new_session(&mut self, id: u64) {
        if let Some(p) = self.settings.mcp_perfil_inicial.clone() {
            self.mcp_apply_profile_to(id, &p);
        }
    }

    /// Sesión restaurada: su lista y su perfil guardados.
    pub(super) fn mcp_restore_session(&mut self, id: u64, saved: &SessionConfig) {
        self.mcp.lists.insert(id, saved.mcp.clone());
        match &saved.profile {
            Some(p) => self.mcp.profiles.insert(id, p.clone()),
            None => self.mcp.profiles.remove(&id),
        };
    }

    /// La última sesión agregada (al restaurar): su lista y su perfil guardados.
    pub(super) fn mcp_restore_last(&mut self, saved: &SessionConfig) {
        let last = self.sessions.iter().last().map(|e| e.id);
        if let Some(id) = last {
            self.mcp_restore_session(id, saved);
        }
    }

    /// Para `persist`: lista y perfil de la sesión.
    pub(super) fn mcp_saved(&self, id: u64) -> (Vec<String>, Option<String>) {
        (
            self.mcp.lists.get(&id).cloned().unwrap_or_default(),
            self.mcp.profiles.get(&id).cloned(),
        )
    }

    /// Al cerrar la sesión se borran sus archivos temporales (configuraciones y salida).
    pub(super) fn mcp_close_session(&mut self, id: u64) {
        self.mcp.lists.remove(&id);
        self.mcp.profiles.remove(&id);
        self.mcp.watchers.remove(&id);
        self.mcp.dirs.remove(id);
    }

    fn mcp_apply_profile_to(&mut self, id: u64, name: &str) {
        if let Some(list) = apply_profile(&self.settings.mcp_perfiles, name, &self.settings.mcp) {
            self.mcp.lists.insert(id, list);
            self.mcp.profiles.insert(id, name.to_string());
        }
    }

    /// Carpeta temporal de la sesión (se crea al usarla) y su vigilante de capturas.
    fn mcp_session_dir(&mut self, id: u64) -> Option<PathBuf> {
        let dir = self.mcp.dirs.create(id).ok()?;
        self.mcp.watchers.entry(id).or_insert_with(|| {
            let mut w = OutputWatcher::new();
            w.prime(&dir.join(kuntur_mcp::OUTPUT_DIR));
            w
        });
        Some(dir)
    }

    // ── Lanzar agentes con el MCP de la sesión ──────────────────────────────

    /// Línea para abrir el agente en la sesión: genera la configuración MCP de su CLI (Claude Code,
    /// Codex, OpenCode) y agrega las opciones del perfil. Sin MCP ni perfil, la línea de siempre.
    pub(super) fn agent_launch_line(
        &mut self,
        session: u64,
        def: &AgentDef,
        shell: ShellFlavor,
        cx: &mut Context<Self>,
    ) -> String {
        let plain = || launch_line(&def.command, &def.args, shell);
        // Remota: el agente corre en el servidor y no vería los archivos de esta PC.
        if self.sessions.get(session).is_some_and(|e| e.value.remote.is_some()) {
            return plain();
        }
        let list = self.mcp.lists.get(&session).cloned().unwrap_or_default();
        let options = self
            .mcp
            .profiles
            .get(&session)
            .and_then(|p| self.settings.mcp_perfiles.iter().find(|x| &x.name == p))
            .map(AgentOptions::from)
            .unwrap_or_default();
        let catalog = self.settings.mcp.clone();
        let active = active_servers(&catalog, &list);
        if active.is_empty() && options.is_empty() && !self.settings.mcp_exclusivo {
            return plain();
        }
        let Some(dir) = self.mcp_session_dir(session) else {
            self.show_notice(
                "No se pudo crear la carpeta temporal de MCP: el agente se abre sin MCP",
                cx,
            );
            return plain();
        };
        let store = self.mcp_secrets();
        let output = dir.join(kuntur_mcp::OUTPUT_DIR);
        let mut warnings: Vec<String> = Vec::new();
        let resolved: Vec<_> = active
            .iter()
            .map(|s| {
                let missing = missing_secrets(s, store.as_ref());
                if !missing.is_empty() {
                    warnings.push(format!("{}: faltan secretos ({})", s.name, missing.join(", ")));
                }
                resolve(s, store.as_ref(), Some(&output))
            })
            .collect();
        let input = LaunchInput {
            cli: Cli::from_agent_id(&def.id),
            command: &def.command,
            args: &def.args,
            shell,
            servers: &resolved,
            options: &options,
            session_dir: &dir,
            exclusive: self.settings.mcp_exclusivo,
            opencode_v2: self.settings.mcp_opencode_v2,
            is_batch: &kuntur_mcp::probe::is_batch,
        };
        let mut plan = plan_launch(&input);
        for f in &plan.files {
            if let Err(e) = f.write() {
                warnings.push(format!("no se pudo escribir {}: {e}", f.path.display()));
            }
        }
        warnings.append(&mut plan.warnings);
        if !warnings.is_empty() {
            self.show_notice(warnings.join(" · "), cx);
        }
        plan.line
    }

    // ── Panel ───────────────────────────────────────────────────────────────

    pub(super) fn toggle_mcp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mcp.open {
            self.close_mcp(window, cx);
        } else {
            self.open_mcp(window, cx);
        }
    }

    /// Abre el gestor (vuelve a leer el catálogo de `config.toml`, por si se editó a mano).
    pub(super) fn open_mcp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reload_mcp_config();
        self.palette = None;
        self.connections.open = false;
        self.mcp.open = true;
        self.mcp.input = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn close_mcp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mcp.open = false;
        self.mcp.input = None;
        self.focus_active(window, cx);
        cx.notify();
    }

    fn reload_mcp_config(&mut self) {
        if self.mcp.smoke {
            return;
        }
        if let Some(path) = &self.config_path {
            let saved = Settings::load(path);
            self.settings.mcp = saved.mcp;
            self.settings.mcp_perfiles = saved.mcp_perfiles;
            self.settings.comandos = saved.comandos;
            self.settings.mcp_exclusivo = saved.mcp_exclusivo;
            self.settings.mcp_opencode_v2 = saved.mcp_opencode_v2;
            self.settings.mcp_perfil_inicial = saved.mcp_perfil_inicial;
        }
    }

    fn toggle_server(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(id) = self.sessions.active_id() else { return };
        let list = self.mcp.lists.entry(id).or_default();
        if !toggle(list, name) {
            self.mcp.status.insert(name.to_string(), ProbeStatus::Stopped);
        }
        self.persist();
        cx.notify();
    }

    fn choose_profile(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(id) = self.sessions.active_id() else { return };
        if self.mcp.profiles.get(&id).is_some_and(|p| p == name) {
            // Volver a pulsar el perfil activo lo quita (los servidores quedan).
            self.mcp.profiles.remove(&id);
        } else {
            self.mcp_apply_profile_to(id, name);
        }
        self.persist();
        cx.notify();
    }

    /// "Probar": lanza el servidor (o se conecta) y hace `initialize`, en segundo plano.
    fn probe_server(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(server) = self.settings.mcp.iter().find(|s| s.name == name).cloned() else {
            return;
        };
        let output = self
            .sessions
            .active_id()
            .and_then(|id| self.mcp_session_dir(id))
            .map(|d| d.join(kuntur_mcp::OUTPUT_DIR));
        let store = self.mcp_secrets();
        let resolved = resolve(&server, store.as_ref(), output.as_deref());
        let key = name.to_string();
        self.mcp.status.insert(key.clone(), ProbeStatus::Testing);
        cx.spawn(async move |this, cx| {
            let status = cx
                .background_executor()
                .spawn(async move { probe(&resolved, PROBE_TIMEOUT) })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.mcp.status.insert(key, status);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn probe_all(&mut self, cx: &mut Context<Self>) {
        let names: Vec<String> = self.settings.mcp.iter().map(|s| s.name.clone()).collect();
        for n in names {
            self.probe_server(&n, cx);
        }
    }

    fn start_secret(&mut self, server: &str, key: &str, cx: &mut Context<Self>) {
        self.mcp.input = Some(SecretInput {
            server: server.into(),
            key: key.into(),
            text: String::new(),
        });
        cx.notify();
    }

    fn save_secret(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.mcp.input.take() else { return };
        if input.text.is_empty() {
            cx.notify();
            return;
        }
        let store = self.mcp_secrets();
        let account = secret_account(&input.server, &input.key);
        if let Err(e) = store.set(&account, &input.text) {
            self.show_notice(format!("No se pudo guardar el secreto: {e}"), cx);
        }
        cx.notify();
    }

    fn open_output_folder(&mut self) {
        let Some(id) = self.sessions.active_id() else { return };
        if let Some(dir) = self.mcp_session_dir(id) {
            let _ = std::process::Command::new("explorer")
                .arg(dir.join(kuntur_mcp::OUTPUT_DIR))
                .spawn();
        }
    }

    /// Teclado con el panel abierto: Esc cierra (o cancela el secreto); el secreto se escribe aquí.
    pub(super) fn on_mcp_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = ks.modifiers;
        let key = ks.key.to_ascii_lowercase();
        if m.control && m.shift && key == "m" {
            self.close_mcp(window, cx);
            return;
        }
        let Some(input) = self.mcp.input.as_mut() else {
            if key == "escape" {
                self.close_mcp(window, cx);
            }
            return;
        };
        match key.as_str() {
            "escape" => self.mcp.input = None,
            "enter" => self.save_secret(cx),
            "backspace" => {
                input.text.pop();
            }
            "v" if m.control && !m.alt => {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    input.text.push_str(text.trim());
                }
            }
            _ if !m.control || m.alt => {
                if let Some(text) = ks.key_char.as_deref() {
                    input.text.push_str(text);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    // ── Capturas de los MCP → vista previa ──────────────────────────────────

    fn poll_mcp_outputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<u64> = self.mcp.watchers.keys().copied().collect();
        let mut newest: Option<PathBuf> = None;
        for id in ids {
            let dir = self.mcp.dirs.output(id);
            if let Some(w) = self.mcp.watchers.get_mut(&id)
                && let Some(last) = w.scan(&dir).pop()
            {
                newest = Some(last);
            }
        }
        if let Some(path) = newest.filter(|_| self.mcp.auto_open) {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.open_preview(path, None, window, cx);
            self.show_notice(format!("Captura de un MCP: {name}"), cx);
        }
    }

    // ── Comandos con atajo ──────────────────────────────────────────────────

    /// Si la tecla es el atajo de un comando (de esta sesión o global), lo escribe y devuelve `true`.
    pub(super) fn run_shortcut_command(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.settings.comandos.is_empty() {
            return false;
        }
        let m = ev.keystroke.modifiers;
        let Some(e) = self.sessions.active() else { return false };
        let (id, name) = (e.id, e.name.clone());
        let Some(cmd) = shortcuts::find(
            &self.settings.comandos,
            &name,
            m.control,
            m.shift,
            m.alt,
            &ev.keystroke.key.to_ascii_lowercase(),
        )
        .cloned() else {
            return false;
        };
        self.send_command(id, &cmd, window, cx);
        true
    }

    fn send_command(&mut self, id: u64, cmd: &ShortcutCommand, window: &mut Window, cx: &mut Context<Self>) {
        let docked = cmd.target == CommandTarget::Acoplada;
        let mut delay = Duration::ZERO;
        if docked {
            let existed = self.sessions.get(id).is_some_and(|e| e.value.dock.is_some());
            if let Some(e) = self.sessions.get_mut(id) {
                if e.value.dock_state.open {
                    e.value.dock_state.set_focus(Slot::Dock);
                } else {
                    e.value.dock_state.toggle(Slot::Main);
                }
            }
            self.ensure_dock(id, window, cx);
            if !existed {
                delay = Duration::from_millis(1_500);
            }
        }
        if let Some(view) = self.terminal(id, docked) {
            let v = view.read(cx);
            if cmd.enter {
                v.send_line(cmd.text.clone(), delay);
            } else {
                v.paste_text(cmd.text.clone(), delay);
            }
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    // ── Prueba de humo ──────────────────────────────────────────────────────

    /// Servidores SINTÉTICOS (el servidor falso de `kuntur-mcp`, uno que no existe y uno http a un
    /// puerto sin servidor), un perfil, la primera sesión con dos activos y el panel abierto; con
    /// "probar" además se prueban todos.
    fn smoke_mcp(&mut self, mode: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.config_path = None;
        let fake = std::env::var("KUNTUR_SMOKE_MCP_FAKE").unwrap_or_else(|_| "kuntur-mcp-falso".into());
        self.settings.mcp = vec![
            McpServerConfig {
                name: "escena-godot".into(),
                command: fake.clone(),
                args: vec!["--captura".into()],
                description: Some("Servidor falso que deja una captura".into()),
                ..Default::default()
            },
            McpServerConfig {
                name: "base-datos".into(),
                command: fake.clone(),
                args: vec!["--modo".into(), "error".into()],
                secrets: vec!["DB_PASSWORD".into()],
                ..Default::default()
            },
            McpServerConfig {
                name: "navegador".into(),
                command: "kuntur-mcp-no-instalado".into(),
                ..Default::default()
            },
            McpServerConfig {
                name: "docs".into(),
                transport: McpTransport::Http,
                url: "http://127.0.0.1:9/mcp".into(),
                token: true,
                ..Default::default()
            },
        ];
        self.settings.mcp_perfiles = vec![
            kuntur_config::McpProfileConfig {
                name: "godot".into(),
                servers: vec!["escena-godot".into(), "base-datos".into()],
                model: Some("sonnet".into()),
                effort: Some("high".into()),
                permissions: Some(PermissionMode::Ediciones),
            },
            kuntur_config::McpProfileConfig {
                name: "web".into(),
                servers: vec!["navegador".into(), "docs".into()],
                ..Default::default()
            },
        ];
        self.settings.comandos = vec![ShortcutCommand {
            name: "Pruebas".into(),
            text: "cargo test --workspace".into(),
            shortcut: "Ctrl+Shift+1".into(),
            target: CommandTarget::Acoplada,
            ..Default::default()
        }];
        let first = self.sessions.iter().next().map(|e| e.id);
        if let Some(id) = first {
            self.mcp_apply_profile_to(id, "godot");
        }
        if mode == "linea" {
            // Pega SIN Enter la línea de Claude Code con el MCP de la sesión: nunca se ejecuta.
            if let Some(store) = &self.mcp.secrets {
                let _ = store.set("base-datos/DB_PASSWORD", "valor-sintetico");
            }
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(4)).await;
                let _ = this.update_in(cx, |w, _, cx| {
                    let (Some(id), Some(def)) = (first, w.catalog.get("claude").cloned()) else {
                        return;
                    };
                    let line = w.agent_launch_line(id, &def, ShellFlavor::PowerShell, cx);
                    if let Some(view) = w.terminal(id, false) {
                        view.read(cx).paste_text(line, Duration::ZERO);
                    }
                });
            })
            .detach();
            return;
        }
        self.open_mcp(window, cx);
        if mode == "probar" {
            self.probe_all(cx);
        }
    }

    // ── Dibujo ──────────────────────────────────────────────────────────────

    pub(super) fn render_mcp(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = self.colors.clone();
        let hover = c.hover;
        let active = self.sessions.active();
        let session_name = active.map(|e| e.name.clone()).unwrap_or_default();
        let remote = active.is_some_and(|e| e.value.remote.is_some());
        let header = h_flex()
            .h(px(44.))
            .px(px(14.))
            .gap(px(8.))
            .items_center()
            .border_b_1()
            .border_color(c.border)
            .child(Icon::new(IconName::Plug).size(px(15.)).text_color(c.muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_size(px(13.5))
                    .font_weight(FontWeight::MEDIUM)
                    .child(format!("MCP · {session_name}")),
            )
            .when(!self.settings.mcp.is_empty(), |this| {
                this.child(
                    h_flex()
                        .id("mcp-probe-all")
                        .h(px(26.))
                        .px(px(8.))
                        .gap(px(6.))
                        .items_center()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_size(px(12.))
                        .text_color(c.accent)
                        .hover(move |s| s.bg(hover))
                        .tooltip(|window, cx| Tooltip::new("Lanza cada servidor y hace «initialize»").build(window, cx))
                        .on_click(cx.listener(|this, _, _, cx| this.probe_all(cx)))
                        .child(Icon::new(IconName::Play).size(px(12.)))
                        .child("Probar todos"),
                )
            });

        let mut body = v_flex();
        if remote {
            body = body.child(
                div()
                    .px(px(14.))
                    .py(px(8.))
                    .text_size(px(11.5))
                    .text_color(c.warn)
                    .child("Pestaña remota: los agentes corren en el servidor y no reciben estos MCP."),
            );
        }
        if self.settings.mcp.is_empty() {
            body = body.child(
                v_flex()
                    .py(px(24.))
                    .px(px(20.))
                    .gap(px(6.))
                    .items_center()
                    .text_center()
                    .child(div().text_size(px(13.)).child("Sin servidores MCP"))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(c.muted)
                            .child("Agrégalos en config.toml con [[mcp]] (nombre, comando, argumentos, entorno, secretos) o tipo = \"http\" con url. Los secretos van al Administrador de credenciales de Windows."),
                    ),
            );
        } else {
            body = body.child(self.render_profiles(cx)).child(self.render_servers(cx));
        }
        body = body.child(self.render_mcp_footer(cx));

        let panel = v_flex()
            .id("mcp-panel")
            .track_focus(&self.focus)
            .w(px(PANEL_W))
            .max_w(relative(0.95))
            .max_h(px(620.))
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
            .child(body);

        deferred(
            div()
                .id("mcp-backdrop")
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
                    cx.listener(|this, _, window, cx| this.close_mcp(window, cx)),
                )
                .child(panel),
        )
        .with_priority(1)
        .into_any_element()
    }

    fn active_profile(&self) -> Option<&String> {
        self.sessions.active_id().and_then(|id| self.mcp.profiles.get(&id))
    }

    fn render_profiles(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let hover = c.hover;
        let current = self.active_profile().cloned();
        let chips: Vec<AnyElement> = self
            .settings
            .mcp_perfiles
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let selected = current.as_deref() == Some(p.name.as_str());
                let name = p.name.clone();
                h_flex()
                    .id(("mcp-profile", i))
                    .h(px(24.))
                    .px(px(10.))
                    .items_center()
                    .rounded(px(12.))
                    .border_1()
                    .cursor_pointer()
                    .text_size(px(12.))
                    .map(|this| {
                        if selected {
                            this.border_color(c.accent).text_color(c.accent).bg(c.surface_2)
                        } else {
                            this.border_color(c.border)
                                .text_color(c.muted)
                                .hover(move |s| s.bg(hover))
                        }
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.choose_profile(&name, cx)))
                    .child(p.name.clone())
                    .into_any_element()
            })
            .collect();
        let options = current
            .as_ref()
            .and_then(|n| self.settings.mcp_perfiles.iter().find(|p| &p.name == n))
            .map(|p| {
                let mut parts = Vec::new();
                if let Some(m) = &p.model {
                    parts.push(format!("modelo {m}"));
                }
                if let Some(e) = &p.effort {
                    parts.push(format!("esfuerzo {e}"));
                }
                if let Some(x) = p.permissions {
                    parts.push(format!("permisos: {}", permissions_label(x)));
                }
                if parts.is_empty() {
                    "sin opciones de agente".to_string()
                } else {
                    parts.join(" · ")
                }
            });
        v_flex()
            .px(px(14.))
            .py(px(10.))
            .gap(px(6.))
            .border_b_1()
            .border_color(c.border)
            .child(
                h_flex()
                    .gap(px(6.))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(c.muted)
                            .mr(px(4.))
                            .child("Perfiles"),
                    )
                    .when(chips.is_empty(), |this| {
                        this.child(
                            div()
                                .text_size(px(11.5))
                                .text_color(c.muted)
                                .child("ninguno ([[mcp_perfiles]] en config.toml)"),
                        )
                    })
                    .children(chips),
            )
            .when_some(options, |this, text| {
                this.child(div().text_size(px(11.5)).text_color(c.muted).child(text))
            })
            .into_any_element()
    }

    fn render_servers(&self, cx: &mut Context<Self>) -> AnyElement {
        let list = self
            .sessions
            .active_id()
            .and_then(|id| self.mcp.lists.get(&id))
            .cloned()
            .unwrap_or_default();
        let store = self.mcp.secrets.clone();
        let rows: Vec<AnyElement> = self
            .settings
            .mcp
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let missing = store
                    .as_ref()
                    .map(|st| missing_secrets(s, st.as_ref()))
                    .unwrap_or_else(|| {
                        // Sin abrir el Administrador todavía: se consulta al primer uso.
                        let mut keys = s.secrets.clone();
                        if s.transport == McpTransport::Http && s.token {
                            keys = vec![TOKEN_KEY.into()];
                        }
                        keys
                    });
                self.render_server(i, s, list.contains(&s.name), missing, cx)
            })
            .collect();
        v_flex()
            .id("mcp-list")
            .max_h(px(380.))
            .overflow_y_scroll()
            .p(px(6.))
            .gap(px(2.))
            .children(rows)
            .into_any_element()
    }

    fn render_server(
        &self,
        index: usize,
        s: &McpServerConfig,
        on: bool,
        missing: Vec<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &self.colors;
        let hover = c.hover;
        let status = self.mcp.status.get(&s.name).cloned().unwrap_or(ProbeStatus::Stopped);
        let dot = match &status {
            ProbeStatus::Connected(_) => c.ok,
            ProbeStatus::Error(_) => c.error,
            ProbeStatus::Testing => c.accent,
            ProbeStatus::Stopped => c.muted,
        };
        let name = s.name.clone();
        let toggle_name = name.clone();
        let probe_name = name.clone();
        let kind = match s.transport {
            McpTransport::Stdio => "stdio",
            McpTransport::Http => "http",
        };
        let editing = self.mcp.input.as_ref().filter(|i| i.server == s.name);

        let switch = div()
            .id(("mcp-switch", index))
            .w(px(30.))
            .h(px(17.))
            .flex_shrink_0()
            .rounded_full()
            .p(px(2.))
            .flex()
            .cursor_pointer()
            .map(|this| {
                if on {
                    this.bg(c.accent).justify_end()
                } else {
                    this.bg(c.surface_2).justify_start()
                }
            })
            .tooltip(move |window, cx| {
                Tooltip::new(if on {
                    "Activo en esta sesión (clic: desactivar)"
                } else {
                    "Inactivo en esta sesión (clic: activar)"
                })
                .build(window, cx)
            })
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_server(&toggle_name, cx)))
            .child(div().size(px(13.)).rounded_full().bg(rgb(0xffffff)));

        let mut info = v_flex()
            .flex_1()
            .min_w_0()
            .gap(px(2.))
            .child(
                h_flex()
                    .gap(px(6.))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(13.))
                            .when(on, |this| this.font_weight(FontWeight::MEDIUM))
                            .when(!on, |this| this.text_color(c.muted))
                            .child(s.name.clone()),
                    )
                    .child(div().text_size(px(10.5)).text_color(c.muted).child(kind)),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(c.muted)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(server_detail(s)),
            )
            .child(
                h_flex()
                    .gap(px(6.))
                    .items_center()
                    .min_w_0()
                    .child(div().size(px(7.)).flex_shrink_0().rounded_full().bg(dot))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(11.5))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_color(if matches!(status, ProbeStatus::Error(_)) {
                                c.error
                            } else {
                                c.muted
                            })
                            .child(status.label()),
                    ),
            );
        // Secretos que faltan: clic para escribirlo (se guarda en el Administrador de credenciales).
        if !missing.is_empty() {
            let chips: Vec<AnyElement> = missing
                .iter()
                .enumerate()
                .map(|(k, key)| {
                    let (srv, key2) = (name.clone(), key.clone());
                    let label = if key == TOKEN_KEY {
                        "Falta el token".to_string()
                    } else {
                        format!("Falta {key}")
                    };
                    h_flex()
                        .id(SharedString::from(format!("mcp-secret-{index}-{k}")))
                        .h(px(20.))
                        .px(px(7.))
                        .gap(px(4.))
                        .items_center()
                        .rounded(px(5.))
                        .cursor_pointer()
                        .text_size(px(11.))
                        .text_color(c.warn)
                        .hover(move |s| s.bg(hover))
                        .tooltip(|window, cx| {
                            Tooltip::new("Escribir el secreto (Administrador de credenciales de Windows)")
                                .build(window, cx)
                        })
                        .on_click(cx.listener(move |this, _, _, cx| this.start_secret(&srv, &key2, cx)))
                        .child(Icon::new(IconName::Asterisk).size(px(11.)))
                        .child(label)
                        .into_any_element()
                })
                .collect();
            info = info.child(h_flex().gap(px(4.)).flex_wrap().children(chips));
        }
        if let Some(input) = editing {
            let shown = "•".repeat(input.text.chars().count());
            let label = if input.key == TOKEN_KEY {
                "token".to_string()
            } else {
                input.key.clone()
            };
            info = info.child(
                h_flex()
                    .mt(px(2.))
                    .h(px(26.))
                    .px(px(8.))
                    .gap(px(6.))
                    .items_center()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(c.accent)
                    .text_size(px(12.))
                    .child(div().text_color(c.muted).child(format!("{label}:")))
                    .child(
                        div()
                            .flex_1()
                            .child(if shown.is_empty() { "▏".to_string() } else { shown }),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(c.muted)
                            .child("Enter guarda · Esc cancela"),
                    ),
            );
        }

        h_flex()
            .id(("mcp-row", index))
            .px(px(8.))
            .py(px(7.))
            .gap(px(10.))
            .items_start()
            .rounded(px(7.))
            .hover(move |s| s.bg(hover))
            .child(div().pt(px(2.)).child(switch))
            .child(info)
            .child(
                h_flex()
                    .id(("mcp-probe", index))
                    .flex_shrink_0()
                    .h(px(24.))
                    .px(px(8.))
                    .gap(px(5.))
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(c.accent)
                    .hover(move |s| s.bg(hover))
                    .tooltip(|window, cx| Tooltip::new("Lanzar y hacer «initialize» (15 s máx.)").build(window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| this.probe_server(&probe_name, cx)))
                    .child(Icon::new(IconName::Play).size(px(11.)))
                    .child("Probar"),
            )
            .into_any_element()
    }

    fn render_mcp_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let hover = c.hover;
        let session = self.sessions.active().map(|e| e.name.clone()).unwrap_or_default();
        let commands: Vec<String> = shortcuts::commands_for(&self.settings.comandos, &session)
            .iter()
            .map(|cmd| {
                let key = shortcuts::Shortcut::parse(&cmd.shortcut)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|e| format!("atajo inválido: {e}"));
                let target = match cmd.target {
                    CommandTarget::Sesion => "sesión",
                    CommandTarget::Acoplada => "acoplada",
                };
                format!("{key} · {} → {target}", cmd.name)
            })
            .collect();
        let auto = self.mcp.auto_open;
        v_flex()
            .px(px(14.))
            .py(px(8.))
            .gap(px(6.))
            .border_t_1()
            .border_color(c.border)
            .text_size(px(11.))
            .text_color(c.muted)
            .when(!commands.is_empty(), |this| {
                this.child(
                    v_flex()
                        .gap(px(2.))
                        .child(div().text_color(c.text).child("Comandos con atajo"))
                        .children(commands.into_iter().map(|t| div().child(t))),
                )
            })
            .child(
                h_flex()
                    .gap(px(10.))
                    .items_center()
                    .child(
                        h_flex()
                            .id("mcp-output")
                            .gap(px(5.))
                            .items_center()
                            .cursor_pointer()
                            .rounded(px(5.))
                            .px(px(4.))
                            .hover(move |s| s.bg(hover))
                            .tooltip(|window, cx| {
                                Tooltip::new(
                                    "Carpeta de la sesión donde los MCP dejan capturas (variable KUNTUR_MCP_SALIDA)",
                                )
                                .build(window, cx)
                            })
                            .on_click(cx.listener(|this, _, _, _| this.open_output_folder()))
                            .child(Icon::new(IconName::FolderOpen).size(px(12.)))
                            .child("Carpeta de salida"),
                    )
                    .child(
                        h_flex()
                            .id("mcp-auto-open")
                            .gap(px(5.))
                            .items_center()
                            .cursor_pointer()
                            .rounded(px(5.))
                            .px(px(4.))
                            .hover(move |s| s.bg(hover))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.mcp.auto_open = !this.mcp.auto_open;
                                cx.notify();
                            }))
                            .child(Icon::new(if auto { IconName::CircleCheck } else { IconName::Square }).size(px(12.)))
                            .child("Abrir capturas en la vista previa"),
                    ),
            )
            .child("Al abrir un agente se genera su configuración MCP solo para esta sesión (nunca se toca la global).")
            .into_any_element()
    }
}
