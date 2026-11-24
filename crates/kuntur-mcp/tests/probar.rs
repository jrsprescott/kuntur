//! "Probar" contra servidores MCP **falsos**: el binario `kuntur-mcp-falso` (stdio) y un servidor
//! http mínimo escrito aquí (respuesta JSON y SSE con *chunked*). Nunca un servidor real.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use kuntur_mcp::{ProbeStatus, ResolvedKind, ResolvedServer, probe};

const FALSO: &str = env!("CARGO_BIN_EXE_kuntur-mcp-falso");

fn stdio(args: &[&str], env: &[(&str, &str)]) -> ResolvedServer {
    ResolvedServer {
        name: "falso".into(),
        kind: ResolvedKind::Stdio {
            command: FALSO.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            env: env.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            secret_env: BTreeMap::new(),
        },
    }
}

#[test]
fn stdio_conectado() {
    match probe(&stdio(&[], &[]), Duration::from_secs(10)) {
        ProbeStatus::Connected(info) => {
            assert!(info.starts_with("kuntur-mcp-falso "), "{info}");
            assert!(info.contains("MCP 2025-06-18"), "{info}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn stdio_ignora_ruido_y_escribe_captura() {
    let dir = std::env::temp_dir().join(format!("kuntur-mcp-captura-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let d = dir.to_string_lossy().into_owned();
    let status = probe(
        &stdio(&["--ruido", "--captura"], &[("KUNTUR_MCP_SALIDA", &d)]),
        Duration::from_secs(10),
    );
    assert!(matches!(status, ProbeStatus::Connected(_)), "{status:?}");
    assert!(dir.join("captura-falsa.svg").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stdio_error_del_servidor() {
    match probe(&stdio(&["--modo", "error"], &[]), Duration::from_secs(10)) {
        ProbeStatus::Error(e) => assert_eq!(e, "el servidor respondió: configuración falsa inválida"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn stdio_proceso_que_se_cae_muestra_stderr() {
    match probe(&stdio(&["--modo", "cae"], &[]), Duration::from_secs(10)) {
        ProbeStatus::Error(e) => {
            assert!(e.contains("código 3"), "{e}");
            assert!(e.contains("falla simulada"), "{e}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn stdio_tiempo_limite() {
    let start = Instant::now();
    match probe(&stdio(&["--modo", "mudo"], &[]), Duration::from_secs(1)) {
        ProbeStatus::Error(e) => assert_eq!(e, "sin respuesta en 1 s"),
        other => panic!("{other:?}"),
    }
    assert!(start.elapsed() < Duration::from_secs(8), "no se quedó colgado");
}

/// Servidor http de una sola conexión; devuelve la URL y el hilo (con la petición recibida).
fn http_once(reply: String) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut req = Vec::new();
        let mut buf = [0u8; 4096];
        // Encabezados + cuerpo (Content-Length).
        loop {
            let n = s.read(&mut buf).unwrap();
            req.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&req).to_string();
            if let Some(end) = text.find("\r\n\r\n") {
                let len: usize = text
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                if req.len() >= end + 4 + len {
                    break;
                }
            }
            if n == 0 {
                break;
            }
        }
        s.write_all(reply.as_bytes()).unwrap();
        s.flush().unwrap();
        // SSE: el flujo queda abierto un rato (el cliente no debe esperar al cierre).
        std::thread::sleep(Duration::from_millis(1500));
        String::from_utf8_lossy(&req).into_owned()
    });
    (url, handle)
}

fn http_server(url: String, token: Option<&str>) -> ResolvedServer {
    ResolvedServer {
        name: "web".into(),
        kind: ResolvedKind::Http {
            url,
            headers: [("X-Prueba".to_string(), "si".to_string())].into(),
            token: token.map(String::from),
        },
    }
}

const INIT_OK: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","serverInfo":{"name":"http-falso","version":"2.0"}}}"#;

#[test]
fn http_json() {
    let reply = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{INIT_OK}",
        INIT_OK.len()
    );
    let (url, h) = http_once(reply);
    let status = probe(&http_server(url, Some("tok")), Duration::from_secs(10));
    assert_eq!(status, ProbeStatus::Connected("http-falso 2.0 (MCP 2025-06-18)".into()));
    let req = h.join().unwrap();
    assert!(req.starts_with("POST /mcp HTTP/1.1"));
    assert!(req.contains("Authorization: Bearer tok"));
    assert!(req.contains("X-Prueba: si"));
    assert!(req.contains("Accept: application/json, text/event-stream"));
    assert!(req.contains(r#""method":"initialize""#));
}

#[test]
fn http_sse_chunked_sin_esperar_el_cierre() {
    let event = format!("event: message\r\ndata: {INIT_OK}\r\n\r\n");
    let reply = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{event}\r\n",
        event.len()
    );
    let (url, h) = http_once(reply);
    let start = Instant::now();
    let status = probe(&http_server(url, None), Duration::from_secs(10));
    assert!(matches!(status, ProbeStatus::Connected(_)), "{status:?}");
    assert!(
        start.elapsed() < Duration::from_millis(1400),
        "respondió antes del cierre"
    );
    h.join().unwrap();
}

#[test]
fn http_no_autorizado() {
    let (url, h) = http_once("HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".into());
    match probe(&http_server(url, None), Duration::from_secs(10)) {
        ProbeStatus::Error(e) => assert!(e.starts_with("HTTP 401"), "{e}"),
        other => panic!("{other:?}"),
    }
    h.join().unwrap();
}

#[test]
fn http_sin_servidor() {
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    match probe(
        &http_server(format!("http://127.0.0.1:{port}/mcp"), None),
        Duration::from_secs(3),
    ) {
        ProbeStatus::Error(e) => assert!(e.starts_with("no se pudo conectar"), "{e}"),
        other => panic!("{other:?}"),
    }
}
