//! Integración SSH sin servidores externos: un servidor `russh` **en proceso** (127.0.0.1, puerto
//! efímero, llaves generadas en la prueba) con un shell simulado (eco), un `exec` y un subsistema
//! SFTP sobre una carpeta temporal (F7). Las pruebas usan una carpeta temporal como "home": nunca
//! leen el `~/.ssh` real ni el Administrador de credenciales (los secretos van a `MemorySecrets`).

mod sftp_servidor;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use kuntur_ssh::{KnownHosts, LinkState, MemorySecrets, RemoteSession, SecretStore, SshContext, SshTarget, runtime};
use kuntur_term::{GridSize, Pane};
use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey, PublicKey};
use russh::server::{self, Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId};

const USER: &str = "kuntur";
const PASSWORD: &str = "secreto-de-prueba";

/// Lo que el servidor de prueba vio.
#[derive(Default)]
struct Seen {
    auths: AtomicUsize,
    channels: AtomicUsize,
    sftp: AtomicUsize,
    pty: Mutex<Vec<(String, u32, u32)>>,
    resizes: Mutex<Vec<(u32, u32)>>,
    execs: Mutex<Vec<String>>,
    handles: Mutex<Vec<server::Handle>>,
}

#[derive(Clone)]
struct TestServer {
    seen: Arc<Seen>,
    allowed_key: Option<PublicKey>,
    /// Carpeta que el subsistema SFTP sirve como `/` (`None` = sin SFTP).
    sftp_root: Option<PathBuf>,
    /// Canales recién abiertos de esta conexión, hasta saber si son shell, `exec` o SFTP.
    channels: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
    /// Canales SFTP: sus datos los lee el subsistema, no el shell simulado.
    sftp_channels: Arc<Mutex<Vec<ChannelId>>>,
}

impl server::Server for TestServer {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        Self {
            channels: Arc::default(),
            sftp_channels: Arc::default(),
            ..self.clone()
        }
    }
}

impl TestServer {
    fn prompt(session: &mut Session, channel: ChannelId, text: &str) {
        let _ = session.data(channel, text.as_bytes().to_vec());
    }
}

impl server::Handler for TestServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if user == USER && password == PASSWORD {
            self.seen.auths.fetch_add(1, Ordering::SeqCst);
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn auth_publickey(&mut self, user: &str, key: server::VerifiedPublicKey<'_>) -> Result<Auth, Self::Error> {
        if user == USER && self.allowed_key.as_ref() == Some(key.public_key) {
            self.seen.auths.fetch_add(1, Ordering::SeqCst);
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.seen.channels.fetch_add(1, Ordering::SeqCst);
        self.channels.lock().unwrap().insert(channel.id(), channel);
        self.seen.handles.lock().unwrap().push(session.handle());
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        term: &str,
        cols: u32,
        rows: u32,
        _: u32,
        _: u32,
        _: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.seen.pty.lock().unwrap().push((term.to_string(), cols, rows));
        session.channel_success(channel)?;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let stream = self.channels.lock().unwrap().remove(&channel);
        match (name, &self.sftp_root, stream) {
            ("sftp", Some(root), Some(ch)) => {
                self.seen.sftp.fetch_add(1, Ordering::SeqCst);
                self.sftp_channels.lock().unwrap().push(channel);
                session.channel_success(channel)?;
                russh_sftp::server::run(ch.into_stream(), sftp_servidor::SftpServer::new(root.clone())).await;
            }
            _ => session.channel_failure(channel)?,
        }
        Ok(())
    }

    async fn shell_request(&mut self, channel: ChannelId, session: &mut Session) -> Result<(), Self::Error> {
        // Shell simulado: se maneja con `data`, sin el objeto del canal.
        self.channels.lock().unwrap().remove(&channel);
        session.channel_success(channel)?;
        Self::prompt(session, channel, "bienvenido\r\n$ ");
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let cmd = String::from_utf8_lossy(data).into_owned();
        self.channels.lock().unwrap().remove(&channel);
        self.seen.execs.lock().unwrap().push(cmd.clone());
        session.channel_success(channel)?;
        if cmd.contains("kuntur-agente") {
            Self::prompt(
                session,
                channel,
                "motd del servidor\nkuntur-agente:codex\nkuntur-agente:claude\n",
            );
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
        } else if let Some(rest) = cmd.strip_prefix("cd -- '") {
            // Shell en una carpeta (terminal acoplada o reconexión).
            let dir = rest.split('\'').next().unwrap_or_default().to_string();
            Self::prompt(session, channel, &format!("shell-en:{dir}\r\n$ "));
        }
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _: ChannelId,
        cols: u32,
        rows: u32,
        _: u32,
        _: u32,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.seen.resizes.lock().unwrap().push((cols, rows));
        Ok(())
    }

    async fn data(&mut self, channel: ChannelId, data: &[u8], session: &mut Session) -> Result<(), Self::Error> {
        if self.sftp_channels.lock().unwrap().contains(&channel) {
            return Ok(());
        }
        if data.starts_with(b"exit") {
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
            return Ok(());
        }
        // Eco como un shell: lo escrito y, con Enter, un "prompt" nuevo. "cd X" informa OSC 7.
        let text = String::from_utf8_lossy(data).replace('\r', "\r\n");
        Self::prompt(session, channel, &text);
        if let Some(dir) = String::from_utf8_lossy(data).trim().strip_prefix("cd ") {
            Self::prompt(session, channel, &format!("\x1b]7;file://servidor{dir}\x07"));
        }
        if data.contains(&b'\r') {
            Self::prompt(session, channel, "$ ");
        }
        Ok(())
    }
}

struct Running {
    port: u16,
    seen: Arc<Seen>,
    host_key: PrivateKey,
}

fn new_key() -> PrivateKey {
    PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap()
}

fn start_server(allowed_key: Option<PublicKey>) -> Running {
    start_server_with(allowed_key, None)
}

fn start_server_with(allowed_key: Option<PublicKey>, sftp_root: Option<PathBuf>) -> Running {
    let host_key = new_key();
    let config = Arc::new(server::Config {
        keys: vec![host_key.clone()],
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::ZERO),
        inactivity_timeout: Some(Duration::from_secs(120)),
        ..Default::default()
    });
    let seen = Arc::new(Seen::default());
    let mut srv = TestServer {
        seen: seen.clone(),
        allowed_key,
        sftp_root,
        channels: Arc::default(),
        sftp_channels: Arc::default(),
    };
    let listener = runtime()
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    runtime().spawn(async move {
        let _ = srv.run_on_socket(config, &listener).await;
    });
    Running { port, seen, host_key }
}

/// Carpeta temporal que hace de "home" (sin `.ssh`: no hay llaves por defecto).
fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kuntur-ssh-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn context(home: &Path, secrets: Arc<MemorySecrets>) -> SshContext {
    SshContext {
        known_hosts: KnownHosts::new(home.join("known_hosts"), vec![]),
        secrets,
        home: Some(home.to_path_buf()),
        agent_pipe: None,
    }
}

fn target(port: u16, key: Option<PathBuf>) -> SshTarget {
    SshTarget {
        host: "127.0.0.1".into(),
        port,
        user: USER.into(),
        key,
        remember: true,
    }
}

fn open(session: &Arc<RemoteSession>, cwd: Option<&str>) -> (Arc<kuntur_ssh::RemoteTerminal>, Pane) {
    let size = GridSize::new(80, 24);
    let (term, reader) = session.open_terminal(cwd.map(String::from), size);
    let pane = Pane::with_transport(term.clone(), Box::new(reader), size, || {}).unwrap();
    (term, pane)
}

fn wait(what: &str, cond: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if cond() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no llegó: {what}");
}

fn screen(pane: &Pane) -> String {
    pane.with_emulator(|e| (0..24).map(|r| e.line_text(r)).collect::<Vec<_>>().join("\n"))
}

fn wait_text(pane: &Pane, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if pane.with_emulator(|e| e.contains_text(needle)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no apareció «{needle}» en pantalla:\n{}", screen(pane));
}

#[test]
fn contrasena_huella_nueva_shell_y_segundo_canal_sin_reautenticar() {
    let srv = start_server(None);
    let home = tmp("contrasena");
    let secrets = Arc::new(MemorySecrets::new());
    let session = RemoteSession::new(target(srv.port, None), context(&home, secrets.clone()));
    let (term, pane) = open(&session, None);

    // Huella nueva: se muestra SHA256 y se pide confirmación.
    wait_text(&pane, "¿Confiar en este servidor");
    let fp = kuntur_ssh::fingerprint(srv.host_key.public_key());
    assert!(screen(&pane).contains(&fp), "la huella SHA256 se muestra");
    assert_eq!(term.state(), LinkState::Asking);
    pane.write(b"s\r").unwrap();

    // Sin agente ni llaves: contraseña, sin eco.
    wait_text(&pane, "Contraseña de kuntur@127.0.0.1");
    pane.write(PASSWORD.as_bytes()).unwrap();
    pane.write(b"\r").unwrap();
    wait_text(&pane, "bienvenido");
    assert!(!screen(&pane).contains(PASSWORD), "la contraseña no se ve");
    wait("conectado", || term.state() == LinkState::Connected);
    assert_eq!(srv.seen.pty.lock().unwrap()[0], ("xterm-256color".to_string(), 80, 24));

    // Eco, OSC 7 remoto y redimensionar.
    pane.write(b"cd /var/www\r").unwrap();
    wait_text(&pane, "cd /var/www");
    wait("carpeta remota", || term.cwd().as_deref() == Some("/var/www"));
    wait("carpeta en el panel", || pane.cwd() == Some(PathBuf::from("/var/www")));
    pane.resize(GridSize::new(100, 30)).unwrap();
    wait("redimensionar", || {
        srv.seen.resizes.lock().unwrap().contains(&(100, 30))
    });

    // Recordada (marcó "recordar") y huella guardada en el known_hosts propio.
    assert_eq!(
        secrets
            .get(&format!("contraseña:kuntur@127.0.0.1:{}", srv.port))
            .as_deref(),
        Some(PASSWORD)
    );
    let kh = std::fs::read_to_string(home.join("known_hosts")).unwrap();
    assert!(kh.contains(&format!("[127.0.0.1]:{}", srv.port)));

    // Agentes del servidor por `exec`.
    wait("agentes", || session.agents().is_some());
    assert_eq!(session.agents().unwrap(), vec!["claude", "codex"]);

    // Terminal acoplada: otro canal en la misma conexión, en la carpeta, sin volver a autenticar.
    let (dock_term, dock) = open(&session, Some("/var/www"));
    wait_text(&dock, "shell-en:/var/www");
    assert_eq!(dock_term.state(), LinkState::Connected);
    assert_eq!(srv.seen.auths.load(Ordering::SeqCst), 1, "una sola autenticación");
    assert!(!screen(&dock).contains("Contraseña"));

    // `exit` en la acoplada la termina; la principal sigue.
    dock.write(b"exit\r").unwrap();
    wait("fin de la acoplada", || dock.has_exited());
    assert!(!pane.has_exited());
    pane.write(b"exit\r").unwrap();
    wait("fin de la principal", || pane.has_exited());
}

#[test]
fn llave_cifrada_pide_passphrase_y_la_recuerda() {
    let user_key = new_key();
    let srv = start_server(Some(user_key.public_key().clone()));
    let home = tmp("llave");
    let key_path = home.join("id_prueba");
    let encrypted = user_key.encrypt(&mut rand::rng(), "frase-de-prueba").unwrap();
    std::fs::write(&key_path, encrypted.to_openssh(LineEnding::LF).unwrap().as_bytes()).unwrap();
    // La huella ya es conocida: no se pregunta.
    KnownHosts::new(home.join("known_hosts"), vec![])
        .learn("127.0.0.1", srv.port, srv.host_key.public_key())
        .unwrap();

    let secrets = Arc::new(MemorySecrets::new());
    let session = RemoteSession::new(
        target(srv.port, Some(key_path.clone())),
        context(&home, secrets.clone()),
    );
    let (_term, pane) = open(&session, None);
    wait_text(&pane, "Passphrase de la llave");
    assert!(!screen(&pane).contains("Confiar"));
    pane.write(b"equivocada\r").unwrap();
    wait_text(&pane, "Passphrase incorrecta");
    pane.write(b"frase-de-prueba\r").unwrap();
    wait_text(&pane, "bienvenido");
    assert_eq!(secrets.len(), 1, "solo la passphrase");

    // Otra sesión con la misma llave: usa la passphrase recordada, sin preguntar.
    let other = RemoteSession::new(target(srv.port, Some(key_path)), context(&home, secrets));
    let (_t2, pane2) = open(&other, None);
    wait_text(&pane2, "bienvenido");
    assert!(!screen(&pane2).contains("Passphrase"));
}

#[test]
fn huella_cambiada_bloquea_la_conexion() {
    let srv = start_server(None);
    let home = tmp("cambiada");
    // Se recuerda OTRA llave para este servidor.
    KnownHosts::new(home.join("known_hosts"), vec![])
        .learn("127.0.0.1", srv.port, new_key().public_key())
        .unwrap();
    let session = RemoteSession::new(target(srv.port, None), context(&home, Arc::new(MemorySecrets::new())));
    let (term, pane) = open(&session, None);
    wait_text(&pane, "CAMBIÓ");
    wait("desconectada", || term.state() == LinkState::Disconnected);
    assert!(term.last_error().unwrap().contains("huella"));
    assert_eq!(srv.seen.auths.load(Ordering::SeqCst), 0, "no se autentica");
    assert!(!pane.has_exited(), "la pestaña queda abierta con el aviso");
}

#[test]
fn rechazar_la_huella_nueva_no_conecta() {
    let srv = start_server(None);
    let home = tmp("rechazo");
    let session = RemoteSession::new(target(srv.port, None), context(&home, Arc::new(MemorySecrets::new())));
    let (term, pane) = open(&session, None);
    wait_text(&pane, "¿Confiar en este servidor");
    pane.write(b"n\r").unwrap();
    wait("desconectada", || term.state() == LinkState::Disconnected);
    wait_text(&pane, "no se confió en la huella");
    assert!(!home.join("known_hosts").exists());
}

#[test]
fn conexion_caida_avisa_y_enter_reconecta_en_la_misma_carpeta() {
    let srv = start_server(None);
    let home = tmp("caida");
    KnownHosts::new(home.join("known_hosts"), vec![])
        .learn("127.0.0.1", srv.port, srv.host_key.public_key())
        .unwrap();
    let secrets = Arc::new(MemorySecrets::new());
    secrets
        .set(&format!("contraseña:kuntur@127.0.0.1:{}", srv.port), PASSWORD)
        .unwrap();
    let session = RemoteSession::new(target(srv.port, None), context(&home, secrets));
    let (term, pane) = open(&session, None);
    wait_text(&pane, "bienvenido");
    assert!(!screen(&pane).contains("Contraseña"), "usa la recordada");
    pane.write(b"cd /srv/app\r").unwrap();
    wait("carpeta", || term.cwd().as_deref() == Some("/srv/app"));

    // El servidor corta la conexión.
    let handle = srv.seen.handles.lock().unwrap()[0].clone();
    runtime()
        .block_on(handle.disconnect(russh::Disconnect::ByApplication, "corte".into(), "es".into()))
        .unwrap();
    wait_text(&pane, "Conexión perdida");
    wait("desconectada", || term.state() == LinkState::Disconnected);
    assert!(!pane.has_exited(), "el historial sigue en la pestaña");
    assert!(pane.with_emulator(|e| e.contains_text("bienvenido")));

    // Enter reconecta (sin preguntar) y abre el shell en la última carpeta.
    pane.write(b"\r").unwrap();
    wait_text(&pane, "shell-en:/srv/app");
    wait("reconectada", || term.state() == LinkState::Connected);
    assert_eq!(srv.seen.auths.load(Ordering::SeqCst), 2);
}

#[test]
fn servidor_apagado_da_un_error_claro() {
    // Un puerto que nadie escucha.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let home = tmp("apagado");
    let session = RemoteSession::new(target(port, None), context(&home, Arc::new(MemorySecrets::new())));
    let (term, pane) = open(&session, None);
    wait("desconectada", || term.state() == LinkState::Disconnected);
    wait_text(&pane, "No se pudo conectar");
    assert!(
        term.last_error().unwrap().contains("rechazó"),
        "{:?}",
        term.last_error()
    );
    // Reintentar vuelve a fallar igual, sin colgarse.
    term.reconnect();
    wait("vuelve a fallar", || {
        term.state() == LinkState::Disconnected && screen(&pane).matches("No se pudo conectar").count() == 2
    });
}

// ── F7: SFTP sobre la misma conexión ────────────────────────────────────────

use kuntur_files::{Conflict, Direction, FileSystem, JobState, LocalFs, Transfers};

/// Pestaña remota ya conectada (huella conocida, contraseña recordada) a un servidor que sirve por
/// SFTP la carpeta temporal `root` (con `/home/kuntur` como carpeta personal).
fn connected_with_sftp(name: &str) -> (Running, PathBuf, Arc<RemoteSession>, Pane) {
    let root = tmp(&format!("{name}-raiz"));
    std::fs::create_dir_all(root.join("home").join("kuntur")).unwrap();
    let srv = start_server_with(None, Some(root.clone()));
    let home = tmp(name);
    KnownHosts::new(home.join("known_hosts"), vec![])
        .learn("127.0.0.1", srv.port, srv.host_key.public_key())
        .unwrap();
    let secrets = Arc::new(MemorySecrets::new());
    secrets
        .set(&format!("contraseña:kuntur@127.0.0.1:{}", srv.port), PASSWORD)
        .unwrap();
    let session = RemoteSession::new(target(srv.port, None), context(&home, secrets));
    let (term, pane) = open(&session, None);
    wait_text(&pane, "bienvenido");
    wait("conectado", || term.state() == LinkState::Connected);
    (srv, root, session, pane)
}

fn wait_job(t: &Transfers, id: u64, what: &str, cond: impl Fn(&JobState) -> bool) {
    wait(what, || cond(&t.job(id).unwrap().state));
}

#[test]
fn sftp_sin_conexion_da_un_error_claro() {
    let home = tmp("sftp-sin");
    let session = RemoteSession::new(target(1, None), context(&home, Arc::new(MemorySecrets::new())));
    let err = session.sftp().err().unwrap().to_string();
    assert!(err.contains("sin conexión"), "{err}");
}

#[test]
fn sftp_lista_opera_sube_descarga_y_resuelve_conflictos() {
    let (srv, root, session, _pane) = connected_with_sftp("sftp");
    let disk_home = root.join("home").join("kuntur");
    std::fs::create_dir_all(disk_home.join("proyecto")).unwrap();
    std::fs::write(disk_home.join("notas.md"), "hola").unwrap();

    let fs = session.sftp().unwrap();
    assert_eq!(fs.home().unwrap(), "/home/kuntur");
    assert_eq!(fs.remote_label(), Some(session.target().destination()));

    // Listar (carpetas primero, sin "." ni "..") y stat.
    let l = fs.list("/home/kuntur").unwrap();
    let names: Vec<_> = l.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["proyecto", "notas.md"]);
    assert_eq!(l.entries[1].size, 4);
    assert_eq!(l.entries[1].path, "/home/kuntur/notas.md");
    assert!(l.entries[1].modified.is_some());
    assert!(fs.stat("/home/kuntur/proyecto").unwrap().unwrap().is_dir);
    assert!(fs.stat("/home/kuntur/no-existe").unwrap().is_none());
    assert!(fs.list("/no/existe").is_err());

    // Leer, escribir, crear carpeta, renombrar y borrar.
    assert_eq!(fs.read("/home/kuntur/notas.md").unwrap(), b"hola");
    fs.create_dir("/home/kuntur/tmp").unwrap();
    fs.write("/home/kuntur/tmp/a.txt", b"contenido").unwrap();
    assert_eq!(
        std::fs::read(disk_home.join("tmp").join("a.txt")).unwrap(),
        b"contenido"
    );
    fs.rename("/home/kuntur/tmp/a.txt", "/home/kuntur/tmp/b.txt").unwrap();
    assert!(disk_home.join("tmp").join("b.txt").exists());
    fs.remove_all("/home/kuntur/tmp").unwrap();
    assert!(!disk_home.join("tmp").exists());

    // Misma conexión: el SFTP se reutiliza y no se volvió a autenticar.
    assert!(Arc::ptr_eq(&fs, &session.sftp().unwrap()));
    assert_eq!(srv.seen.auths.load(Ordering::SeqCst), 1);
    assert_eq!(srv.seen.sftp.load(Ordering::SeqCst), 1);

    // Subir (archivo de 300 KB, varios bloques) a una carpeta del árbol.
    let local_dir = tmp("sftp-local");
    let big = local_dir.join("datos.bin");
    let bytes: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(&big, &bytes).unwrap();
    let local: Arc<dyn FileSystem> = Arc::new(LocalFs);
    let remote: Arc<dyn FileSystem> = fs.clone();
    let t = Transfers::new();
    let big_s = big.to_string_lossy().into_owned();
    let up = t.enqueue(
        Direction::Upload,
        local.clone(),
        &big_s,
        remote.clone(),
        "/home/kuntur/proyecto",
    );
    wait_job(&t, up, "subida", |s| *s == JobState::Done);
    assert_eq!(t.job(up).unwrap().done, 300_000);
    assert_eq!(
        std::fs::read(disk_home.join("proyecto").join("datos.bin")).unwrap(),
        bytes
    );

    // Conflicto: el destino existe → pregunta; "Renombrar" sube como "datos (1).bin".
    let again = t.enqueue(
        Direction::Upload,
        local.clone(),
        &big_s,
        remote.clone(),
        "/home/kuntur/proyecto",
    );
    wait_job(&t, again, "pregunta", |s| *s == JobState::Asking);
    t.resolve(again, Conflict::Rename, false);
    wait_job(&t, again, "renombrada", |s| *s == JobState::Done);
    assert!(disk_home.join("proyecto").join("datos (1).bin").exists());

    // Carpeta temporal "para el agente": se crea sola.
    let agent = kuntur_files::agent_dir("Sesión 1");
    let to_agent = t.enqueue(Direction::Upload, local.clone(), &big_s, remote.clone(), &agent);
    wait_job(&t, to_agent, "carpeta del agente", |s| *s == JobState::Done);
    assert!(root.join("tmp").join("kuntur-sesion-1").join("datos.bin").exists());

    // Descargar una carpeta remota a una carpeta local.
    std::fs::write(disk_home.join("proyecto").join("leeme.txt"), "léeme").unwrap();
    let out = tmp("sftp-descarga");
    let out_s = out.to_string_lossy().into_owned();
    let down = t.enqueue(Direction::Download, remote, "/home/kuntur/proyecto", local, &out_s);
    wait_job(&t, down, "descarga", |s| *s == JobState::Done);
    let got = out.join("proyecto");
    assert_eq!(std::fs::read_to_string(got.join("leeme.txt")).unwrap(), "léeme");
    assert_eq!(std::fs::read(got.join("datos.bin")).unwrap(), bytes);
}

/// Disco local que lee de a poco: da tiempo a cancelar una subida a mitad de camino.
struct SlowLocal;

struct SlowReader(Box<dyn std::io::Read + Send>);

impl std::io::Read for SlowReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        std::thread::sleep(Duration::from_millis(10));
        let n = buf.len().min(16 * 1024);
        self.0.read(&mut buf[..n])
    }
}

impl FileSystem for SlowLocal {
    fn style(&self) -> kuntur_files::PathStyle {
        LocalFs.style()
    }
    fn remote_label(&self) -> Option<String> {
        None
    }
    fn home(&self) -> std::io::Result<String> {
        LocalFs.home()
    }
    fn list(&self, dir: &str) -> std::io::Result<kuntur_files::Listing> {
        LocalFs.list(dir)
    }
    fn stat(&self, path: &str) -> std::io::Result<Option<kuntur_files::FileEntry>> {
        LocalFs.stat(path)
    }
    fn open_read(&self, path: &str) -> std::io::Result<Box<dyn std::io::Read + Send>> {
        Ok(Box::new(SlowReader(LocalFs.open_read(path)?)))
    }
    fn create(&self, path: &str) -> std::io::Result<Box<dyn std::io::Write + Send>> {
        LocalFs.create(path)
    }
    fn create_dir(&self, path: &str) -> std::io::Result<()> {
        LocalFs.create_dir(path)
    }
    fn rename(&self, from: &str, to: &str) -> std::io::Result<()> {
        LocalFs.rename(from, to)
    }
    fn remove_file(&self, path: &str) -> std::io::Result<()> {
        LocalFs.remove_file(path)
    }
    fn remove_dir(&self, path: &str) -> std::io::Result<()> {
        LocalFs.remove_dir(path)
    }
}

#[test]
fn sftp_cancelar_una_subida_no_deja_el_archivo_a_medias() {
    let (_srv, root, session, _pane) = connected_with_sftp("sftp-cancelar");
    let fs = session.sftp().unwrap();
    let local_dir = tmp("sftp-cancelar-local");
    let file = local_dir.join("grande.bin");
    std::fs::write(&file, vec![9u8; 3_000_000]).unwrap();
    let t = Transfers::new();
    let id = t.enqueue(
        Direction::Upload,
        Arc::new(SlowLocal),
        &file.to_string_lossy(),
        fs,
        "/home/kuntur",
    );
    wait("empezó", || t.job(id).unwrap().done > 0);
    t.cancel(id);
    wait_job(&t, id, "cancelada", |s| *s == JobState::Cancelled);
    assert!(t.job(id).unwrap().done < 3_000_000);
    assert!(
        !root.join("home").join("kuntur").join("grande.bin").exists(),
        "se borró lo subido a medias"
    );
}
