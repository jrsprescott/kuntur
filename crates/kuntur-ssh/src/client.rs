//! Conexión SSH con `russh`: huella del servidor, autenticación (agente → llaves → contraseña) y
//! canales. Las preguntas al usuario salen por la [`Console`] de la terminal que conecta.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use russh::client::{self, AuthResult, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::{HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{ChannelMsg, MethodKind};
use zeroize::Zeroizing;

use crate::auth::{AuthStep, SshTarget, auth_plan, display_key, passphrase_account, password_account};
use crate::commands::{detect_agents_command, parse_agents};
use crate::console::{BOLD, Console, DIM, RED, RESET, YELLOW};
use crate::known_hosts::{HostKeyStatus, KnownHosts, fingerprint, key_kind};
use crate::prompt::is_yes;
use crate::secrets::SecretStore;

/// Named pipe del ssh-agent de OpenSSH para Windows.
pub const WINDOWS_AGENT_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Intercambio de llaves, incluida la pregunta de la huella (el usuario puede tardar en mirarla).
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(300);
const EXEC_TIMEOUT: Duration = Duration::from_secs(15);
/// Intentos de passphrase o contraseña antes de pasar al siguiente método.
const TRIES: usize = 3;

/// Runtime de `tokio` de las conexiones: se crea la primera vez que se abre una sesión remota.
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("kuntur-ssh")
            .enable_all()
            .build()
            .expect("no se pudo crear el runtime de SSH")
    })
}

/// Lo que una conexión necesita además del destino.
#[derive(Clone)]
pub struct SshContext {
    pub known_hosts: KnownHosts,
    pub secrets: Arc<dyn SecretStore>,
    /// Carpeta personal (para `~/.ssh/id_*` y para mostrar rutas cortas).
    pub home: Option<PathBuf>,
    /// Named pipe del agente; `None` = no usar agente (pruebas).
    pub agent_pipe: Option<String>,
}

/// Conexión autenticada, compartida por todos los canales de una pestaña.
#[derive(Clone)]
pub struct Connection {
    handle: Arc<Handle<ClientHandler>>,
}

impl Connection {
    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }

    /// ¿Es la misma conexión (no una reconexión)?
    pub fn same(&self, other: &Connection) -> bool {
        Arc::ptr_eq(&self.handle, &other.handle)
    }

    /// Canal `subsystem sftp` sobre esta conexión (F7).
    pub async fn open_sftp(&self, label: String) -> anyhow::Result<crate::sftp::SftpFs> {
        let channel = self.handle.channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        crate::sftp::SftpFs::open(channel.into_stream(), label).await
    }

    /// Canal con PTY `xterm-256color`: el shell de inicio de sesión o, con `command`, ese comando.
    pub async fn open_terminal(
        &self,
        cols: u32,
        rows: u32,
        command: Option<&str>,
    ) -> anyhow::Result<russh::Channel<client::Msg>> {
        let channel = self.handle.channel_open_session().await?;
        channel
            .request_pty(false, "xterm-256color", cols, rows, 0, 0, &[])
            .await?;
        match command {
            Some(cmd) => channel.exec(true, cmd).await?,
            None => channel.request_shell(true).await?,
        }
        Ok(channel)
    }

    /// Ejecuta un comando sin PTY y devuelve su salida estándar.
    pub async fn exec_output(&self, command: &str) -> anyhow::Result<String> {
        let mut channel = self.handle.channel_open_session().await?;
        channel.exec(true, command).await?;
        let mut out = Vec::new();
        let read = async {
            while let Some(msg) = channel.wait().await {
                match msg {
                    ChannelMsg::Data { data } => out.extend_from_slice(&data),
                    ChannelMsg::Eof | ChannelMsg::Close => break,
                    _ => {}
                }
            }
        };
        tokio::time::timeout(EXEC_TIMEOUT, read)
            .await
            .map_err(|_| anyhow::anyhow!("el comando tardó demasiado"))?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// Agentes de IA instalados en el servidor (§5).
    pub async fn detect_agents(&self) -> Vec<String> {
        self.exec_output(&detect_agents_command())
            .await
            .map(|out| parse_agents(&out))
            .unwrap_or_default()
    }

    pub async fn disconnect(&self) {
        let _ = self.handle.disconnect(russh::Disconnect::ByApplication, "", "es").await;
    }
}

/// Recibe la llave del servidor y decide (preguntando si es nueva).
pub struct ClientHandler {
    host: String,
    port: u16,
    known: KnownHosts,
    console: Console,
    /// Motivo del rechazo, para el mensaje de error.
    verdict: Arc<Mutex<Option<String>>>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, server_key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let key = match server_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key,
            PublicKeyOrCertificate::Certificate(_) => {
                *self.verdict.lock().unwrap() =
                    Some("el servidor presentó un certificado de host (todavía no soportado)".into());
                return Ok(false);
            }
        };
        let shown = if self.port == 22 {
            self.host.clone()
        } else {
            format!("[{}]:{}", self.host, self.port)
        };
        match self.known.check(&self.host, self.port, key) {
            HostKeyStatus::Known => Ok(true),
            HostKeyStatus::Unknown => {
                self.console.print(&format!(
                    "{YELLOW}No se conoce la huella de {shown}.{RESET}\n  Llave {}: {BOLD}{}{RESET}\n  Compárala con la del servidor (`ssh-keygen -lf /etc/ssh/ssh_host_*_key.pub`).\n",
                    key_kind(key),
                    fingerprint(key)
                ));
                let answer = self
                    .console
                    .ask("¿Confiar en este servidor y recordarlo? (s/n): ", false)
                    .await;
                if answer.as_deref().is_some_and(is_yes) {
                    if let Err(e) = self.known.learn(&self.host, self.port, key) {
                        self.console
                            .print(&format!("{YELLOW}No se pudo guardar la huella: {e}{RESET}\n"));
                    }
                    Ok(true)
                } else {
                    *self.verdict.lock().unwrap() = Some("no se confió en la huella del servidor".into());
                    Ok(false)
                }
            }
            HostKeyStatus::Changed { file, line } => {
                self.console.print(&format!(
                    "{RED}{BOLD}¡ATENCIÓN: LA HUELLA DE {shown} CAMBIÓ!{RESET}\n{RED}Alguien podría estar interceptando la conexión, o el servidor se reinstaló.\n  Llave {} recibida: {}\n  La recordada está en {}, línea {line}.\nKuntur no se conecta. Si el cambio es legítimo, borra esa línea y vuelve a intentar.{RESET}\n",
                    key_kind(key),
                    fingerprint(key),
                    file.display()
                ));
                *self.verdict.lock().unwrap() = Some("la huella del servidor cambió".into());
                Ok(false)
            }
        }
    }
}

/// Error de conexión en palabras para el usuario.
fn friendly(err: &russh::Error, verdict: Option<String>) -> String {
    if let Some(v) = verdict {
        return v;
    }
    match err {
        russh::Error::IO(e) => match e.kind() {
            std::io::ErrorKind::ConnectionRefused => {
                "el servidor rechazó la conexión (¿host y puerto correctos?)".into()
            }
            std::io::ErrorKind::TimedOut => "tiempo de espera agotado".into(),
            std::io::ErrorKind::NotFound => "no se encontró el host".into(),
            _ if e.to_string().contains("11001") || e.to_string().to_lowercase().contains("host") => {
                format!("no se encontró el host ({e})")
            }
            _ => format!("error de red: {e}"),
        },
        russh::Error::UnknownKey => "huella del servidor rechazada".into(),
        russh::Error::Disconnect | russh::Error::HUP => "el servidor cerró la conexión".into(),
        other => other.to_string(),
    }
}

/// Abre la conexión y autentica. Lo que haya que preguntar se pregunta en `console`.
pub(crate) async fn connect(target: &SshTarget, ctx: &SshContext, console: &Console) -> anyhow::Result<Connection> {
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        inactivity_timeout: None,
        ..Default::default()
    });
    let verdict = Arc::new(Mutex::new(None));
    let handler = ClientHandler {
        host: target.host.clone(),
        port: target.port,
        known: ctx.known_hosts.clone(),
        console: console.clone(),
        verdict: verdict.clone(),
    };
    let tcp = tokio::net::TcpStream::connect((target.host.as_str(), target.port));
    let stream = match tokio::time::timeout(CONNECT_TIMEOUT, tcp).await {
        Err(_) => anyhow::bail!("tiempo de espera agotado"),
        Ok(Err(e)) => anyhow::bail!(friendly(&russh::Error::IO(e), None)),
        Ok(Ok(s)) => s,
    };
    let _ = stream.set_nodelay(true);
    // El límite incluye la pregunta de la huella: es generoso.
    let handshake = client::connect_stream(config, stream, handler);
    let mut handle = match tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake).await {
        Err(_) => anyhow::bail!("tiempo de espera agotado"),
        Ok(Err(e)) => anyhow::bail!(friendly(&e, verdict.lock().unwrap().take())),
        Ok(Ok(h)) => h,
    };

    let ssh_dir = ctx.home.as_ref().map(|h| h.join(".ssh"));
    for step in auth_plan(target, ssh_dir.as_deref(), Path::is_file) {
        let ok = match step {
            AuthStep::Agent => try_agent(&mut handle, target, ctx).await,
            AuthStep::KeyFile(path) => try_key_file(&mut handle, target, ctx, console, &path).await?,
            AuthStep::Password => try_password(&mut handle, target, ctx, console).await?,
        };
        if ok {
            return Ok(Connection {
                handle: Arc::new(handle),
            });
        }
    }
    anyhow::bail!("el servidor no aceptó ninguna forma de autenticación (agente, llaves ni contraseña)")
}

/// Hash para llaves RSA (`rsa-sha2-512` si el servidor lo acepta).
async fn rsa_hash(handle: &Handle<ClientHandler>, key_is_rsa: bool) -> Option<HashAlg> {
    if !key_is_rsa {
        return None;
    }
    handle.best_supported_rsa_hash().await.ok().flatten().flatten()
}

#[cfg(windows)]
async fn try_agent(handle: &mut Handle<ClientHandler>, target: &SshTarget, ctx: &SshContext) -> bool {
    use russh::keys::agent::AgentIdentity;
    use russh::keys::agent::client::AgentClient;
    let Some(pipe) = &ctx.agent_pipe else {
        return false;
    };
    let Ok(Ok(mut agent)) = tokio::time::timeout(Duration::from_secs(2), AgentClient::connect_named_pipe(pipe)).await
    else {
        return false;
    };
    let Ok(identities) = agent.request_identities().await else {
        return false;
    };
    for id in identities {
        let AgentIdentity::PublicKey { key, .. } = id else {
            continue;
        };
        let hash = rsa_hash(handle, key.algorithm().is_rsa()).await;
        if let Ok(AuthResult::Success) = handle
            .authenticate_publickey_with(target.user.clone(), key, hash, &mut agent)
            .await
        {
            return true;
        }
    }
    false
}

#[cfg(not(windows))]
async fn try_agent(_: &mut Handle<ClientHandler>, _: &SshTarget, _: &SshContext) -> bool {
    false
}

async fn auth_with_key(
    handle: &mut Handle<ClientHandler>,
    target: &SshTarget,
    key: PrivateKey,
) -> anyhow::Result<bool> {
    let hash = rsa_hash(handle, key.algorithm().is_rsa()).await;
    let result = handle
        .authenticate_publickey(target.user.clone(), PrivateKeyWithHashAlg::new(Arc::new(key), hash))
        .await?;
    Ok(result.success())
}

/// Llave en archivo; si está cifrada, usa la passphrase recordada o la pide.
async fn try_key_file(
    handle: &mut Handle<ClientHandler>,
    target: &SshTarget,
    ctx: &SshContext,
    console: &Console,
    path: &Path,
) -> anyhow::Result<bool> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => {
            if target.key.as_deref() == Some(path) {
                console.print(&format!(
                    "{YELLOW}No se pudo leer la llave {}.{RESET}\n",
                    display_key(path, ctx.home.as_deref())
                ));
            }
            return Ok(false);
        }
    };
    match russh::keys::decode_secret_key(&text, None) {
        Ok(key) => return auth_with_key(handle, target, key).await,
        Err(russh::keys::Error::KeyIsEncrypted) => {}
        Err(e) => {
            console.print(&format!(
                "{YELLOW}La llave {} no se pudo leer: {e}{RESET}\n",
                display_key(path, ctx.home.as_deref())
            ));
            return Ok(false);
        }
    }

    let account = passphrase_account(path);
    if target.remember
        && let Some(saved) = ctx.secrets.get(&account).map(Zeroizing::new)
    {
        match russh::keys::decode_secret_key(&text, Some(&saved)) {
            Ok(key) => return auth_with_key(handle, target, key).await,
            Err(_) => ctx.secrets.delete(&account),
        }
    }
    let shown = display_key(path, ctx.home.as_deref());
    for _ in 0..TRIES {
        // Se borra de la memoria al terminar cada intento (ajustes v0.2.2).
        let Some(pass) = console
            .ask(&format!("Passphrase de la llave {shown}: "), true)
            .await
            .map(Zeroizing::new)
        else {
            return Ok(false);
        };
        if pass.is_empty() {
            return Ok(false);
        }
        match russh::keys::decode_secret_key(&text, Some(&pass)) {
            Ok(key) => {
                if target.remember {
                    remember(ctx, console, &account, &pass);
                }
                return auth_with_key(handle, target, key).await;
            }
            Err(_) => console.print(&format!("{RED}Passphrase incorrecta.{RESET}\n")),
        }
    }
    Ok(false)
}

fn remember(ctx: &SshContext, console: &Console, account: &str, secret: &str) {
    match ctx.secrets.set(account, secret) {
        Ok(()) => console.print(&format!(
            "{DIM}Guardada en el Administrador de credenciales de Windows.{RESET}\n"
        )),
        Err(e) => console.print(&format!("{YELLOW}No se pudo recordar: {e}{RESET}\n")),
    }
}

/// Contraseña (método `password` y, si el servidor solo acepta ese, `keyboard-interactive`).
async fn try_password(
    handle: &mut Handle<ClientHandler>,
    target: &SshTarget,
    ctx: &SshContext,
    console: &Console,
) -> anyhow::Result<bool> {
    let account = password_account(target);
    if target.remember
        && let Some(saved) = ctx.secrets.get(&account).map(Zeroizing::new)
    {
        if password_attempt(handle, target, &saved).await? {
            return Ok(true);
        }
        ctx.secrets.delete(&account);
    }
    for _ in 0..TRIES {
        let Some(pass) = console
            .ask(&format!("Contraseña de {}: ", target.destination()), true)
            .await
            .map(Zeroizing::new)
        else {
            anyhow::bail!("autenticación cancelada");
        };
        if password_attempt(handle, target, &pass).await? {
            if target.remember {
                remember(ctx, console, &account, &pass);
            }
            return Ok(true);
        }
        console.print(&format!("{RED}Contraseña incorrecta.{RESET}\n"));
    }
    Ok(false)
}

async fn password_attempt(handle: &mut Handle<ClientHandler>, target: &SshTarget, pass: &str) -> anyhow::Result<bool> {
    match handle.authenticate_password(target.user.clone(), pass).await? {
        AuthResult::Success => Ok(true),
        AuthResult::Failure { remaining_methods, .. } => {
            if !remaining_methods.contains(&MethodKind::KeyboardInteractive) {
                return Ok(false);
            }
            // keyboard-interactive con una sola pregunta oculta = la contraseña.
            let mut resp = handle
                .authenticate_keyboard_interactive_start(target.user.clone(), None)
                .await?;
            for _ in 0..4 {
                match resp {
                    KeyboardInteractiveAuthResponse::Success => return Ok(true),
                    KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
                    KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                        let answers = prompts
                            .iter()
                            .map(|p| if p.echo { String::new() } else { pass.to_string() })
                            .collect();
                        resp = handle.authenticate_keyboard_interactive_respond(answers).await?;
                    }
                }
            }
            Ok(false)
        }
    }
}
