//! Configuración MCP y opciones del agente en el formato de cada CLI (verificado en su
//! documentación oficial; ver `docs/plans/f10-resultados.md`):
//!
//! - **Claude Code**: archivo JSON `{"mcpServers": {…}}` para `--mcp-config`; stdio con
//!   `type/command/args/env`, http con `type/url/headers`. Opciones: `--model`, `--effort`,
//!   `--permission-mode`.
//! - **Codex**: sobrescrituras `-c mcp_servers.<nombre>={…}` (valores TOML). Los secretos no van en
//!   la línea: se reenvían con `env_vars` / `bearer_token_env_var` desde variables que pone un
//!   script de la sesión. Opciones: `--model`, `-c model_reasoning_effort=…`, `--sandbox`,
//!   `--ask-for-approval`, `--dangerously-bypass-approvals-and-sandbox`.
//! - **OpenCode**: archivo JSON para `OPENCODE_CONFIG` (`mcp.<nombre>` en V1, `mcp.servers.<nombre>`
//!   en V2) con `type: local|remote`, `command` (lista), `environment`, `url`, `headers`.
//!   Opciones: `--model`, `--agent plan`, `--auto`, `permission` en el archivo (V1).

use std::collections::BTreeMap;

use kuntur_config::{McpProfileConfig, PermissionMode};
use serde_json::{Map, Value, json};

use crate::catalog::{ResolvedKind, ResolvedServer};
use crate::toml_text;

/// CLIs para los que Kuntur sabe generar la configuración MCP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cli {
    Claude,
    Codex,
    OpenCode,
}

impl Cli {
    /// Por el id del catálogo de agentes (`claude`, `codex`, `opencode`).
    pub fn from_agent_id(id: &str) -> Option<Cli> {
        match id {
            "claude" => Some(Cli::Claude),
            "codex" => Some(Cli::Codex),
            "opencode" => Some(Cli::OpenCode),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Cli::Claude => "Claude Code",
            Cli::Codex => "Codex",
            Cli::OpenCode => "OpenCode",
        }
    }
}

/// Opciones del agente que vienen del perfil de la sesión.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentOptions {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub permissions: Option<PermissionMode>,
}

impl AgentOptions {
    pub fn is_empty(&self) -> bool {
        self.model.is_none() && self.effort.is_none() && self.permissions.is_none()
    }
}

impl From<&McpProfileConfig> for AgentOptions {
    fn from(p: &McpProfileConfig) -> Self {
        let clean = |s: &Option<String>| s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
        Self {
            model: clean(&p.model),
            effort: clean(&p.effort),
            permissions: p.permissions,
        }
    }
}

/// Windows: un `.cmd`/`.bat` (p. ej. `npx`) no se puede lanzar como programa; va con `cmd /c`.
pub fn wrap_batch(command: &str, args: &[String]) -> (String, Vec<String>) {
    let mut all = vec!["/c".to_string(), command.to_string()];
    all.extend(args.iter().cloned());
    ("cmd".to_string(), all)
}

/// Variable con el token *bearer* de un servidor http (Codex la lee con `bearer_token_env_var`).
pub fn token_env_name(server: &str) -> String {
    let upper: String = server
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("KUNTUR_MCP_{upper}_TOKEN")
}

fn str_map(map: &BTreeMap<String, String>) -> Value {
    Value::Object(map.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect())
}

fn headers_with_token(headers: &BTreeMap<String, String>, token: &Option<String>) -> BTreeMap<String, String> {
    let mut h = headers.clone();
    if let Some(t) = token {
        h.insert("Authorization".into(), format!("Bearer {t}"));
    }
    h
}

/// Claude Code: contenido del archivo de `--mcp-config`. Incluye los secretos (el archivo vive en
/// la carpeta temporal de la sesión y se borra al cerrarla).
pub fn claude_json(servers: &[ResolvedServer]) -> String {
    let mut map = Map::new();
    for s in servers {
        let entry = match &s.kind {
            ResolvedKind::Stdio {
                command,
                args,
                env,
                secret_env,
            } => {
                let mut all = env.clone();
                all.extend(secret_env.clone());
                json!({ "type": "stdio", "command": command, "args": args, "env": str_map(&all) })
            }
            ResolvedKind::Http { url, headers, token } => {
                let h = headers_with_token(headers, token);
                let mut o = json!({ "type": "http", "url": url });
                if !h.is_empty() {
                    o["headers"] = str_map(&h);
                }
                o
            }
        };
        map.insert(s.name.clone(), entry);
    }
    serde_json::to_string_pretty(&json!({ "mcpServers": Value::Object(map) })).unwrap_or_default()
}

/// Codex: sobrescrituras `-c` y las variables secretas que tiene que poner el script de la sesión.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodexOverrides {
    /// Pares `-c`, `mcp_servers.<nombre>={…}`, listos para agregar a la línea.
    pub args: Vec<String>,
    /// Variables secretas (nombre, valor): nunca van en la línea.
    pub secret_env: Vec<(String, String)>,
}

pub fn codex_overrides(servers: &[ResolvedServer]) -> CodexOverrides {
    let mut out = CodexOverrides::default();
    for s in servers {
        let mut fields: Vec<String> = Vec::new();
        match &s.kind {
            ResolvedKind::Stdio {
                command,
                args,
                env,
                secret_env,
            } => {
                fields.push(format!("command={}", toml_text::string(command)));
                if !args.is_empty() {
                    fields.push(format!("args={}", toml_text::array(args)));
                }
                if !env.is_empty() {
                    fields.push(format!("env={}", toml_text::inline_table(env)));
                }
                if !secret_env.is_empty() {
                    let names: Vec<String> = secret_env.keys().cloned().collect();
                    fields.push(format!("env_vars={}", toml_text::array(&names)));
                    out.secret_env
                        .extend(secret_env.iter().map(|(k, v)| (k.clone(), v.clone())));
                }
            }
            ResolvedKind::Http { url, headers, token } => {
                fields.push(format!("url={}", toml_text::string(url)));
                if !headers.is_empty() {
                    fields.push(format!("http_headers={}", toml_text::inline_table(headers)));
                }
                if let Some(t) = token {
                    let var = token_env_name(&s.name);
                    fields.push(format!("bearer_token_env_var={}", toml_text::string(&var)));
                    out.secret_env.push((var, t.clone()));
                }
            }
        }
        out.args.push("-c".into());
        out.args.push(format!(
            "mcp_servers.{}={{{}}}",
            toml_text::key(&s.name),
            fields.join(",")
        ));
    }
    out
}

/// OpenCode: contenido del archivo de `OPENCODE_CONFIG` (`None` si no hay nada que escribir).
pub fn opencode_json(servers: &[ResolvedServer], v2: bool, permissions: Option<PermissionMode>) -> Option<String> {
    opencode_json_with(servers, v2, permissions, None)
}

/// Igual que [`opencode_json`], más las claves de `extra` (ajustes v0.2.1: el proveedor y el modelo
/// de una IA local) en el mismo archivo.
pub fn opencode_json_with(
    servers: &[ResolvedServer],
    v2: bool,
    permissions: Option<PermissionMode>,
    extra: Option<&Value>,
) -> Option<String> {
    let mut servers_map = Map::new();
    for s in servers {
        let mut entry = match &s.kind {
            ResolvedKind::Stdio {
                command,
                args,
                env,
                secret_env,
            } => {
                let mut cmd = vec![command.clone()];
                cmd.extend(args.iter().cloned());
                let mut all = env.clone();
                all.extend(secret_env.clone());
                let mut o = json!({ "type": "local", "command": cmd });
                if !all.is_empty() {
                    o["environment"] = str_map(&all);
                }
                o
            }
            ResolvedKind::Http { url, headers, token } => {
                let h = headers_with_token(headers, token);
                let mut o = json!({ "type": "remote", "url": url });
                if !h.is_empty() {
                    o["headers"] = str_map(&h);
                }
                o
            }
        };
        if !v2 {
            entry["enabled"] = Value::Bool(true);
        }
        servers_map.insert(s.name.clone(), entry);
    }
    let permission = if v2 { None } else { opencode_permission(permissions) };
    let extra = extra.and_then(Value::as_object).filter(|o| !o.is_empty());
    if servers_map.is_empty() && permission.is_none() && extra.is_none() {
        return None;
    }
    let mut root = Map::new();
    root.insert("$schema".into(), json!("https://opencode.ai/config.json"));
    if !servers_map.is_empty() {
        let mcp = if v2 {
            json!({ "servers": Value::Object(servers_map) })
        } else {
            Value::Object(servers_map)
        };
        root.insert("mcp".into(), mcp);
    }
    if let Some(p) = permission {
        root.insert("permission".into(), p);
    }
    for (k, v) in extra.into_iter().flatten() {
        root.insert(k.clone(), v.clone());
    }
    serde_json::to_string_pretty(&Value::Object(root)).ok()
}

/// Permisos de OpenCode (V1): `edit` / `bash` con `ask` o `allow`. Plan y Total van por flags.
fn opencode_permission(p: Option<PermissionMode>) -> Option<Value> {
    match p? {
        PermissionMode::Preguntar => Some(json!({ "edit": "ask", "bash": "ask" })),
        PermissionMode::Ediciones => Some(json!({ "edit": "allow", "bash": "ask" })),
        PermissionMode::Plan | PermissionMode::Total => None,
    }
}

fn s(x: &str) -> String {
    x.to_string()
}

/// Flags de modelo, esfuerzo y permisos para el CLI, y avisos de lo que no se pudo traducir.
pub fn option_args(cli: Cli, opts: &AgentOptions, opencode_v2: bool) -> (Vec<String>, Vec<String>) {
    let mut args = Vec::new();
    let mut warnings = Vec::new();
    if let Some(m) = &opts.model {
        args.extend([s("--model"), m.clone()]);
    }
    if let Some(e) = &opts.effort {
        match cli {
            Cli::Claude => args.extend([s("--effort"), e.clone()]),
            Cli::Codex => args.extend([s("-c"), format!("model_reasoning_effort={}", toml_text::string(e))]),
            Cli::OpenCode => warnings.push(format!("OpenCode no tiene un flag de esfuerzo: se ignora «{e}»")),
        }
    }
    if let Some(p) = opts.permissions {
        match (cli, p) {
            (Cli::Claude, _) => args.extend([
                s("--permission-mode"),
                s(match p {
                    PermissionMode::Preguntar => "default",
                    PermissionMode::Ediciones => "acceptEdits",
                    PermissionMode::Plan => "plan",
                    PermissionMode::Total => "bypassPermissions",
                }),
            ]),
            (Cli::Codex, PermissionMode::Total) => args.push(s("--dangerously-bypass-approvals-and-sandbox")),
            (Cli::Codex, _) => {
                let (sandbox, approval) = match p {
                    PermissionMode::Preguntar => ("read-only", "on-request"),
                    PermissionMode::Ediciones => ("workspace-write", "on-request"),
                    _ => ("read-only", "never"),
                };
                args.extend([s("--sandbox"), s(sandbox), s("--ask-for-approval"), s(approval)]);
            }
            (Cli::OpenCode, PermissionMode::Plan) => args.extend([s("--agent"), s("plan")]),
            (Cli::OpenCode, PermissionMode::Total) => args.push(s("--auto")),
            (Cli::OpenCode, _) if opencode_v2 => warnings.push(s(
                "OpenCode V2: los permisos por perfil no se traducen (usa `permissions` en su configuración)",
            )),
            // Preguntar / Ediciones: `permission` en el archivo de la sesión.
            (Cli::OpenCode, _) => {}
        }
    }
    (args, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stdio() -> ResolvedServer {
        ResolvedServer {
            name: "db".into(),
            kind: ResolvedKind::Stdio {
                command: r"C:\Herramientas\srv.exe".into(),
                args: vec!["--modo".into(), "it's".into()],
                env: [("LOG".to_string(), "info".to_string())].into(),
                secret_env: [("DB_PASSWORD".to_string(), "s3creto".to_string())].into(),
            },
        }
    }

    fn http() -> ResolvedServer {
        ResolvedServer {
            name: "docs".into(),
            kind: ResolvedKind::Http {
                url: "https://mcp.ejemplo.test/mcp".into(),
                headers: [("X-Region".to_string(), "sa".to_string())].into(),
                token: Some("tok".into()),
            },
        }
    }

    #[test]
    fn claude_formato_mcp_servers() {
        let v: Value = serde_json::from_str(&claude_json(&[stdio(), http()])).unwrap();
        let db = &v["mcpServers"]["db"];
        assert_eq!(db["type"], "stdio");
        assert_eq!(db["command"], r"C:\Herramientas\srv.exe");
        assert_eq!(db["args"], json!(["--modo", "it's"]));
        assert_eq!(db["env"]["LOG"], "info");
        assert_eq!(db["env"]["DB_PASSWORD"], "s3creto");
        let docs = &v["mcpServers"]["docs"];
        assert_eq!(docs["type"], "http");
        assert_eq!(docs["url"], "https://mcp.ejemplo.test/mcp");
        assert_eq!(docs["headers"]["Authorization"], "Bearer tok");
        assert_eq!(docs["headers"]["X-Region"], "sa");
        // Sin servidores: objeto vacío (válido para `--mcp-config`).
        let v: Value = serde_json::from_str(&claude_json(&[])).unwrap();
        assert_eq!(v, json!({ "mcpServers": {} }));
    }

    #[test]
    fn codex_sobrescrituras_toml_sin_secretos_en_la_linea() {
        let o = codex_overrides(&[stdio(), http()]);
        assert_eq!(o.args.len(), 4);
        assert_eq!(o.args[0], "-c");
        let joined = o.args.join(" ");
        assert!(!joined.contains("s3creto") && !joined.contains("tok'"), "{joined}");
        assert!(!joined.contains('"') || joined.contains("it's"));
        // Cada valor es TOML válido con la forma de `[mcp_servers.<nombre>]`.
        let doc = format!("{}\n{}", o.args[1], o.args[3]);
        let t: toml::Table = toml::from_str(&doc).unwrap();
        let db = &t["mcp_servers"]["db"];
        assert_eq!(db["command"].as_str(), Some(r"C:\Herramientas\srv.exe"));
        assert_eq!(db["args"][1].as_str(), Some("it's"));
        assert_eq!(db["env"]["LOG"].as_str(), Some("info"));
        assert_eq!(db["env_vars"][0].as_str(), Some("DB_PASSWORD"));
        let docs = &t["mcp_servers"]["docs"];
        assert_eq!(docs["url"].as_str(), Some("https://mcp.ejemplo.test/mcp"));
        assert_eq!(docs["http_headers"]["X-Region"].as_str(), Some("sa"));
        assert_eq!(docs["bearer_token_env_var"].as_str(), Some("KUNTUR_MCP_DOCS_TOKEN"));
        assert_eq!(
            o.secret_env,
            [
                ("DB_PASSWORD".to_string(), "s3creto".to_string()),
                ("KUNTUR_MCP_DOCS_TOKEN".to_string(), "tok".to_string())
            ]
        );
    }

    #[test]
    fn opencode_con_un_modelo_local_en_el_mismo_archivo() {
        let extra =
            json!({ "model": "lmstudio/demo", "provider": { "lmstudio": { "npm": "@ai-sdk/openai-compatible" } } });
        // Solo el modelo local: igual se escribe el archivo.
        let v: Value = serde_json::from_str(&opencode_json_with(&[], false, None, Some(&extra)).unwrap()).unwrap();
        assert_eq!(v["model"], "lmstudio/demo");
        assert!(v.get("mcp").is_none());
        // Con MCP: las dos cosas juntas.
        let v: Value =
            serde_json::from_str(&opencode_json_with(&[stdio()], false, None, Some(&extra)).unwrap()).unwrap();
        assert_eq!(v["model"], "lmstudio/demo");
        assert!(v["mcp"].is_object());
        assert_eq!(opencode_json_with(&[], false, None, Some(&json!({}))), None);
    }

    #[test]
    fn opencode_v1_y_v2() {
        let v: Value = serde_json::from_str(&opencode_json(&[stdio(), http()], false, None).unwrap()).unwrap();
        assert_eq!(v["$schema"], "https://opencode.ai/config.json");
        let db = &v["mcp"]["db"];
        assert_eq!(db["type"], "local");
        assert_eq!(db["command"], json!([r"C:\Herramientas\srv.exe", "--modo", "it's"]));
        assert_eq!(db["environment"]["DB_PASSWORD"], "s3creto");
        assert_eq!(db["enabled"], true);
        assert_eq!(v["mcp"]["docs"]["type"], "remote");
        assert_eq!(v["mcp"]["docs"]["headers"]["Authorization"], "Bearer tok");

        let v: Value = serde_json::from_str(&opencode_json(&[stdio()], true, None).unwrap()).unwrap();
        assert_eq!(v["mcp"]["servers"]["db"]["type"], "local");
        assert!(v["mcp"]["servers"]["db"].get("enabled").is_none());

        assert_eq!(opencode_json(&[], false, None), None);
        let v: Value =
            serde_json::from_str(&opencode_json(&[], false, Some(PermissionMode::Ediciones)).unwrap()).unwrap();
        assert_eq!(v["permission"], json!({ "edit": "allow", "bash": "ask" }));
        assert!(v.get("mcp").is_none());
    }

    #[test]
    fn opciones_por_cli() {
        let o = AgentOptions {
            model: Some("sonnet".into()),
            effort: Some("high".into()),
            permissions: Some(PermissionMode::Ediciones),
        };
        let (a, w) = option_args(Cli::Claude, &o, false);
        assert_eq!(
            a,
            [
                "--model",
                "sonnet",
                "--effort",
                "high",
                "--permission-mode",
                "acceptEdits"
            ]
        );
        assert!(w.is_empty());
        let (a, _) = option_args(Cli::Codex, &o, false);
        assert_eq!(
            a,
            [
                "--model",
                "sonnet",
                "-c",
                "model_reasoning_effort='high'",
                "--sandbox",
                "workspace-write",
                "--ask-for-approval",
                "on-request"
            ]
        );
        let (a, w) = option_args(Cli::OpenCode, &o, false);
        assert_eq!(a, ["--model", "sonnet"]);
        assert_eq!(w.len(), 1, "OpenCode no tiene esfuerzo");

        let total = AgentOptions {
            permissions: Some(PermissionMode::Total),
            ..Default::default()
        };
        assert_eq!(
            option_args(Cli::Claude, &total, false).0,
            ["--permission-mode", "bypassPermissions"]
        );
        assert_eq!(
            option_args(Cli::Codex, &total, false).0,
            ["--dangerously-bypass-approvals-and-sandbox"]
        );
        assert_eq!(option_args(Cli::OpenCode, &total, false).0, ["--auto"]);
        let plan = AgentOptions {
            permissions: Some(PermissionMode::Plan),
            ..Default::default()
        };
        assert_eq!(option_args(Cli::OpenCode, &plan, false).0, ["--agent", "plan"]);
        assert_eq!(option_args(Cli::Codex, &plan, false).0[1], "read-only");
    }

    #[test]
    fn otros() {
        assert_eq!(Cli::from_agent_id("codex"), Some(Cli::Codex));
        assert_eq!(Cli::from_agent_id("qwen"), None);
        assert_eq!(token_env_name("mi-docs"), "KUNTUR_MCP_MI_DOCS_TOKEN");
        assert_eq!(
            wrap_batch("npx", &["-y".into(), "pkg".into()]),
            (
                "cmd".to_string(),
                vec!["/c".into(), "npx".into(), "-y".into(), "pkg".into()]
            )
        );
        let p = McpProfileConfig {
            name: "x".into(),
            model: Some("  ".into()),
            effort: Some("max".into()),
            ..Default::default()
        };
        let o = AgentOptions::from(&p);
        assert_eq!(o.model, None);
        assert_eq!(o.effort.as_deref(), Some("max"));
    }
}
