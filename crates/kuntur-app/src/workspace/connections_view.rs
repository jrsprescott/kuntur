//! Gestor de conexiones (§8) y pestañas remotas (F6).
//!
//! - Botón de ícono arriba a la derecha (y Ctrl+Shift+O o la paleta): panel flotante con las
//!   conexiones guardadas; clic = pestaña remota conectada; lápiz = editar; papelera = borrar.
//! - Pestaña remota = la misma `TerminalView` sobre un `RemoteTerminal` de `kuntur-ssh`. Las
//!   preguntas (huella, passphrase, contraseña) y los errores salen dentro de la terminal.
//! - Si la conexión se cae: franja "Desconectado · Reconectar" sobre la terminal, Enter en la
//!   terminal o Ctrl+Shift+R.
//!
//! La lógica (formulario, validación, destino) vive en `crate::connections`, con pruebas.

use std::sync::Arc;

use kuntur_config::ConnectionConfig;
use kuntur_ssh::{
    KnownHosts, LinkState, MemorySecrets, RemoteSession, RemoteTerminal, SshContext, WINDOWS_AGENT_PIPE, system_store,
};
use kuntur_term::Pane;

use super::*;
use crate::connections::{ConnectionForm, FIELDS, Field, synthetic_connections, target_for, upsert};
use crate::terminal_view::{INITIAL_SIZE, notifier};

const PANEL_W: f32 = 460.0;
const ROW_H: f32 = 52.0;
const FIELD_H: f32 = 30.0;

/// Datos remotos de una pestaña.
pub(super) struct RemoteTab {
    /// Nombre de la conexión guardada (se guarda con la sesión).
    pub(super) connection: String,
    pub(super) session: Arc<RemoteSession>,
    /// Transporte de la terminal principal (estado, carpeta, reconectar).
    pub(super) main: Arc<RemoteTerminal>,
}

#[derive(Default)]
pub(super) struct ConnectionsState {
    pub(super) open: bool,
    form: Option<ConnectionForm>,
    /// Conexión que pide confirmar el borrado.
    confirm_delete: Option<String>,
    /// Fila elegida con el teclado (F11: ↑/↓, Enter, F2, Supr).
    selected: usize,
    /// Contexto SSH (known_hosts, secretos): se crea con la primera sesión remota.
    ctx: Option<SshContext>,
}

/// Carpeta de datos de Kuntur (`%APPDATA%\Kuntur`).
fn app_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Kuntur"))
}

/// ¿Prueba de humo del gestor? (conexiones sintéticas, secretos en memoria, nada del usuario).
fn smoke_mode() -> Option<String> {
    std::env::var("KUNTUR_SMOKE_CONNECTIONS").ok()
}

impl Workspace {
    // ── Contexto y sesiones remotas ─────────────────────────────────────────

    fn ssh_context(&mut self) -> SshContext {
        if let Some(ctx) = &self.connections.ctx {
            return ctx.clone();
        }
        let ctx = if smoke_mode().is_some() {
            // Nada del usuario: ni ~/.ssh, ni agente, ni el Administrador de credenciales.
            let dir = std::env::temp_dir().join(format!("kuntur-smoke-ssh-{}", std::process::id()));
            SshContext {
                known_hosts: KnownHosts::new(dir.join("known_hosts"), vec![]),
                secrets: Arc::new(MemorySecrets::new()),
                home: None,
                agent_pipe: None,
            }
        } else {
            let dir = app_dir().unwrap_or_else(std::env::temp_dir);
            SshContext {
                known_hosts: KnownHosts::for_user(&dir, self.home.as_deref()),
                secrets: system_store(),
                home: self.home.clone(),
                agent_pipe: Some(WINDOWS_AGENT_PIPE.into()),
            }
        };
        self.connections.ctx = Some(ctx.clone());
        ctx
    }

    /// Vista de terminal sobre un canal nuevo de `session` (conecta si hace falta).
    pub(super) fn new_remote_terminal(
        &self,
        session: &Arc<RemoteSession>,
        cwd: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<TerminalView>, Arc<RemoteTerminal>) {
        let (term, reader) = session.open_terminal(cwd, INITIAL_SIZE);
        let transport = term.clone();
        let palette = palette_from(&self.theme);
        let (family, size_pt, background) = (self.mono_font.clone(), self.settings.font_size, self.colors.terminal);
        let view = cx.new(|cx| {
            let make = move |tx| {
                Pane::with_transport(transport, Box::new(reader), INITIAL_SIZE, notifier(tx))
                    .expect("no se pudo crear el panel remoto")
            };
            let mut v = TerminalView::with_pane(window, cx, make, palette.clone(), family, size_pt);
            v.set_palette(palette, background, cx);
            // F9: el autocompletado mira el disco y el historial de esta PC, no los del servidor.
            v.set_remote();
            v
        });
        (view, term)
    }

    /// Abre una pestaña remota con la conexión guardada `name`. `false` si ya no existe.
    pub(super) fn open_remote(
        &mut self,
        name: &str,
        cwd: Option<String>,
        session_name: Option<String>,
        dock_ratio: Option<f32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(conn) = self.settings.connections.iter().find(|c| c.name == name).cloned() else {
            return false;
        };
        let ctx = self.ssh_context();
        let session = RemoteSession::new(target_for(&conn, self.home.as_deref()), ctx);
        let (view, main) = self.new_remote_terminal(&session, cwd, window, cx);
        let events = cx.subscribe_in(&view, window, |this, view, ev: &TerminalEvent, window, cx| match ev {
            TerminalEvent::TitleChanged => cx.notify(),
            TerminalEvent::Exited => {
                let id = this.sessions.iter().find(|e| &e.value.view == view).map(|e| e.id);
                if let Some(id) = id {
                    this.close_session(id, window, cx);
                }
            }
        });
        let name = session_name.unwrap_or_else(|| {
            let existing: Vec<&str> = self.sessions.iter().map(|e| e.name.as_str()).collect();
            next_default_name(&conn.name, &existing)
        });
        self.sessions.push(
            name,
            Session {
                manual_name: false,
                view,
                shell: None,
                dock: None,
                dock_state: DockState::with_ratio(dock_ratio),
                remote: Some(RemoteTab {
                    connection: conn.name.clone(),
                    session,
                    main,
                }),
                _events: events,
                _dock_events: None,
            },
        );
        cx.notify();
        true
    }

    /// Clic en una conexión: pestaña nueva, activa y con el teclado.
    fn connect_to(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.open_remote(name, None, None, None, window, cx) {
            self.persist();
            self.usage.open = false;
            self.close_connections(window, cx);
        }
    }

    /// Reconecta la pestaña activa (Ctrl+Shift+R, botón de la franja o la paleta).
    pub(super) fn reconnect_active(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.sessions.active().and_then(|e| e.value.remote.as_ref()) {
            r.main.reconnect();
        }
        cx.notify();
    }

    /// `usuario@host` de una sesión remota.
    pub(super) fn remote_label(&self, id: u64) -> Option<String> {
        self.sessions
            .get(id)
            .and_then(|e| e.value.remote.as_ref())
            .map(|r| r.session.target().destination())
    }

    /// Agentes del servidor de la sesión activa, si es remota (`Some(vacío)` = remota sin datos).
    pub(super) fn active_remote_agents(&self) -> Option<Vec<String>> {
        let r = self.sessions.active()?.value.remote.as_ref()?;
        Some(r.session.agents().unwrap_or_default())
    }

    // ── Panel ───────────────────────────────────────────────────────────────

    pub(super) fn toggle_connections(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.connections.open {
            self.close_connections(window, cx);
        } else {
            self.open_connections(window, cx);
        }
    }

    pub(super) fn open_connections(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        // Integración F10-F11: Conexiones y el gestor de MCP no se abren a la vez (cada uno se
        // queda con todo el teclado).
        self.mcp.open = false;
        self.settings_panel.open = false;
        self.connections.open = true;
        self.connections.confirm_delete = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn close_connections(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.connections.open = false;
        self.connections.form = None;
        self.connections.confirm_delete = None;
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(super) fn new_connection_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_connections(window, cx);
        self.connections.form = Some(ConnectionForm::new());
        cx.notify();
    }

    fn edit_connection(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(c) = self.settings.connections.iter().find(|c| c.name == name) {
            self.connections.form = Some(ConnectionForm::edit(c));
            self.connections.confirm_delete = None;
        }
        cx.notify();
    }

    /// Guarda el formulario (o muestra el error). Si se renombró, las pestañas lo siguen.
    fn save_form(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.connections.form.as_mut() else {
            return;
        };
        match form.to_config(&self.settings.connections) {
            Err(e) => form.error = Some(e),
            Ok(conn) => {
                let original = form.editing.clone();
                if let Some(old) = &original
                    && *old != conn.name
                {
                    let ids: Vec<u64> = self.sessions.iter().map(|e| e.id).collect();
                    for id in ids {
                        if let Some(r) = self
                            .sessions
                            .get_mut(id)
                            .and_then(|e| e.value.remote.as_mut())
                            .filter(|r| &r.connection == old)
                        {
                            r.connection = conn.name.clone();
                        }
                    }
                }
                upsert(&mut self.settings.connections, original.as_deref(), conn);
                self.connections.form = None;
                self.persist();
            }
        }
        cx.notify();
    }

    /// Borra la conexión y su contraseña recordada (las pestañas abiertas siguen conectadas).
    fn delete_connection(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(c) = self.settings.connections.iter().find(|c| c.name == name).cloned() {
            if c.remember {
                let target = target_for(&c, self.home.as_deref());
                self.ssh_context()
                    .secrets
                    .delete(&kuntur_ssh::auth::password_account(&target));
            }
            self.settings.connections.retain(|x| x.name != name);
            self.persist();
        }
        self.connections.confirm_delete = None;
        cx.notify();
    }

    /// Teclado con el panel abierto: formulario o lista (Esc cierra).
    pub(super) fn on_connections_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = ks.modifiers;
        let key = ks.key.to_ascii_lowercase();
        if m.control && m.shift && key == "o" {
            self.close_connections(window, cx);
            return;
        }
        // Ctrl+Shift+M pasa directo al gestor de MCP (que cierra este panel).
        if m.control && m.shift && !m.alt && key == "m" {
            self.open_mcp(window, cx);
            return;
        }
        let Some(form) = self.connections.form.as_mut() else {
            self.on_connections_list_key(&key, m, window, cx);
            return;
        };
        match key.as_str() {
            "escape" => self.connections.form = None,
            "enter" => self.save_form(cx),
            "tab" if m.shift => form.prev_field(),
            "tab" => form.next_field(),
            "up" => form.prev_field(),
            "down" => form.next_field(),
            "backspace" if m.control => form.clear_field(),
            "backspace" => form.backspace(),
            "v" if m.control && !m.alt => {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    form.insert(&text);
                }
            }
            // Ctrl+R alterna "recordar".
            "r" if m.control && !m.alt => form.remember = !form.remember,
            _ if !m.control || m.alt => {
                if let Some(text) = ks.key_char.as_deref() {
                    form.insert(text);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// Lista con el teclado (F11): ↑/↓ eligen, Enter conecta, F2 edita, Supr pide confirmar el
    /// borrado (Enter = sí, Esc = no), Ctrl+N nueva, Esc cierra.
    fn on_connections_list_key(&mut self, key: &str, m: Modifiers, window: &mut Window, cx: &mut Context<Self>) {
        let len = self.settings.connections.len();
        let selected = self.connections.selected.min(len.saturating_sub(1));
        let name = self.settings.connections.get(selected).map(|c| c.name.clone());
        let confirming = self.connections.confirm_delete.is_some();
        match key {
            "escape" if confirming => self.connections.confirm_delete = None,
            "escape" => {
                self.close_connections(window, cx);
                return;
            }
            "n" if m.control && !m.shift => {
                self.new_connection_form(window, cx);
                return;
            }
            _ if m.modified() => return,
            "up" | "down" => {
                self.connections.selected = crate::keynav::step(selected, len, key == "down");
                self.connections.confirm_delete = None;
            }
            "enter" if confirming => {
                if let Some(target) = self.connections.confirm_delete.clone() {
                    self.delete_connection(&target, cx);
                }
            }
            "enter" => {
                if let Some(name) = name {
                    self.connect_to(&name, window, cx);
                }
                return;
            }
            "f2" => {
                if let Some(name) = name {
                    self.edit_connection(&name, cx);
                }
            }
            "delete" => self.connections.confirm_delete = name,
            _ => return,
        }
        cx.notify();
    }

    /// Pruebas de humo: conexiones sintéticas y el panel abierto ("list"), el formulario ("form")
    /// o una pestaña remota a un puerto sin servidor ("remote") para ver el error y la franja.
    pub(super) fn smoke_connections(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mode) = smoke_mode() else {
            return;
        };
        self.config_path = None;
        self.settings.connections = synthetic_connections();
        match mode.as_str() {
            "form" => {
                self.open_connections(window, cx);
                self.edit_connection("staging", cx);
                if let Some(f) = self.connections.form.as_mut() {
                    f.focus = Field::Key;
                }
            }
            "remote" => {
                self.open_remote("local-apagado", None, None, None, window, cx);
                self.focus_active(window, cx);
            }
            _ => self.open_connections(window, cx),
        }
    }

    // ── Dibujo ──────────────────────────────────────────────────────────────

    /// Franja sobre la terminal de una sesión remota sin conexión.
    pub(super) fn render_remote_banner(&self, id: u64, cx: &mut Context<Self>) -> Option<AnyElement> {
        let r = self.sessions.get(id)?.value.remote.as_ref()?;
        if r.main.state() != LinkState::Disconnected {
            return None;
        }
        let c = &self.colors;
        let detail = r.main.last_error().unwrap_or_default();
        let hover = c.hover;
        Some(
            h_flex()
                .h(px(30.))
                .flex_shrink_0()
                .px(px(12.))
                .gap(px(8.))
                .items_center()
                .bg(c.terminal)
                .border_b_1()
                .border_color(c.border)
                .text_size(px(12.))
                .child(div().size(px(7.)).rounded_full().bg(c.error).flex_shrink_0())
                .child(div().flex_shrink_0().text_color(c.text).child("Desconectado"))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(c.muted)
                        .child(format!("· {} · {detail}", r.session.target().destination())),
                )
                .child(
                    h_flex()
                        .id(("reconnect", id))
                        .flex_shrink_0()
                        .h(px(22.))
                        .px(px(8.))
                        .gap(px(6.))
                        .items_center()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_color(c.accent)
                        .hover(move |s| s.bg(hover))
                        .tooltip(|window, cx| Tooltip::new("Reconectar (Ctrl+Shift+R)").build(window, cx))
                        .on_click(cx.listener(|this, _, _, cx| this.reconnect_active(cx)))
                        .child(Icon::new(IconName::RefreshCw).size(px(12.)))
                        .child("Reconectar"),
                )
                .into_any_element(),
        )
    }

    /// Panel flotante bajo el botón (arriba a la derecha). Un clic fuera lo cierra.
    pub(super) fn render_connections(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = self.colors.clone();
        let body = match &self.connections.form {
            Some(form) => self.render_form(form, cx),
            None => self.render_list(cx),
        };
        let title = match &self.connections.form {
            Some(f) if f.editing.is_some() => "Editar conexión",
            Some(_) => "Nueva conexión",
            None => "Conexiones",
        };
        let hover = c.hover;
        let header = h_flex()
            .h(px(44.))
            .px(px(14.))
            .gap(px(8.))
            .items_center()
            .border_b_1()
            .border_color(c.border)
            .child(Icon::new(IconName::Server).size(px(15.)).text_color(c.muted))
            .child(
                div()
                    .flex_1()
                    .text_size(px(13.5))
                    .font_weight(FontWeight::MEDIUM)
                    .child(title),
            )
            .when(self.connections.form.is_none(), |this| {
                this.child(
                    h_flex()
                        .id("conn-new")
                        .h(px(26.))
                        .px(px(8.))
                        .gap(px(6.))
                        .items_center()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_size(px(12.))
                        .text_color(c.accent)
                        .hover(move |s| s.bg(hover))
                        .tooltip(|window, cx| Tooltip::new("Nueva conexión (Ctrl+N)").build(window, cx))
                        .on_click(cx.listener(|this, _, window, cx| this.new_connection_form(window, cx)))
                        .child(Icon::new(IconName::Plus).size(px(13.)))
                        .child("Nueva"),
                )
            });

        let panel = v_flex()
            .id("connections")
            .track_focus(&self.focus)
            .w(px(PANEL_W))
            .max_w(relative(0.95))
            .max_h(px(560.))
            .bg(c.popover)
            .border_1()
            .border_color(c.border)
            .rounded(px(10.))
            .shadow_lg()
            .overflow_hidden()
            .occlude()
            .font_family(self.ui_font.clone())
            .text_color(c.text)
            // Los clics dentro del panel no lo cierran.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .child(body);

        deferred(
            div()
                .id("connections-backdrop")
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
                    cx.listener(|this, _, window, cx| this.close_connections(window, cx)),
                )
                .child(panel),
        )
        .with_priority(1)
        .into_any_element()
    }

    fn render_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        if self.settings.connections.is_empty() {
            return v_flex()
                .py(px(26.))
                .px(px(20.))
                .gap(px(6.))
                .items_center()
                .text_center()
                .child(div().text_size(px(13.)).child("Sin conexiones guardadas"))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(c.muted)
                        .child("«Nueva» guarda host, puerto, usuario y llave. Las contraseñas nunca van al archivo de configuración."),
                )
                .into_any_element();
        }
        let rows: Vec<AnyElement> = self
            .settings
            .connections
            .iter()
            .enumerate()
            .map(|(i, conn)| self.render_row(i, conn, cx))
            .collect();
        v_flex()
            .child(
                v_flex()
                    .id("connections-list")
                    .max_h(px(420.))
                    .overflow_y_scroll()
                    .p(px(6.))
                    .gap(px(2.))
                    .children(rows),
            )
            .child(
                div()
                    .px(px(14.))
                    .py(px(8.))
                    .border_t_1()
                    .border_color(c.border)
                    .text_size(px(11.))
                    .text_color(c.muted)
                    .child("Clic o Enter: nueva pestaña conectada · ↑↓ elegir · F2 editar · Supr borrar · Contraseñas solo en el Administrador de credenciales de Windows"),
            )
            .into_any_element()
    }

    /// Fila: nombre, `usuario@host:puerto`, llave y, si hay una pestaña abierta, su estado y los
    /// agentes del servidor. A la derecha (al pasar el mouse) editar y borrar.
    fn render_row(&self, index: usize, conn: &ConnectionConfig, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let hover = c.hover;
        let name = conn.name.clone();
        let group: SharedString = format!("conn-{index}").into();
        // Pestañas abiertas con esta conexión.
        let open: Vec<&RemoteTab> = self
            .sessions
            .iter()
            .filter_map(|e| e.value.remote.as_ref())
            .filter(|r| r.connection == conn.name)
            .collect();
        let state = open.iter().map(|r| r.main.state()).max_by_key(|s| match s {
            LinkState::Connected => 3,
            LinkState::Asking | LinkState::Connecting => 2,
            LinkState::Disconnected => 1,
        });
        let agents = open.iter().find_map(|r| r.session.agents());
        let mut detail = vec![conn.destination()];
        detail.push(match &conn.key {
            Some(k) => format!("llave {k}"),
            None => "agente o ~/.ssh".into(),
        });
        if conn.remember {
            detail.push("recuerda la contraseña".into());
        }
        let confirming = self.connections.confirm_delete.as_deref() == Some(conn.name.as_str());
        // F11: la fila elegida con el teclado se resalta y muestra sus acciones (F2 / Supr).
        let selected = index
            == self
                .connections
                .selected
                .min(self.settings.connections.len().saturating_sub(1));

        let actions: AnyElement = if confirming {
            let yes = name.clone();
            h_flex()
                .gap(px(4.))
                .items_center()
                .text_size(px(12.))
                .child(div().text_color(c.muted).child("¿Borrar?"))
                .child(
                    div()
                        .id(("conn-del-yes", index))
                        .px(px(6.))
                        .py(px(2.))
                        .rounded(px(5.))
                        .cursor_pointer()
                        .text_color(c.error)
                        .hover(move |s| s.bg(hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.delete_connection(&yes, cx);
                        }))
                        .child("Sí"),
                )
                .child(
                    div()
                        .id(("conn-del-no", index))
                        .px(px(6.))
                        .py(px(2.))
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.connections.confirm_delete = None;
                            cx.notify();
                        }))
                        .child("No"),
                )
                .into_any_element()
        } else {
            let (edit_name, del_name) = (name.clone(), name.clone());
            h_flex()
                .gap(px(2.))
                .opacity(if selected { 1. } else { 0. })
                .group_hover(group.clone(), |s| s.opacity(1.))
                .child(
                    row_icon(("conn-edit", index), IconName::Pencil, c, "Editar").on_click(cx.listener(
                        move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.edit_connection(&edit_name, cx);
                        },
                    )),
                )
                .child(
                    row_icon(("conn-del", index), IconName::Trash, c, "Borrar").on_click(cx.listener(
                        move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.connections.confirm_delete = Some(del_name.clone());
                            cx.notify();
                        },
                    )),
                )
                .into_any_element()
        };

        let dot = state.map(|s| {
            div().size(px(7.)).rounded_full().flex_shrink_0().bg(match s {
                LinkState::Connected => c.ok,
                LinkState::Disconnected => c.error,
                _ => c.warn,
            })
        });
        let agents_line = agents.filter(|a| !a.is_empty()).map(|a| {
            div()
                .text_size(px(11.))
                .text_color(c.muted)
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(format!("En el servidor: {}", a.join(" · ")))
        });

        h_flex()
            .id(("conn-row", index))
            .group(group)
            .min_h(px(ROW_H))
            .px(px(10.))
            .py(px(6.))
            .gap(px(10.))
            .items_center()
            .rounded(px(7.))
            .cursor_pointer()
            .when(selected, |s| s.bg(hover))
            .hover(move |s| s.bg(hover))
            .on_click(cx.listener(move |this, _, window, cx| this.connect_to(&name, window, cx)))
            .child(
                div()
                    .size(px(30.))
                    .flex_shrink_0()
                    .rounded(px(8.))
                    .bg(c.surface_2)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(c.muted)
                    .child(Icon::new(IconName::Server).size(px(15.))),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(1.))
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(conn.name.clone()),
                            )
                            .children(dot),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(c.muted)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(detail.join(" · ")),
                    )
                    .children(agents_line),
            )
            .child(actions)
            .into_any_element()
    }

    fn render_form(&self, form: &ConnectionForm, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let hover = c.hover;
        let fields: Vec<AnyElement> = FIELDS.iter().map(|&f| self.render_field(f, form, cx)).collect();
        let check = div()
            .size(px(15.))
            .flex_shrink_0()
            .rounded(px(4.))
            .border_1()
            .border_color(if form.remember { c.accent } else { c.border })
            .when(form.remember, |this| this.bg(c.accent))
            .flex()
            .items_center()
            .justify_center()
            .when(form.remember, |this| {
                this.child(Icon::new(IconName::Check).size(px(11.)).text_color(c.popover))
            });
        let remember = h_flex()
            .id("conn-remember")
            .gap(px(8.))
            .items_start()
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(f) = this.connections.form.as_mut() {
                    f.remember = !f.remember;
                }
                cx.notify();
            }))
            .child(div().pt(px(1.)).child(check))
            .child(
                v_flex()
                    .gap(px(1.))
                    .child(div().text_size(px(12.5)).child("Recordar contraseña o passphrase"))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(c.muted)
                            .child("En el Administrador de credenciales de Windows, nunca en config.toml (Ctrl+R)"),
                    ),
            );
        let button = |id: &'static str, label: &'static str, primary: bool| {
            div()
                .id(id)
                .h(px(28.))
                .px(px(12.))
                .rounded(px(7.))
                .flex()
                .items_center()
                .cursor_pointer()
                .text_size(px(12.5))
                .map(|this| {
                    if primary {
                        this.bg(c.accent).text_color(c.popover)
                    } else {
                        this.text_color(c.muted).hover(move |s| s.bg(hover))
                    }
                })
                .child(label)
        };
        v_flex()
            .p(px(14.))
            .gap(px(10.))
            .children(fields)
            .child(remember)
            .when_some(form.error.clone(), |this, e| {
                this.child(
                    h_flex()
                        .gap(px(6.))
                        .items_center()
                        .text_size(px(12.))
                        .text_color(c.error)
                        .child(Icon::new(IconName::CircleAlert).size(px(13.)))
                        .child(e),
                )
            })
            .child(
                h_flex()
                    .pt(px(2.))
                    .gap(px(6.))
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.))
                            .text_color(c.muted)
                            .child("Tab cambia de campo · Enter guarda · Esc vuelve"),
                    )
                    .child(
                        button("conn-cancel", "Cancelar", false).on_click(cx.listener(|this, _, _, cx| {
                            this.connections.form = None;
                            cx.notify();
                        })),
                    )
                    .child(
                        button("conn-save", "Guardar", true).on_click(cx.listener(|this, _, _, cx| this.save_form(cx))),
                    ),
            )
            .into_any_element()
    }

    /// Campo de texto: etiqueta arriba, valor (o guía tenue) y cursor si tiene el foco.
    fn render_field(&self, f: Field, form: &ConnectionForm, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let focused = form.focus == f;
        let value = form.value(f).to_string();
        let caret = div().w(px(1.5)).h(px(15.)).flex_shrink_0().bg(c.accent);
        let width: DefiniteLength = match f {
            Field::Port => px(90.).into(),
            _ => relative(1.),
        };
        v_flex()
            .id(SharedString::from(format!("conn-field-{}", f.label())))
            .gap(px(4.))
            .w(width)
            .cursor_text()
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(form) = this.connections.form.as_mut() {
                    form.focus = f;
                }
                cx.notify();
            }))
            .child(div().text_size(px(11.)).text_color(c.muted).child(f.label()))
            .child(
                h_flex()
                    .h(px(FIELD_H))
                    .px(px(9.))
                    .items_center()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(if focused { c.accent } else { c.border })
                    .bg(c.surface)
                    .text_size(px(12.5))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .map(|this| {
                        if value.is_empty() {
                            this.when(focused, |t| t.child(caret))
                                .child(div().ml(px(1.)).text_color(c.muted.opacity(0.7)).child(f.placeholder()))
                        } else {
                            this.child(div().text_color(c.text).child(value))
                                .when(focused, |t| t.child(caret))
                        }
                    }),
            )
            .into_any_element()
    }
}

/// Botón de ícono chico de una fila.
fn row_icon(id: impl Into<ElementId>, icon: IconName, c: &UiColors, tip: &'static str) -> Stateful<Div> {
    let (hover, text) = (c.hover, c.text);
    div()
        .id(id)
        .size(px(24.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .text_color(c.muted)
        .cursor_pointer()
        .hover(move |s| s.bg(hover).text_color(text))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .child(Icon::new(icon).size(px(13.)))
}
