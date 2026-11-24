//! IA local (ajustes v0.2.1): proveedores de modelos en esta PC (LM Studio, Ollama, Jan, llama.cpp),
//! sus modelos y la configuración para usarlos como agente con OpenCode. Sin UI.
//!
//! Privacidad y costo: solo se consulta `127.0.0.1` (conexión TCP con un tiempo corto y una
//! petición HTTP/1.0 mínima) y el disco. La lectura automática **nunca** ejecuta `lms ls`, que
//! arranca el servicio de LM Studio: con el servidor apagado, los modelos se leen de su carpeta.

use std::collections::{BTreeMap, HashSet};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::launch::{ShellFlavor, launch_line};

/// Proveedor de modelos locales.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LocalProvider {
    LmStudio,
    Ollama,
    Jan,
    LlamaCpp,
}

impl LocalProvider {
    pub const ALL: [LocalProvider; 4] = [
        LocalProvider::LmStudio,
        LocalProvider::Ollama,
        LocalProvider::Jan,
        LocalProvider::LlamaCpp,
    ];

    /// Id estable (también la clave del proveedor en la configuración de OpenCode).
    pub fn id(self) -> &'static str {
        match self {
            LocalProvider::LmStudio => "lmstudio",
            LocalProvider::Ollama => "ollama",
            LocalProvider::Jan => "jan",
            LocalProvider::LlamaCpp => "llamacpp",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LocalProvider::LmStudio => "LM Studio",
            LocalProvider::Ollama => "Ollama",
            LocalProvider::Jan => "Jan",
            LocalProvider::LlamaCpp => "llama.cpp",
        }
    }

    /// Letra y color de la insignia (hasta que el hilo principal conecte los íconos de marca).
    pub fn letter(self) -> char {
        match self {
            LocalProvider::LmStudio => 'L',
            LocalProvider::Ollama => 'O',
            LocalProvider::Jan => 'J',
            LocalProvider::LlamaCpp => 'C',
        }
    }

    pub fn color(self) -> u32 {
        match self {
            LocalProvider::LmStudio => 0x6E56CF,
            LocalProvider::Ollama => 0x9CA3AF,
            LocalProvider::Jan => 0x3B82F6,
            LocalProvider::LlamaCpp => 0xE11D48,
        }
    }

    /// Comando de su CLI (lo que se busca en el PATH). Jan no tiene: se detecta por su servidor.
    pub fn command(self) -> Option<&'static str> {
        match self {
            LocalProvider::LmStudio => Some("lms"),
            LocalProvider::Ollama => Some("ollama"),
            LocalProvider::Jan => None,
            LocalProvider::LlamaCpp => Some("llama-server"),
        }
    }

    /// Puerto por defecto de su servidor (siempre en `127.0.0.1`).
    pub fn port(self) -> u16 {
        match self {
            LocalProvider::LmStudio => 1234,
            LocalProvider::Ollama => 11434,
            LocalProvider::Jan => 1337,
            LocalProvider::LlamaCpp => 8080,
        }
    }

    /// Dirección compatible con la API de OpenAI (la que usa OpenCode).
    pub fn base_url(self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port())
    }

    /// Chat rápido en la terminal (sin Enter: lo decide el usuario). `None` si no tiene CLI de chat.
    pub fn chat_line(self, model: &str, shell: ShellFlavor) -> Option<String> {
        let (cmd, args): (&str, Vec<String>) = match self {
            LocalProvider::LmStudio => ("lms", vec!["chat".into(), model.into()]),
            LocalProvider::Ollama => ("ollama", vec!["run".into(), model.into()]),
            LocalProvider::Jan | LocalProvider::LlamaCpp => return None,
        };
        Some(launch_line(cmd, &args, shell))
    }

    /// Programa y argumentos para iniciar su servidor en segundo plano. `None` si no se puede
    /// desde aquí (Jan se abre desde su aplicación; llama.cpp necesita elegir un modelo).
    pub fn start_server(self) -> Option<(&'static str, &'static [&'static str])> {
        match self {
            LocalProvider::LmStudio => Some(("lms", &["server", "start"])),
            LocalProvider::Ollama => Some(("ollama", &["serve"])),
            LocalProvider::Jan | LocalProvider::LlamaCpp => None,
        }
    }
}

/// Un modelo de un proveedor local.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalModel {
    pub provider: LocalProvider,
    /// Lo que se le pasa al servidor (`model`) y a `lms chat` / `ollama run`.
    pub id: String,
    /// Nombre para mostrar.
    pub name: String,
    pub size: Option<u64>,
    /// Cargado en memoria (solo se sabe con el servidor encendido).
    pub loaded: bool,
    /// LM Studio, leído del disco: ruta relativa del archivo (para encontrar su `modelKey` real).
    pub path: Option<String>,
}

/// Lo que se sabe de un proveedor en una lectura.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderStatus {
    pub provider: LocalProvider,
    /// Su CLI está en el PATH (o, para Jan, su servidor responde).
    pub installed: bool,
    pub server: bool,
    pub models: Vec<LocalModel>,
}

// ── Análisis de las listas de modelos ────────────────────────────────────────

/// `lms ls --json`: solo los LLM (no los de embeddings). Id = `modelKey`.
pub fn parse_lms_ls_json(text: &str) -> Vec<LocalModel> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(text.trim()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|m| m["type"].as_str().is_none_or(|t| t == "llm" || t == "vlm"))
        .filter_map(|m| {
            let id = m["modelKey"].as_str()?.to_string();
            Some(LocalModel {
                provider: LocalProvider::LmStudio,
                name: m["displayName"].as_str().unwrap_or(&id).to_string(),
                size: m["sizeBytes"].as_u64(),
                loaded: false,
                path: m["path"].as_str().map(str::to_string),
                id,
            })
        })
        .collect()
}

/// `GET /v1/models` de cualquier servidor compatible con OpenAI: `{"data":[{"id":…}]}`.
pub fn parse_openai_models(provider: LocalProvider, body: &str) -> Vec<LocalModel> {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    v["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|m| m["id"].as_str())
                .filter(|id| !id.to_ascii_lowercase().contains("embed"))
                .map(|id| LocalModel {
                    provider,
                    id: id.to_string(),
                    name: id.to_string(),
                    size: None,
                    loaded: false,
                    path: None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// LM Studio `GET /api/v0/models`: `{"data":[{"id","type","state":"loaded"|"not-loaded"}]}`.
pub fn parse_lmstudio_v0(body: &str) -> Vec<LocalModel> {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    v["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|m| m["type"].as_str().is_none_or(|t| t == "llm" || t == "vlm"))
                .filter_map(|m| {
                    let id = m["id"].as_str()?.to_string();
                    Some(LocalModel {
                        provider: LocalProvider::LmStudio,
                        name: id.clone(),
                        size: None,
                        loaded: m["state"].as_str() == Some("loaded"),
                        path: None,
                        id,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Ollama `GET /api/tags`: `{"models":[{"name","size"}]}`.
pub fn parse_ollama_tags(body: &str) -> Vec<LocalModel> {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    v["models"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|m| {
                    let id = m["name"].as_str()?.to_string();
                    Some(LocalModel {
                        provider: LocalProvider::Ollama,
                        name: id.clone(),
                        size: m["size"].as_u64(),
                        loaded: false,
                        path: None,
                        id,
                    })
                })
                .filter(|m| !m.id.to_ascii_lowercase().contains("embed"))
                .collect()
        })
        .unwrap_or_default()
}

/// Ollama `GET /api/ps`: nombres de los modelos cargados.
pub fn parse_ollama_ps(body: &str) -> HashSet<String> {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v["models"].as_array().map(|a| {
                a.iter()
                    .filter_map(|m| m["name"].as_str().map(str::to_string))
                    .collect()
            })
        })
        .unwrap_or_default()
}

// ── Lectura del disco (servidor apagado) ─────────────────────────────────────

/// Carpetas de modelos de LM Studio: la que eligió el usuario (`downloadsFolder` de
/// `~/.lmstudio/settings.json`), la de siempre y la de versiones viejas.
pub fn lmstudio_dirs(home: &Path) -> Vec<PathBuf> {
    let settings = std::fs::read_to_string(home.join(".lmstudio").join("settings.json")).ok();
    let mut dirs: Vec<PathBuf> = settings
        .as_deref()
        .and_then(lmstudio_downloads_folder)
        .into_iter()
        .collect();
    dirs.push(home.join(".lmstudio").join("models"));
    dirs.push(home.join(".cache").join("lm-studio").join("models"));
    dirs
}

/// `downloadsFolder` de la configuración de LM Studio (solo esa clave: el archivo tiene otros datos,
/// como tokens, que no se leen ni se guardan).
pub fn lmstudio_downloads_folder(settings_json: &str) -> Option<PathBuf> {
    let v: Value = serde_json::from_str(settings_json).ok()?;
    v.get("downloadsFolder")?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// Modelos de LM Studio en `root/<publicador>/<repo>/<archivo>.gguf` (sin proyectores de visión
/// `mmproj` ni modelos de embeddings). Id provisional = nombre del archivo sin extensión.
pub fn scan_lmstudio(root: &Path) -> Vec<LocalModel> {
    let mut out = Vec::new();
    let Ok(publishers) = std::fs::read_dir(root) else {
        return out;
    };
    for publisher in publishers.flatten().filter(|e| e.path().is_dir()) {
        let Ok(repos) = std::fs::read_dir(publisher.path()) else {
            continue;
        };
        for repo in repos.flatten().filter(|e| e.path().is_dir()) {
            let Ok(files) = std::fs::read_dir(repo.path()) else {
                continue;
            };
            for f in files.flatten() {
                let path = f.path();
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let lower = name.to_ascii_lowercase();
                if !lower.ends_with(".gguf") || lower.contains("mmproj") || lower.contains("embed") {
                    continue;
                }
                let stem = name[..name.len() - 5].to_string();
                let rel = format!(
                    "{}/{}/{}",
                    publisher.file_name().to_string_lossy(),
                    repo.file_name().to_string_lossy(),
                    name
                );
                out.push(LocalModel {
                    provider: LocalProvider::LmStudio,
                    id: stem.clone(),
                    name: stem,
                    size: f.metadata().ok().map(|m| m.len()),
                    loaded: false,
                    path: Some(rel),
                });
            }
        }
    }
    out.sort_by_key(|m| m.name.to_ascii_lowercase());
    out
}

/// Modelos de Ollama: `root/manifests/<registro>/<espacio>/<modelo>/<etiqueta>` (JSON con sus capas).
/// Id = `modelo:etiqueta` (o `espacio/modelo:etiqueta` fuera de `library`), como `ollama run`.
pub fn scan_ollama(root: &Path) -> Vec<LocalModel> {
    let mut out = Vec::new();
    let manifests = root.join("manifests");
    let Ok(registries) = std::fs::read_dir(&manifests) else {
        return out;
    };
    for registry in registries.flatten().filter(|e| e.path().is_dir()) {
        let Ok(spaces) = std::fs::read_dir(registry.path()) else {
            continue;
        };
        for space in spaces.flatten().filter(|e| e.path().is_dir()) {
            let Ok(models) = std::fs::read_dir(space.path()) else {
                continue;
            };
            for model in models.flatten().filter(|e| e.path().is_dir()) {
                let Ok(tags) = std::fs::read_dir(model.path()) else {
                    continue;
                };
                for tag in tags.flatten().filter(|e| e.path().is_file()) {
                    let space_name = space.file_name().to_string_lossy().into_owned();
                    let base = model.file_name().to_string_lossy().into_owned();
                    let id = if space_name == "library" {
                        format!("{base}:{}", tag.file_name().to_string_lossy())
                    } else {
                        format!("{space_name}/{base}:{}", tag.file_name().to_string_lossy())
                    };
                    if id.to_ascii_lowercase().contains("embed") {
                        continue;
                    }
                    let size = std::fs::read_to_string(tag.path())
                        .ok()
                        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                        .and_then(|v| {
                            v["layers"]
                                .as_array()
                                .map(|l| l.iter().filter_map(|x| x["size"].as_u64()).sum::<u64>())
                        })
                        .filter(|&s| s > 0);
                    out.push(LocalModel {
                        provider: LocalProvider::Ollama,
                        name: id.clone(),
                        id,
                        size,
                        loaded: false,
                        path: None,
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Carpeta de modelos de Ollama (`OLLAMA_MODELS` o `~/.ollama/models`).
pub fn ollama_dir(home: &Path) -> PathBuf {
    std::env::var_os("OLLAMA_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".ollama").join("models"))
}

/// El `modelKey` real de un modelo de LM Studio leído del disco: el de `lms ls --json` con la misma
/// ruta relativa (las barras pueden venir al revés en Windows).
pub fn lmstudio_key_for(path: &str, listed: &[LocalModel]) -> Option<String> {
    let norm = |p: &str| p.replace('\\', "/").to_ascii_lowercase();
    let want = norm(path);
    listed
        .iter()
        .find(|m| m.path.as_deref().is_some_and(|p| norm(p) == want))
        .map(|m| m.id.clone())
}

/// Pasa a los modelos leídos del disco la clave, el nombre y el tamaño reales de `lms ls --json`
/// (el que pide el usuario, nunca la lectura automática: `lms ls` arranca el servicio).
pub fn apply_lms_keys(models: &mut [LocalModel], listed: &[LocalModel]) {
    for m in models.iter_mut() {
        let Some(path) = m.path.clone() else { continue };
        if let Some(key) = lmstudio_key_for(&path, listed)
            && let Some(l) = listed.iter().find(|l| l.id == key)
        {
            m.id = l.id.clone();
            m.name = l.name.clone();
            m.size = l.size.or(m.size);
        }
    }
}

/// `lms ls --json` (solo cuando el usuario actualiza o elige un modelo de LM Studio).
pub fn lms_list() -> Vec<LocalModel> {
    let mut cmd = std::process::Command::new("lms");
    cmd.args(["ls", "--json"]);
    hide_window(&mut cmd);
    cmd.output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_lms_ls_json(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// Inicia el servidor del proveedor en segundo plano (sin ventana) y espera hasta `wait` a que su
/// puerto responda. `Err` con el motivo si no se pudo.
pub fn start_server_and_wait(provider: LocalProvider, wait: Duration) -> Result<(), String> {
    if port_open(provider.port(), Duration::from_millis(150)) {
        return Ok(());
    }
    let Some((program, args)) = provider.start_server() else {
        return Err(format!("{} se inicia desde su aplicación", provider.name()));
    };
    let mut cmd = std::process::Command::new(program);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    hide_window(&mut cmd);
    cmd.spawn().map_err(|e| format!("{program}: {e}"))?;
    let step = Duration::from_millis(500);
    let mut waited = Duration::ZERO;
    while waited < wait {
        std::thread::sleep(step);
        waited += step;
        if port_open(provider.port(), Duration::from_millis(150)) {
            return Ok(());
        }
    }
    Err(format!(
        "{} no respondió en el puerto {}",
        provider.name(),
        provider.port()
    ))
}

#[cfg(windows)]
fn hide_window(cmd: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW: sin consola negra.
    cmd.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_window(_cmd: &mut std::process::Command) {}

// ── Servidor local ───────────────────────────────────────────────────────────

/// ¿Hay algo escuchando en `127.0.0.1:port`? (Nunca otra dirección).
pub fn port_open(port: u16, timeout: Duration) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, timeout).is_ok()
}

/// Cuerpo de una respuesta HTTP si el estado es 2xx (respuesta de HTTP/1.0, sin `chunked`).
pub fn http_body(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    let (head, body) = text.split_once("\r\n\r\n")?;
    let status = head.lines().next()?.split_whitespace().nth(1)?;
    if !status.starts_with('2') {
        return None;
    }
    let chunked = head.lines().any(|l| {
        l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.to_ascii_lowercase().contains("chunked")
    });
    if chunked {
        return Some(dechunk(body));
    }
    Some(body.to_string())
}

/// Por si un servidor responde `chunked` igual.
fn dechunk(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some((size_line, after)) = rest.split_once("\r\n") {
        let Ok(size) = usize::from_str_radix(size_line.split(';').next().unwrap_or("").trim(), 16) else {
            break;
        };
        if size == 0 || after.len() < size {
            break;
        }
        out.push_str(&after[..size]);
        rest = after[size..].strip_prefix("\r\n").unwrap_or(&after[size..]);
    }
    out
}

/// `GET <path>` a `127.0.0.1:port` con HTTP/1.0 (como mucho 2 MB y 1,5 s).
pub fn local_get(port: u16, path: &str) -> Option<String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(1_500))).ok()?;
    s.set_write_timeout(Some(Duration::from_millis(500))).ok()?;
    let req = format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nAccept: application/json\r\n\r\n");
    s.write_all(req.as_bytes()).ok()?;
    let mut raw = Vec::new();
    let _ = s.take(2 * 1024 * 1024).read_to_end(&mut raw);
    http_body(&raw)
}

/// Lee un proveedor: con el servidor encendido, de su API; si no, del disco. `has_cli(cmd)` dice si
/// un comando está en el PATH.
pub fn read_provider(provider: LocalProvider, home: &Path, has_cli: &dyn Fn(&str) -> bool) -> ProviderStatus {
    let cli = provider.command().is_some_and(has_cli);
    // Jan y llama.cpp: el puerto 8080 lo usan muchos otros programas; llama.cpp solo con su CLI.
    let probe = match provider {
        LocalProvider::LlamaCpp => cli,
        _ => true,
    };
    let server = probe && port_open(provider.port(), Duration::from_millis(150));
    let mut models = Vec::new();
    if server {
        models = match provider {
            LocalProvider::LmStudio => {
                let v0 = local_get(provider.port(), "/api/v0/models")
                    .map(|b| parse_lmstudio_v0(&b))
                    .unwrap_or_default();
                if v0.is_empty() {
                    local_get(provider.port(), "/v1/models")
                        .map(|b| parse_openai_models(provider, &b))
                        .unwrap_or_default()
                } else {
                    v0
                }
            }
            LocalProvider::Ollama => {
                let mut m = local_get(provider.port(), "/api/tags")
                    .map(|b| parse_ollama_tags(&b))
                    .unwrap_or_default();
                let loaded = local_get(provider.port(), "/api/ps")
                    .map(|b| parse_ollama_ps(&b))
                    .unwrap_or_default();
                for x in &mut m {
                    x.loaded = loaded.contains(&x.id);
                }
                m
            }
            LocalProvider::Jan | LocalProvider::LlamaCpp => local_get(provider.port(), "/v1/models")
                .map(|b| parse_openai_models(provider, &b))
                .unwrap_or_default(),
        };
    }
    if models.is_empty() {
        models = match provider {
            LocalProvider::LmStudio if cli => lmstudio_dirs(home)
                .iter()
                .map(|d| scan_lmstudio(d))
                .find(|m| !m.is_empty())
                .unwrap_or_default(),
            LocalProvider::Ollama if cli => scan_ollama(&ollama_dir(home)),
            _ => Vec::new(),
        };
    }
    // Jan no tiene CLI: cuenta como instalado si su servidor respondió con modelos.
    let installed = cli || (provider == LocalProvider::Jan && server && !models.is_empty());
    ProviderStatus {
        provider,
        installed,
        server,
        models,
    }
}

/// ¿Está `cmd` en alguna carpeta de `path_var`? En Windows con `.exe`/`.cmd`/`.bat`/`.ps1`; en
/// Linux y Mac, el archivo tal cual.
pub fn command_in_path(cmd: &str, path_var: &std::ffi::OsStr) -> bool {
    std::env::split_paths(path_var).any(|dir| {
        if cfg!(windows) {
            [".exe", ".cmd", ".bat", ".ps1"]
                .iter()
                .any(|ext| dir.join(format!("{cmd}{ext}")).is_file())
        } else {
            dir.join(cmd).is_file()
        }
    })
}

/// Lectura completa con el PATH y la carpeta personal de este proceso.
pub fn detect_now(home: &Path) -> Vec<ProviderStatus> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    detect(home, &|cmd| command_in_path(cmd, &path))
}

/// Todos los proveedores instalados (los que no, no se devuelven).
pub fn detect(home: &Path, has_cli: &dyn Fn(&str) -> bool) -> Vec<ProviderStatus> {
    LocalProvider::ALL
        .iter()
        .map(|&p| read_provider(p, home, has_cli))
        .filter(|s| s.installed)
        .collect()
}

// ── OpenCode con un modelo local ─────────────────────────────────────────────

/// Lo que se agrega a la configuración de OpenCode de la sesión (`OPENCODE_CONFIG`) para usar el
/// modelo: un proveedor compatible con OpenAI que apunta al servidor local y `model`.
pub fn opencode_provider(model: &LocalModel) -> Value {
    let p = model.provider;
    let mut models = Map::new();
    models.insert(model.id.clone(), json!({ "name": model.name }));
    let mut provider = Map::new();
    provider.insert(
        p.id().to_string(),
        json!({
            "npm": "@ai-sdk/openai-compatible",
            "name": format!("{} (local)", p.name()),
            "options": { "baseURL": p.base_url() },
            "models": Value::Object(models),
        }),
    );
    json!({
        "provider": Value::Object(provider),
        "model": format!("{}/{}", p.id(), model.id),
    })
}

/// Tamaño legible ("4,7 GB", "812 MB").
pub fn format_size(bytes: u64) -> String {
    const GB: f64 = 1_000_000_000.0;
    const MB: f64 = 1_000_000.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB).replace('.', ",")
    } else {
        format!("{:.0} MB", (b / MB).max(1.0))
    }
}

/// Nombre corto para el panel: sin el publicador ni la etiqueta `:latest`.
pub fn short_name(model: &LocalModel) -> String {
    let n = model.name.rsplit('/').next().unwrap_or(&model.name);
    n.strip_suffix(":latest").unwrap_or(n).to_string()
}

/// Modelos agrupados por proveedor, en el orden de [`LocalProvider::ALL`].
pub fn by_provider(list: &[ProviderStatus]) -> BTreeMap<LocalProvider, &ProviderStatus> {
    list.iter().map(|s| (s.provider, s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Formato real de `lms ls --json` (anonimizado: modelos y publicadores inventados).
    const LMS_JSON: &str = r#"[{"type":"llm","modelKey":"qwen-coder-mini","format":"gguf","displayName":"Qwen Coder Mini","publisher":"demo-pub","path":"demo-pub/Qwen-Coder-Mini-GGUF/qwen-coder-mini.Q4_K_M.gguf","sizeBytes":4683000000,"paramsString":"7B","architecture":"qwen2","quantization":{"name":"Q4_K_M","bits":4},"vision":false,"trainedForToolUse":true,"maxContextLength":32768},{"type":"embedding","modelKey":"text-embedding-demo","format":"gguf","displayName":"Demo Embed","publisher":"demo-pub","path":"demo-pub/demo-embed-GGUF/demo-embed.Q4_K_M.gguf","sizeBytes":84106624}]"#;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kuntur-local-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn lms_ls_json_solo_llm_con_su_clave() {
        let m = parse_lms_ls_json(LMS_JSON);
        assert_eq!(m.len(), 1, "los de embeddings no son agentes");
        assert_eq!(m[0].id, "qwen-coder-mini");
        assert_eq!(m[0].name, "Qwen Coder Mini");
        assert_eq!(m[0].size, Some(4_683_000_000));
        assert_eq!(
            m[0].path.as_deref(),
            Some("demo-pub/Qwen-Coder-Mini-GGUF/qwen-coder-mini.Q4_K_M.gguf")
        );
        assert!(parse_lms_ls_json("no es json").is_empty());
        assert!(parse_lms_ls_json("[]").is_empty());
    }

    #[test]
    fn respuestas_de_las_apis_locales() {
        let openai = r#"{"object":"list","data":[{"id":"llama-demo"},{"id":"nomic-embed-demo"}]}"#;
        let m = parse_openai_models(LocalProvider::Jan, openai);
        assert_eq!(m.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["llama-demo"]);

        let v0 = r#"{"data":[{"id":"qwen-coder-mini","type":"llm","state":"loaded"},{"id":"otro","type":"vlm","state":"not-loaded"},{"id":"emb","type":"embeddings","state":"not-loaded"}]}"#;
        let m = parse_lmstudio_v0(v0);
        assert_eq!(m.len(), 2);
        assert!(m[0].loaded && !m[1].loaded);

        let tags = r#"{"models":[{"name":"llama-demo:latest","size":4700000000},{"name":"nomic-embed-text:latest","size":1}]}"#;
        let m = parse_ollama_tags(tags);
        assert_eq!(m.len(), 1);
        assert_eq!(
            (m[0].id.as_str(), m[0].size),
            ("llama-demo:latest", Some(4_700_000_000))
        );
        let ps = parse_ollama_ps(r#"{"models":[{"name":"llama-demo:latest"}]}"#);
        assert!(ps.contains("llama-demo:latest"));
        assert!(parse_ollama_ps("basura").is_empty());
    }

    #[test]
    fn lee_lm_studio_del_disco_sin_mmproj_ni_embeddings() {
        let root = tmp("lms");
        let repo = root.join("demo-pub").join("Qwen-Coder-Mini-GGUF");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("qwen-coder-mini.Q4_K_M.gguf"), b"0123456789").unwrap();
        std::fs::write(repo.join("mmproj-model-f16.gguf"), b"x").unwrap();
        std::fs::write(repo.join("README.md"), b"x").unwrap();
        let emb = root.join("demo-pub").join("demo-embed-GGUF");
        std::fs::create_dir_all(&emb).unwrap();
        std::fs::write(emb.join("demo-embed.Q4_K_M.gguf"), b"x").unwrap();
        let m = scan_lmstudio(&root);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].id, "qwen-coder-mini.Q4_K_M");
        assert_eq!(m[0].size, Some(10));
        assert_eq!(
            m[0].path.as_deref(),
            Some("demo-pub/Qwen-Coder-Mini-GGUF/qwen-coder-mini.Q4_K_M.gguf")
        );
        // Con `lms ls --json` se encuentra su clave real por la ruta (aunque venga con `\`).
        let listed = parse_lms_ls_json(LMS_JSON);
        assert_eq!(
            lmstudio_key_for(r"demo-pub\Qwen-Coder-Mini-GGUF\qwen-coder-mini.Q4_K_M.gguf", &listed).as_deref(),
            Some("qwen-coder-mini")
        );
        assert_eq!(lmstudio_key_for("otro/modelo.gguf", &listed), None);
        assert!(scan_lmstudio(&root.join("no-existe")).is_empty());
        // Al actualizar, el modelo del disco toma la clave y el nombre reales.
        let mut m = m;
        apply_lms_keys(&mut m, &listed);
        assert_eq!(
            (m[0].id.as_str(), m[0].name.as_str()),
            ("qwen-coder-mini", "Qwen Coder Mini")
        );
        assert_eq!(m[0].size, Some(4_683_000_000));
    }

    #[test]
    fn lee_ollama_del_disco_con_su_tamano() {
        let root = tmp("ollama");
        let lib = root
            .join("manifests")
            .join("registry.ollama.ai")
            .join("library")
            .join("llama-demo");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(
            lib.join("latest"),
            r#"{"layers":[{"size":4000000000},{"size":700000000}],"config":{"size":10}}"#,
        )
        .unwrap();
        let ns = root
            .join("manifests")
            .join("registry.ollama.ai")
            .join("demo-user")
            .join("coder");
        std::fs::create_dir_all(&ns).unwrap();
        std::fs::write(ns.join("7b"), "{}").unwrap();
        let m = scan_ollama(&root);
        let ids: Vec<&str> = m.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, ["demo-user/coder:7b", "llama-demo:latest"]);
        assert_eq!(m[1].size, Some(4_700_000_000));
        assert_eq!(m[0].size, None);
    }

    #[test]
    fn http_minimo_y_solo_localhost() {
        let ok = b"HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n{\"data\":[]}";
        assert_eq!(http_body(ok).as_deref(), Some("{\"data\":[]}"));
        assert_eq!(http_body(b"HTTP/1.1 404 Not Found\r\n\r\nno"), None);
        assert_eq!(http_body(b"basura"), None);
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n";
        assert_eq!(http_body(chunked).as_deref(), Some("{\"a\":1}"));

        // Un "servidor" sintético en un puerto libre de 127.0.0.1.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let srv = std::thread::spawn(move || {
            // La primera conexión es la de `port_open` (no manda nada); la segunda, la petición.
            loop {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = [0u8; 512];
                let n = s.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    continue;
                }
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                s.write_all(b"HTTP/1.0 200 OK\r\n\r\n{\"data\":[{\"id\":\"demo\"}]}")
                    .unwrap();
                return req;
            }
        });
        assert!(port_open(port, Duration::from_millis(500)));
        let body = local_get(port, "/v1/models").unwrap();
        assert_eq!(parse_openai_models(LocalProvider::Jan, &body)[0].id, "demo");
        let req = srv.join().unwrap();
        assert!(req.starts_with("GET /v1/models HTTP/1.0\r\nHost: 127.0.0.1:"));
    }

    #[test]
    fn busca_el_comando_en_el_path() {
        let dir = tmp("path");
        let file = if cfg!(windows) { "lms.exe" } else { "lms" };
        std::fs::write(dir.join(file), b"").unwrap();
        let path = std::env::join_paths([dir.join("otra"), dir.clone()]).unwrap();
        assert!(command_in_path("lms", &path));
        assert!(!command_in_path("ollama", &path));
    }

    #[test]
    fn deteccion_sin_cli_ni_servidor_no_devuelve_nada() {
        let home = tmp("home-vacio");
        // Ningún comando en el PATH: LM Studio, Ollama y llama.cpp no cuentan; Jan solo si su
        // servidor responde con modelos (aquí no hay nada escuchando en 1337 salvo coincidencia).
        let found = detect(&home, &|_| false);
        assert!(found.iter().all(|s| s.provider == LocalProvider::Jan));
    }

    #[test]
    fn carpeta_de_modelos_elegida_en_lm_studio() {
        let s = r#"{"downloadsFolder":"D:\\Modelos","hfDownloadToken":"no-se-lee","otra":1}"#;
        assert_eq!(lmstudio_downloads_folder(s), Some(PathBuf::from(r"D:\Modelos")));
        assert_eq!(lmstudio_downloads_folder(r#"{"downloadsFolder":"  "}"#), None);
        assert_eq!(lmstudio_downloads_folder("no es json"), None);
        // La elegida va primero, después las de siempre.
        let home = tmp("home-settings");
        std::fs::create_dir_all(home.join(".lmstudio")).unwrap();
        let custom = home.join("mis-modelos");
        std::fs::write(
            home.join(".lmstudio").join("settings.json"),
            json!({ "downloadsFolder": custom.to_string_lossy() }).to_string(),
        )
        .unwrap();
        let dirs = lmstudio_dirs(&home);
        assert_eq!(dirs[0], custom);
        assert_eq!(dirs[1], home.join(".lmstudio").join("models"));
    }

    #[test]
    fn lm_studio_instalado_con_servidor_apagado_lee_el_disco() {
        let home = tmp("home-lms");
        let repo = home.join(".lmstudio").join("models").join("demo-pub").join("Demo-GGUF");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("demo-7b.Q4_K_M.gguf"), b"12345").unwrap();
        let s = read_provider(LocalProvider::LmStudio, &home, &|c| c == "lms");
        assert!(s.installed);
        if !s.server {
            assert_eq!(s.models.len(), 1);
            assert_eq!(s.models[0].id, "demo-7b.Q4_K_M");
        }
    }

    #[test]
    fn configuracion_de_opencode_para_el_modelo_local() {
        let m = LocalModel {
            provider: LocalProvider::LmStudio,
            id: "qwen-coder-mini".into(),
            name: "Qwen Coder Mini".into(),
            size: None,
            loaded: true,
            path: None,
        };
        let v = opencode_provider(&m);
        assert_eq!(v["model"], "lmstudio/qwen-coder-mini");
        let p = &v["provider"]["lmstudio"];
        assert_eq!(p["npm"], "@ai-sdk/openai-compatible");
        assert_eq!(p["options"]["baseURL"], "http://127.0.0.1:1234/v1");
        assert_eq!(p["models"]["qwen-coder-mini"]["name"], "Qwen Coder Mini");
        let o = LocalModel {
            provider: LocalProvider::Ollama,
            id: "llama-demo:latest".into(),
            ..m
        };
        assert_eq!(
            opencode_provider(&o)["provider"]["ollama"]["options"]["baseURL"],
            "http://127.0.0.1:11434/v1"
        );
    }

    #[test]
    fn lineas_de_chat_e_inicio_del_servidor() {
        assert_eq!(
            LocalProvider::LmStudio
                .chat_line("qwen-coder-mini", ShellFlavor::PowerShell)
                .as_deref(),
            Some("lms chat qwen-coder-mini")
        );
        assert_eq!(
            LocalProvider::Ollama
                .chat_line("llama demo:latest", ShellFlavor::Posix)
                .as_deref(),
            Some("ollama run 'llama demo:latest'")
        );
        assert_eq!(LocalProvider::Jan.chat_line("x", ShellFlavor::PowerShell), None);
        assert_eq!(
            LocalProvider::LmStudio.start_server(),
            Some(("lms", &["server", "start"][..]))
        );
        assert_eq!(LocalProvider::Jan.start_server(), None);
    }

    #[test]
    fn tamanos_y_nombres_cortos() {
        assert_eq!(format_size(4_683_000_000), "4,7 GB");
        assert_eq!(format_size(84_106_624), "84 MB");
        assert_eq!(format_size(10), "1 MB");
        let m = LocalModel {
            provider: LocalProvider::Ollama,
            id: "demo-user/coder:latest".into(),
            name: "demo-user/coder:latest".into(),
            size: None,
            loaded: false,
            path: None,
        };
        assert_eq!(short_name(&m), "coder");
    }

    /// Comprobación manual en la PC real (no imprime nombres de modelos, solo cantidades). Nunca
    /// ejecuta `lms ls` ni inicia servidores.
    /// `cargo test -p kuntur-agents --lib detecta_lo_instalado -- --ignored --nocapture`.
    #[test]
    #[ignore = "lee la PC real: solo a mano"]
    fn detecta_lo_instalado_en_esta_pc() {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .expect("carpeta personal");
        for s in detect_now(&home) {
            eprintln!(
                "{}: instalado={} servidor={} modelos={}",
                s.provider.name(),
                s.installed,
                s.server,
                s.models.len()
            );
        }
    }
}
