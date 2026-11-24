//! Sesión remota de una pestaña: una conexión compartida ([`RemoteSession`]) y una o más terminales
//! sobre ella ([`RemoteTerminal`]: la principal y la acoplada), cada una en su propio canal.
//!
//! Una terminal remota es un [`Transport`] de `kuntur-term`: el mismo `Pane` y la misma vista que una
//! local. Si la conexión se cae, la terminal lo dice en pantalla y Enter (o [`RemoteTerminal::reconnect`])
//! vuelve a conectar en la última carpeta conocida, sin perder el historial.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use kuntur_term::{CwdScanner, GridSize, Transport};
use russh::ChannelMsg;
use tokio::sync::mpsc;

use crate::auth::SshTarget;
use crate::client::{Connection, SshContext, connect, runtime};
use crate::commands::shell_in_dir;
use crate::console::{ChannelCmd, Console, DIM, Input, OutputReader, RED, RESET, Routed};
use crate::sftp::{SftpFs, SftpSlot, block};

/// Estado de una terminal remota (para la franja "Desconectado · Reconectar" de la app).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    Connecting,
    /// Esperando una respuesta del usuario (huella, passphrase, contraseña).
    Asking,
    Connected,
    Disconnected,
}

/// Conexión de una pestaña: la comparten la terminal principal y la acoplada.
pub struct RemoteSession {
    target: SshTarget,
    ctx: SshContext,
    conn: tokio::sync::Mutex<Option<Connection>>,
    agents: Mutex<Option<Vec<String>>>,
    sftp: SftpSlot,
}

impl RemoteSession {
    pub fn new(target: SshTarget, ctx: SshContext) -> Arc<Self> {
        Arc::new(Self {
            target,
            ctx,
            conn: tokio::sync::Mutex::new(None),
            agents: Mutex::new(None),
            sftp: SftpSlot::default(),
        })
    }

    pub fn target(&self) -> &SshTarget {
        &self.target
    }

    /// Agentes de IA encontrados en el servidor (`None` hasta la primera conexión).
    pub fn agents(&self) -> Option<Vec<String>> {
        self.agents.lock().unwrap().clone()
    }

    /// ¿Hay una conexión viva?
    pub fn is_connected(&self) -> bool {
        self.conn
            .try_lock()
            .map(|c| c.as_ref().is_some_and(|c| !c.is_closed()))
            .unwrap_or(false)
    }

    /// Árbol remoto por SFTP sobre la conexión viva (F7). **Bloquea**: llamarlo fuera del hilo de
    /// la UI. No conecta ni pregunta nada: sin conexión (o mientras se conecta) devuelve un error.
    /// Si la conexión se rehízo, abre un canal SFTP nuevo.
    pub fn sftp(&self) -> anyhow::Result<Arc<SftpFs>> {
        let conn = match self.conn.try_lock() {
            Ok(slot) => slot.as_ref().filter(|c| !c.is_closed()).cloned(),
            Err(_) => anyhow::bail!("conectando…"),
        };
        let Some(conn) = conn else {
            anyhow::bail!("sin conexión con el servidor");
        };
        let mut cached = self.sftp.0.lock().unwrap();
        if let Some((c, fs)) = cached.as_ref()
            && c.same(&conn)
        {
            return Ok(fs.clone());
        }
        let fs = Arc::new(block(conn.open_sftp(self.target.destination()))?);
        *cached = Some((conn, fs.clone()));
        Ok(fs)
    }

    /// Terminal nueva sobre esta conexión (conecta si hace falta; si ya está conectada, abre otro
    /// canal sin volver a autenticar). `cwd`: carpeta donde abrir el shell.
    pub fn open_terminal(self: &Arc<Self>, cwd: Option<String>, size: GridSize) -> (Arc<RemoteTerminal>, OutputReader) {
        let (console, reader) = Console::new();
        let term = Arc::new_cyclic(|me| RemoteTerminal {
            me: me.clone(),
            session: self.clone(),
            console,
            size: Mutex::new(size),
            cwd: Mutex::new(cwd),
            finished: AtomicBool::new(false),
            killed: AtomicBool::new(false),
            last_error: Mutex::new(None),
        });
        term.start();
        (term, reader)
    }

    /// La conexión viva o una nueva (las preguntas salen en `console`).
    async fn connection(self: &Arc<Self>, console: &Console) -> anyhow::Result<Connection> {
        let mut slot = self.conn.lock().await;
        if let Some(c) = slot.as_ref().filter(|c| !c.is_closed()) {
            return Ok(c.clone());
        }
        let conn = connect(&self.target, &self.ctx, console).await?;
        *slot = Some(conn.clone());
        // Agentes del servidor (§5), en otro canal y sin esperar.
        let (weak, probe) = (Arc::downgrade(self), conn.clone());
        runtime().spawn(async move {
            let agents = probe.detect_agents().await;
            if let Some(me) = weak.upgrade() {
                *me.agents.lock().unwrap() = Some(agents);
            }
        });
        Ok(conn)
    }
}

impl Drop for RemoteSession {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.conn.try_lock()
            && let Some(conn) = slot.take()
        {
            runtime().spawn(async move { conn.disconnect().await });
        }
    }
}

/// Cómo terminó un canal.
enum Outcome {
    /// El shell terminó (`exit`): la pestaña se cierra como una local.
    Exited,
    /// Se cayó la conexión: se ofrece reconectar.
    Lost,
    /// Lo cerró Kuntur (se cerró la pestaña).
    Closed,
}

/// Una terminal remota: un canal con PTY sobre la conexión de su [`RemoteSession`].
pub struct RemoteTerminal {
    me: Weak<RemoteTerminal>,
    session: Arc<RemoteSession>,
    console: Console,
    size: Mutex<GridSize>,
    /// Última carpeta informada por el shell (OSC 7): ahí se reconecta.
    cwd: Mutex<Option<String>>,
    finished: AtomicBool,
    killed: AtomicBool,
    last_error: Mutex<Option<String>>,
}

impl RemoteTerminal {
    pub fn session(&self) -> &Arc<RemoteSession> {
        &self.session
    }

    pub fn state(&self) -> LinkState {
        if self.console.is_connected() {
            LinkState::Connected
        } else if self.console.is_prompting() {
            LinkState::Asking
        } else if self.console.is_disconnected() {
            LinkState::Disconnected
        } else {
            LinkState::Connecting
        }
    }

    /// Último error de conexión (se borra al conectar).
    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }

    /// Última carpeta conocida del shell remoto.
    pub fn cwd(&self) -> Option<String> {
        self.cwd.lock().unwrap().clone()
    }

    /// Vuelve a conectar (botón "Reconectar", Ctrl+Shift+R). Solo si está desconectada.
    pub fn reconnect(&self) {
        if self.console.is_disconnected() {
            self.console.set_input(Input::Idle);
            self.start();
        }
    }

    fn start(&self) {
        if let Some(me) = self.me.upgrade() {
            runtime().spawn(async move { me.run().await });
        }
    }

    fn fail(&self, what: &str, err: &str) {
        if self.killed.load(Ordering::SeqCst) {
            return;
        }
        *self.last_error.lock().unwrap() = Some(err.to_string());
        // Ajustes v0.2.2: si se cortó con un programa a pantalla completa abierto, el aviso y el
        // historial vuelven a la pantalla principal, con el mouse apagado.
        self.console.output(kuntur_term::MODES_RESET.to_vec());
        self.console.print(&format!(
            "{RED}{what}: {err}{RESET}\n{DIM}Enter o Ctrl+Shift+R para reintentar · Ctrl+Shift+W cierra la pestaña.{RESET}\n"
        ));
        self.console.set_input(Input::Disconnected);
    }

    async fn run(self: Arc<Self>) {
        let target = self.session.target.clone();
        // Ajustes v0.2.2: la pestaña puede venir de una conexión anterior con un programa a pantalla
        // completa que dejó el mouse, la pantalla alterna o el cursor oculto: estado limpio antes de
        // conectar (no se envía nada al servidor; solo lo procesa la terminal de Kuntur).
        self.console.output(kuntur_term::MODES_RESET.to_vec());
        self.console
            .print(&format!("{DIM}Conectando a {}…{RESET}\n", target.destination()));
        let conn = match self.session.connection(&self.console).await {
            Ok(c) => c,
            Err(e) => return self.fail("No se pudo conectar", &e.to_string()),
        };
        let size = *self.size.lock().unwrap();
        let command = self.cwd().map(|d| shell_in_dir(&d));
        let channel = match conn
            .open_terminal(size.cols as u32, size.rows as u32, command.as_deref())
            .await
        {
            Ok(c) => c,
            Err(e) => return self.fail("No se pudo abrir la terminal remota", &e.to_string()),
        };
        if self.killed.load(Ordering::SeqCst) {
            let _ = channel.close().await;
            return;
        }
        *self.last_error.lock().unwrap() = None;
        let (tx, rx) = mpsc::unbounded_channel();
        self.console.set_input(Input::Channel(tx));
        // Por si el tamaño cambió mientras se conectaba.
        let now = *self.size.lock().unwrap();
        if now != size {
            self.console.send(ChannelCmd::Resize(now.cols as u32, now.rows as u32));
        }

        match self.pump(channel, rx, &conn).await {
            Outcome::Exited => {
                self.finished.store(true, Ordering::SeqCst);
                self.console.set_input(Input::Idle);
                self.console.close_output();
            }
            Outcome::Closed => {}
            Outcome::Lost => {
                if !self.killed.load(Ordering::SeqCst) {
                    self.console.set_input(Input::Idle);
                    self.fail("Conexión perdida", "el servidor dejó de responder o se cortó la red");
                }
            }
        }
    }

    async fn pump(
        &self,
        channel: russh::Channel<russh::client::Msg>,
        mut rx: mpsc::UnboundedReceiver<ChannelCmd>,
        conn: &Connection,
    ) -> Outcome {
        let (mut read, write) = channel.split();
        let mut scanner = CwdScanner::new();
        let mut exited = false;
        loop {
            tokio::select! {
                msg = read.wait() => match msg {
                    Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                        if let Some(dir) = scanner.feed(&data) {
                            *self.cwd.lock().unwrap() = Some(dir);
                        }
                        self.console.output(data.to_vec());
                    }
                    Some(ChannelMsg::ExitStatus { .. }) | Some(ChannelMsg::ExitSignal { .. }) => exited = true,
                    Some(ChannelMsg::Close) | None => break,
                    Some(_) => {}
                },
                cmd = rx.recv() => match cmd {
                    Some(ChannelCmd::Write(bytes)) => {
                        if write.data_bytes(bytes).await.is_err() {
                            break;
                        }
                    }
                    Some(ChannelCmd::Resize(cols, rows)) => {
                        let _ = write.window_change(cols, rows, 0, 0).await;
                    }
                    Some(ChannelCmd::Close) | None => {
                        let _ = write.close().await;
                        return Outcome::Closed;
                    }
                },
            }
        }
        if exited {
            return Outcome::Exited;
        }
        // Sin "exit-status": si la conexión también se cerró, fue una caída.
        tokio::time::sleep(Duration::from_millis(300)).await;
        if conn.is_closed() {
            Outcome::Lost
        } else {
            Outcome::Exited
        }
    }
}

impl Transport for RemoteTerminal {
    fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        if let Routed::Reconnect = self.console.route(bytes) {
            self.start();
        }
        Ok(())
    }

    fn resize(&self, size: GridSize) -> anyhow::Result<()> {
        *self.size.lock().unwrap() = size;
        self.console
            .send(ChannelCmd::Resize(size.cols as u32, size.rows as u32));
        Ok(())
    }

    fn has_exited(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    fn kill(&self) {
        self.killed.store(true, Ordering::SeqCst);
        if !self.console.send(ChannelCmd::Close) {
            // Si estaba preguntando algo, la pregunta queda sin respuesta.
            self.console.set_input(Input::Idle);
        }
    }
}
