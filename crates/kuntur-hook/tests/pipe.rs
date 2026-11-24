//! Integración real del named pipe: servidor y cliente en el mismo proceso.
#![cfg(windows)]

use std::sync::mpsc;
use std::time::{Duration, Instant};

use kuntur_hook::{EventKind, HookEvent, PipeServer, decode, pipe_name, send};

fn unique(tag: &str) -> String {
    format!("{}-{tag}-prueba", pipe_name(std::process::id()))
}

fn ev(pane: &str, kind: EventKind) -> HookEvent {
    HookEvent {
        pane: pane.into(),
        kind,
        ts: kuntur_hook::event::now_micros(),
        agent: Some("claude".into()),
        ..HookEvent::default()
    }
}

#[test]
fn entrega_mensajes_y_el_sello_recupera_el_orden() {
    let name = unique("orden");
    let (tx, rx) = mpsc::channel();
    let server = PipeServer::start(&name, move |m| tx.send(m).unwrap()).unwrap();
    assert_eq!(server.name(), name);
    for kind in [EventKind::Prompt, EventKind::PreTool, EventKind::Stop] {
        send(&name, &ev("k1", kind).encode()).unwrap();
    }
    // Varias instancias escuchan a la vez: pueden llegar desordenados; el sello `ts` los ordena.
    let mut got: Vec<HookEvent> = (0..3)
        .map(|_| decode(&rx.recv_timeout(Duration::from_secs(5)).unwrap()).remove(0))
        .collect();
    got.sort_by_key(|e| e.ts);
    let got: Vec<EventKind> = got.iter().map(|e| e.kind).collect();
    assert_eq!(got, [EventKind::Prompt, EventKind::PreTool, EventKind::Stop]);
    drop(server);
}

#[test]
fn varios_clientes_a_la_vez() {
    let name = unique("varios");
    let (tx, rx) = mpsc::channel();
    let _server = PipeServer::start(&name, move |m| tx.send(m).unwrap()).unwrap();
    let threads: Vec<_> = (0..24)
        .map(|i| {
            let name = name.clone();
            std::thread::spawn(move || send(&name, &ev(&format!("k{i}"), EventKind::PreTool).encode()))
        })
        .collect();
    let ok = threads
        .into_iter()
        .map(|t| t.join())
        .filter(|r| matches!(r, Ok(Ok(()))))
        .count();
    assert_eq!(ok, 24, "ningún gancho debe perder su mensaje");
    let mut panes: Vec<String> = (0..24)
        .map(|_| {
            decode(&rx.recv_timeout(Duration::from_secs(5)).unwrap())[0]
                .pane
                .clone()
        })
        .collect();
    panes.sort();
    let mut want: Vec<String> = (0..24).map(|i| format!("k{i}")).collect();
    want.sort();
    assert_eq!(panes, want);
}

#[test]
fn sin_kuntur_vuelve_enseguida() {
    let start = Instant::now();
    assert!(send(&unique("nadie"), b"{}\n").is_err());
    assert!(start.elapsed() < Duration::from_millis(500));
}

#[test]
fn el_nombre_no_se_puede_robar_y_se_libera_al_cerrar() {
    let name = unique("dueno");
    let first = PipeServer::start(&name, |_| {}).unwrap();
    assert!(
        PipeServer::start(&name, |_| {}).is_err(),
        "la primera instancia es exclusiva"
    );
    drop(first);
    let (tx, rx) = mpsc::channel();
    let _again = PipeServer::start(&name, move |m| tx.send(m).unwrap()).unwrap();
    send(&name, &ev("k", EventKind::Stop).encode()).unwrap();
    assert_eq!(decode(&rx.recv_timeout(Duration::from_secs(5)).unwrap()).len(), 1);
}

/// El binario real: con stdin de un hook, manda el evento y no escribe nada.
#[test]
fn el_binario_reenvia_y_calla() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let name = unique("binario");
    let (tx, rx) = mpsc::channel();
    let _server = PipeServer::start(&name, move |m| tx.send(m).unwrap()).unwrap();
    let run = |args: &[&str], env: bool, input: &str| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kuntur-hook"));
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_remove("KUNTUR_PIPE")
            .env_remove("KUNTUR_SESSION");
        if env {
            cmd.env("KUNTUR_PIPE", &name).env("KUNTUR_SESSION", "k7");
        }
        let mut child = cmd.spawn().unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        child.wait_with_output().unwrap()
    };
    let hook = r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"npm test"}}"#;
    let out = run(&[], true, hook);
    assert!(out.status.success());
    assert!(out.stdout.is_empty() && out.stderr.is_empty());
    let e = &decode(&rx.recv_timeout(Duration::from_secs(5)).unwrap())[0];
    assert_eq!((e.pane.as_str(), e.kind), ("k7", EventKind::PreTool));
    assert_eq!(e.detail.as_deref(), Some("npm test"));

    // Fuera de Kuntur, con basura o sin nada: sale con 0 y en silencio.
    for (env, input) in [(false, hook), (true, "no es json"), (true, "")] {
        let out = run(&[], env, input);
        assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
    }

    // Línea de estado: imprime su texto y manda los límites.
    let status = r#"{"model":{"display_name":"Opus"},"rate_limits":{"five_hour":{"used_percentage":40}}}"#;
    let out = run(&["statusline"], true, status);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Opus · 5 h 40 %");
    let e = &decode(&rx.recv_timeout(Duration::from_secs(5)).unwrap())[0];
    assert_eq!(e.kind, EventKind::StatusLine);
    assert_eq!(e.limits.as_ref().unwrap().five_hour_pct, Some(40.0));
    // Nada más llegó al pipe (las ejecuciones sin Kuntur o con basura no mandan).
    assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
}
