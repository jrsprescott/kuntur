//! Servidor MCP **falso** (stdio) para las pruebas de integración y las pruebas de humo de Kuntur.
//! Nunca se usa con datos reales.
//!
//! `kuntur-mcp-falso [--modo ok|error|cae|mudo] [--ruido] [--captura]`
//! - `ok` (por defecto): responde `initialize` (y `tools/list` vacío) hasta que se cierre stdin.
//! - `error`: responde `initialize` con un error JSON-RPC.
//! - `cae`: escribe un mensaje en stderr y termina con código 3 sin responder.
//! - `mudo`: lee pero nunca responde (para el tiempo límite).
//! - `--ruido`: antes, una línea que no es JSON en stdout (servidores que registran mal).
//! - `--captura`: al inicializar, deja `captura-falsa.svg` en la carpeta `KUNTUR_MCP_SALIDA`.

use std::io::{BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args
        .iter()
        .position(|a| a == "--modo")
        .and_then(|i| args.get(i + 1))
        .map_or("ok", String::as_str)
        .to_string();
    let noise = args.iter().any(|a| a == "--ruido");
    let capture = args.iter().any(|a| a == "--captura");

    if mode == "cae" {
        eprintln!("falla simulada del servidor falso");
        std::process::exit(3);
    }
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    if noise {
        let _ = writeln!(out, "arrancando servidor falso...");
        let _ = out.flush();
    }
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let Some(id) = msg.get("id").cloned() else {
            continue; // notificación
        };
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let reply = match (mode.as_str(), method) {
            ("mudo", _) => continue,
            ("error", "initialize") => serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32603, "message": "configuración falsa inválida" }
            }),
            (_, "initialize") => {
                if capture && let Some(dir) = std::env::var_os("KUNTUR_MCP_SALIDA") {
                    let dir = std::path::PathBuf::from(dir);
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::fs::write(dir.join("captura-falsa.svg"), SVG);
                }
                serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {
                        "protocolVersion": "2025-06-18",
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "kuntur-mcp-falso", "version": env!("CARGO_PKG_VERSION") }
                    }
                })
            }
            (_, "tools/list") => serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": [] } }),
            _ => serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32601, "message": "método no soportado" }
            }),
        };
        let _ = writeln!(out, "{reply}");
        let _ = out.flush();
    }
}

/// Captura sintética (una "escena" simple).
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="480" height="270" viewBox="0 0 480 270">
<rect width="480" height="270" fill="#1d2433"/>
<rect x="0" y="190" width="480" height="80" fill="#2f6f4f"/>
<circle cx="380" cy="70" r="34" fill="#f2c14e"/>
<polygon points="60,190 150,80 240,190" fill="#5a6b8c"/>
<polygon points="170,190 270,60 370,190" fill="#7a8bb0"/>
<rect x="200" y="150" width="40" height="40" fill="#d97757"/>
<text x="16" y="28" font-family="Segoe UI, sans-serif" font-size="16" fill="#e6e6e6">Captura sintética · kuntur-mcp-falso</text>
</svg>
"##;
