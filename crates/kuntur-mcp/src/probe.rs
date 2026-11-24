//! "Probar" un servidor MCP (§11): lanzarlo (stdio) o conectarse (http) y hacer el `initialize` del
//! protocolo (JSON-RPC 2.0) con tiempo límite. Los servidores MCP los lanza cada agente, así que
//! Kuntur no ve su estado directamente: este es el indicador de "conectado / error / detenido".

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::catalog::{ResolvedKind, ResolvedServer};

/// Versión del protocolo que se pide (el servidor puede contestar otra).
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Estado que muestra el gestor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeStatus {
    /// Nunca se probó (o se apagó en la sesión).
    Stopped,
    Testing,
    /// Respondió el `initialize`: nombre y versión que dice el servidor.
    Connected(String),
    Error(String),
}

impl ProbeStatus {
    pub fn label(&self) -> String {
        match self {
            ProbeStatus::Stopped => "detenido".into(),
            ProbeStatus::Testing => "probando…".into(),
            ProbeStatus::Connected(info) => format!("conectado · {info}"),
            ProbeStatus::Error(e) => format!("error · {e}"),
        }
    }
}

/// Mensaje `initialize` (una línea JSON).
pub fn initialize_request() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "kuntur", "version": env!("CARGO_PKG_VERSION") }
        }
    })
}

/// Si `msg` es la respuesta al `initialize` (id 1): `Ok(nombre versión)` o `Err(mensaje)`.
pub fn read_response(msg: &Value) -> Option<Result<String, String>> {
    if msg.get("id")? != &json!(1) {
        return None;
    }
    if let Some(err) = msg.get("error") {
        let text = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("error sin mensaje");
        return Some(Err(format!("el servidor respondió: {text}")));
    }
    let result = msg.get("result")?;
    let info = result.get("serverInfo");
    let name = info
        .and_then(|i| i.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("sin nombre");
    let version = info.and_then(|i| i.get("version")).and_then(Value::as_str);
    let protocol = result.get("protocolVersion").and_then(Value::as_str);
    let mut text = name.to_string();
    if let Some(v) = version {
        text.push(' ');
        text.push_str(v);
    }
    if let Some(p) = protocol {
        text.push_str(&format!(" (MCP {p})"));
    }
    Some(Ok(text))
}

/// Prueba el servidor (bloquea hasta `timeout`; llamarlo en segundo plano).
pub fn probe(server: &ResolvedServer, timeout: Duration) -> ProbeStatus {
    let result = match &server.kind {
        ResolvedKind::Stdio {
            command,
            args,
            env,
            secret_env,
        } => probe_stdio(command, args, env.iter().chain(secret_env.iter()), timeout),
        ResolvedKind::Http { url, headers, token } => {
            let mut h: Vec<(String, String)> = headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            if let Some(t) = token {
                h.push(("Authorization".into(), format!("Bearer {t}")));
            }
            probe_http(url, &h, timeout)
        }
    };
    match result {
        Ok(info) => ProbeStatus::Connected(info),
        Err(e) => ProbeStatus::Error(e),
    }
}

// ── stdio ───────────────────────────────────────────────────────────────────

/// Busca el programa en el PATH con las extensiones de `PATHEXT` (como hace cmd).
pub fn find_program(command: &str) -> Option<PathBuf> {
    let p = Path::new(command);
    let exts: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .filter(|e| !e.is_empty())
        .map(|e| e.to_ascii_lowercase())
        .collect();
    let with_ext = |base: &Path| -> Option<PathBuf> {
        if base.extension().is_some() && base.is_file() {
            return Some(base.to_path_buf());
        }
        exts.iter()
            .map(|e| PathBuf::from(format!("{}{e}", base.display())))
            .find(|c| c.is_file())
    };
    if p.components().count() > 1 || p.is_absolute() {
        return with_ext(p);
    }
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| with_ext(&dir.join(command)))
}

/// ¿Es un `.cmd` / `.bat`? (`npx`, `uvx.cmd`… en Windows): hay que lanzarlo con `cmd /c`.
pub fn is_batch(command: &str) -> bool {
    find_program(command)
        .and_then(|p| p.extension().map(|e| e.to_ascii_lowercase()))
        .is_some_and(|e| e == "cmd" || e == "bat")
}

fn spawn_command(command: &str, args: &[String]) -> Command {
    let program = find_program(command);
    let batch = program
        .as_ref()
        .and_then(|p| p.extension().map(|e| e.to_ascii_lowercase()))
        .is_some_and(|e| e == "cmd" || e == "bat");
    let mut cmd = if batch {
        let mut c = Command::new("cmd.exe");
        c.arg("/d").arg("/c").arg(program.as_ref().unwrap());
        c
    } else {
        Command::new(program.unwrap_or_else(|| PathBuf::from(command)))
    };
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Sin ventana de consola (Kuntur en release no tiene consola).
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Termina el servidor y sus hijos (`cmd /c npx …` deja a `node` debajo).
fn stop(child: &mut Child) {
    drop(child.stdin.take());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

enum Line {
    Out(String),
    Closed,
}

fn probe_stdio<'a>(
    command: &str,
    args: &[String],
    env: impl Iterator<Item = (&'a String, &'a String)>,
    timeout: Duration,
) -> Result<String, String> {
    let mut cmd = spawn_command(command, args);
    cmd.envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("no se pudo lanzar «{command}»: {e}"))?;
    let result = talk_stdio(&mut child, timeout);
    stop(&mut child);
    result
}

fn talk_stdio(child: &mut Child, timeout: Duration) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    let stdout = child.stdout.take().ok_or("sin stdout")?;
    let stderr = child.stderr.take().ok_or("sin stderr")?;
    let (tx, rx) = mpsc::channel::<Line>();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(Line::Closed);
                    return;
                }
                Ok(_) => {
                    if tx.send(Line::Out(line.trim().to_string())).is_err() {
                        return;
                    }
                }
            }
        }
    });
    // Últimas líneas de stderr: el mensaje de error si el servidor se cae.
    let err_tail = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let tail = err_tail.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let mut t = tail.lock().unwrap();
            if !line.trim().is_empty() {
                t.push(line.trim().to_string());
            }
            let n = t.len();
            if n > 3 {
                t.drain(..n - 3);
            }
        }
    });

    let mut stdin = child.stdin.take().ok_or("sin stdin")?;
    let mut msg = initialize_request().to_string();
    msg.push('\n');
    stdin
        .write_all(msg.as_bytes())
        .and_then(|_| stdin.flush())
        .map_err(|e| format!("no se pudo escribir al servidor: {e}"))?;

    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(format!("sin respuesta en {} s", timeout.as_secs()));
        }
        match rx.recv_timeout(left) {
            Ok(Line::Out(text)) => {
                // Las líneas que no son JSON-RPC (registros mal dirigidos a stdout) se ignoran.
                if let Ok(v) = serde_json::from_str::<Value>(&text)
                    && let Some(r) = read_response(&v)
                {
                    child.stdin = Some(stdin);
                    return r;
                }
            }
            Ok(Line::Closed) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                // Un momento para que stderr termine de llegar.
                std::thread::sleep(Duration::from_millis(150));
                let code = child.try_wait().ok().flatten().and_then(|s| s.code());
                let tail = err_tail.lock().unwrap().join(" · ");
                let mut text = match code {
                    Some(c) => format!("el proceso terminó (código {c})"),
                    None => "el proceso cerró su salida".to_string(),
                };
                if !tail.is_empty() {
                    text.push_str(": ");
                    text.push_str(&tail);
                }
                return Err(text);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(format!("sin respuesta en {} s", timeout.as_secs()));
            }
        }
    }
}

// ── http ────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
struct Url {
    tls: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Result<Url, String> {
    let (tls, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err("la URL debe empezar con http:// o https://".into());
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let default_port = if tls { 443 } else { 80 };
    let port_of = |p: &str| p.parse::<u16>().map_err(|_| "puerto no válido".to_string());
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (h, after) = v6.split_once(']').ok_or("dirección IPv6 no válida")?;
        match after.strip_prefix(':') {
            Some(p) => (h.to_string(), port_of(p)?),
            None => (h.to_string(), default_port),
        }
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), port_of(p)?),
            None => (authority.to_string(), default_port),
        }
    };
    if host.is_empty() {
        return Err("falta el host".into());
    }
    Ok(Url {
        tls,
        host,
        port,
        path: path.to_string(),
    })
}

/// Lee del cuerpo (ya sin el *chunked*) la respuesta al `initialize`: JSON o eventos SSE (`data:`).
fn find_in_body(body: &str, sse: bool) -> Option<Result<String, String>> {
    if !sse {
        return serde_json::from_str::<Value>(body.trim())
            .ok()
            .and_then(|v| read_response(&v));
    }
    // Un evento puede tener varias líneas `data:`; se juntan hasta la línea vacía.
    let mut data = String::new();
    for line in body.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(d) = line.strip_prefix("data:") {
            data.push_str(d.trim_start());
        } else if line.is_empty() && !data.is_empty() {
            if let Some(r) = serde_json::from_str::<Value>(&data)
                .ok()
                .and_then(|v| read_response(&v))
            {
                return Some(r);
            }
            data.clear();
        }
    }
    serde_json::from_str::<Value>(&data)
        .ok()
        .and_then(|v| read_response(&v))
}

/// Quita el *chunked* (`tamaño\r\n datos\r\n …`). Devuelve lo decodificado hasta ahora.
fn dechunk(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(end) = raw[i..].windows(2).position(|w| w == b"\r\n") {
        let size_text = String::from_utf8_lossy(&raw[i..i + end]);
        let Ok(size) = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16) else {
            break;
        };
        let start = i + end + 2;
        if size == 0 {
            break;
        }
        let stop = (start + size).min(raw.len());
        out.extend_from_slice(&raw[start..stop]);
        if start + size + 2 > raw.len() {
            break;
        }
        i = start + size + 2;
    }
    out
}

trait Stream: Read + Write {}
impl<T: Read + Write> Stream for T {}

fn connect(url: &Url, timeout: Duration) -> Result<Box<dyn Stream>, String> {
    let addr = (url.host.as_str(), url.port)
        .to_socket_addrs()
        .map_err(|e| format!("no se encontró {}: {e}", url.host))?
        .next()
        .ok_or_else(|| format!("no se encontró {}", url.host))?;
    let tcp = TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("no se pudo conectar: {e}"))?;
    tcp.set_read_timeout(Some(Duration::from_millis(200))).ok();
    tcp.set_write_timeout(Some(timeout)).ok();
    if !url.tls {
        return Ok(Box::new(tcp));
    }
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(url.host.clone()).map_err(|e| e.to_string())?;
    let conn = rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
    Ok(Box::new(rustls::StreamOwned::new(conn, tcp)))
}

fn probe_http(url: &str, headers: &[(String, String)], timeout: Duration) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    let u = parse_url(url)?;
    let mut stream = connect(&u, timeout)?;
    let body = initialize_request().to_string();
    let host = if (u.tls && u.port == 443) || (!u.tls && u.port == 80) {
        u.host.clone()
    } else {
        format!("{}:{}", u.host, u.port)
    };
    let mut req = format!(
        "POST {} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nMCP-Protocol-Version: {PROTOCOL_VERSION}\r\nUser-Agent: kuntur\r\nContent-Length: {}\r\nConnection: close\r\n",
        u.path,
        body.len()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(&body);
    stream
        .write_all(req.as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|e| format!("no se pudo enviar: {e}"))?;

    let mut raw: Vec<u8> = Vec::new();
    let mut buf = [0u8; 8192];
    let mut head: Option<(usize, u16, bool, bool)> = None; // (fin de encabezados, estado, sse, chunked)
    loop {
        if Instant::now() >= deadline {
            return Err(format!("sin respuesta en {} s", timeout.as_secs()));
        }
        let closed = match stream.read(&mut buf) {
            Ok(0) => true,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                false
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => false,
            Err(e) => return Err(format!("se cortó la conexión: {e}")),
        };
        if head.is_none()
            && let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n")
        {
            let text = String::from_utf8_lossy(&raw[..end]).to_ascii_lowercase();
            let status = text
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(0);
            if !(200..300).contains(&status) {
                return Err(match status {
                    401 | 403 => format!("HTTP {status}: falta el token o no es válido"),
                    0 => "respuesta HTTP no válida".to_string(),
                    s => format!("HTTP {s}"),
                });
            }
            let sse = text.contains("content-type: text/event-stream");
            let chunked = text.contains("transfer-encoding: chunked");
            head = Some((end + 4, status, sse, chunked));
        }
        if let Some((start, _, sse, chunked)) = head {
            let body = if chunked {
                dechunk(&raw[start..])
            } else {
                raw[start..].to_vec()
            };
            if let Some(r) = find_in_body(&String::from_utf8_lossy(&body), sse) {
                return r;
            }
        }
        if closed {
            return Err(if head.is_some() {
                "el servidor no respondió al initialize".into()
            } else {
                "el servidor cerró la conexión".into()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respuesta_al_initialize() {
        let ok = json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","serverInfo":{"name":"falso","version":"1.0"}}});
        assert_eq!(read_response(&ok), Some(Ok("falso 1.0 (MCP 2025-06-18)".into())));
        let err = json!({"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"no"}});
        assert_eq!(read_response(&err), Some(Err("el servidor respondió: no".into())));
        let other = json!({"jsonrpc":"2.0","method":"notifications/message"});
        assert_eq!(read_response(&other), None);
        assert_eq!(initialize_request()["method"], "initialize");
    }

    #[test]
    fn urls() {
        assert_eq!(
            parse_url("https://mcp.ejemplo.test/mcp").unwrap(),
            Url {
                tls: true,
                host: "mcp.ejemplo.test".into(),
                port: 443,
                path: "/mcp".into()
            }
        );
        let u = parse_url("http://127.0.0.1:8080").unwrap();
        assert_eq!((u.port, u.path.as_str(), u.tls), (8080, "/", false));
        assert!(parse_url("ftp://x").is_err());
        assert!(parse_url("http://:80/").is_err());
    }

    #[test]
    fn cuerpos_json_sse_y_chunked() {
        let msg = r#"{"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"s"}}}"#;
        assert_eq!(find_in_body(msg, false), Some(Ok("s".into())));
        let sse = format!("event: message\r\ndata: {msg}\r\n\r\n");
        assert_eq!(find_in_body(&sse, true), Some(Ok("s".into())));
        assert_eq!(find_in_body("event: ping\n\n", true), None);
        let chunked = format!("{:x}\r\n{sse}\r\n0\r\n\r\n", sse.len());
        assert_eq!(dechunk(chunked.as_bytes()), sse.as_bytes());
        // Incompleto: lo que haya llegado.
        assert_eq!(dechunk(b"5\r\nhol"), b"hol");
    }

    #[test]
    fn programa_inexistente_da_error() {
        let s = ResolvedServer {
            name: "x".into(),
            kind: ResolvedKind::Stdio {
                command: "kuntur-no-existe-este-programa".into(),
                args: vec![],
                env: Default::default(),
                secret_env: Default::default(),
            },
        };
        match probe(&s, Duration::from_secs(2)) {
            ProbeStatus::Error(e) => assert!(e.contains("no se pudo lanzar"), "{e}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(ProbeStatus::Stopped.label(), "detenido");
    }
}
