//! Lanzar un agente con el MCP de la sesión: qué archivos escribir en la carpeta temporal de la
//! sesión y qué línea escribir en la terminal. Nunca se toca la configuración global del usuario
//! (`~/.claude.json`, `~/.codex/config.toml`, `~/.config/opencode/…`).

use std::path::{Path, PathBuf};

use kuntur_agents::{ShellFlavor, launch_line};

use crate::catalog::{ResolvedKind, ResolvedServer};
use crate::generate::{self, AgentOptions, Cli};

/// Archivo que hay que escribir antes de mandar la línea.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: PathBuf,
    pub contents: String,
    /// Con BOM UTF-8 (scripts de Windows PowerShell 5.1).
    pub bom: bool,
}

impl GeneratedFile {
    pub fn write(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut bytes = Vec::with_capacity(self.contents.len() + 3);
        if self.bom {
            bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        }
        bytes.extend_from_slice(self.contents.as_bytes());
        std::fs::write(&self.path, bytes)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchPlan {
    pub files: Vec<GeneratedFile>,
    /// Lo que se escribe en la terminal (sin el Enter).
    pub line: String,
    /// Lo que no se pudo traducir (se muestra como aviso).
    pub warnings: Vec<String>,
}

pub struct LaunchInput<'a> {
    /// `None` = un agente sin formato MCP conocido (se lanza igual, sin MCP).
    pub cli: Option<Cli>,
    pub command: &'a str,
    pub args: &'a [String],
    pub shell: ShellFlavor,
    pub servers: &'a [ResolvedServer],
    pub options: &'a AgentOptions,
    pub session_dir: &'a Path,
    /// Claude Code: `--strict-mcp-config`.
    pub exclusive: bool,
    pub opencode_v2: bool,
    /// OpenCode: claves extra para su configuración de la sesión (ajustes v0.2.1: proveedor y
    /// modelo de una IA local). Se combinan con los MCP en el mismo `opencode.json`.
    pub opencode_extra: Option<&'a serde_json::Value>,
    /// ¿El comando es un `.cmd`/`.bat` (como `npx` en Windows)? Entonces va con `cmd /c`.
    pub is_batch: &'a dyn Fn(&str) -> bool,
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Envuelve los comandos `.cmd`/`.bat` con `cmd /c`.
fn prepare(servers: &[ResolvedServer], is_batch: &dyn Fn(&str) -> bool) -> Vec<ResolvedServer> {
    servers
        .iter()
        .cloned()
        .map(|mut s| {
            if let ResolvedKind::Stdio { command, args, .. } = &mut s.kind
                && is_batch(command)
            {
                let (c, a) = generate::wrap_batch(command, args);
                *command = c;
                *args = a;
            }
            s
        })
        .collect()
}

fn env_script(dir: &Path, shell: ShellFlavor, vars: &[(String, String)]) -> GeneratedFile {
    match shell {
        ShellFlavor::Cmd => GeneratedFile {
            path: dir.join("codex-entorno.cmd"),
            contents: vars
                .iter()
                .map(|(k, v)| format!("@set \"{k}={}\"\r\n", v.replace('"', "")))
                .collect(),
            bom: false,
        },
        _ => GeneratedFile {
            path: dir.join("codex-entorno.ps1"),
            contents: vars
                .iter()
                .map(|(k, v)| format!("$env:{k} = {}\r\n", ps_quote(v)))
                .collect(),
            bom: true,
        },
    }
}

/// Antes y después de la línea del agente: poner y quitar variables (o cargar un script).
fn wrap_line(shell: ShellFlavor, before: &[String], main: String, after: &[String]) -> String {
    let sep = if shell == ShellFlavor::Cmd { " & " } else { "; " };
    let mut parts: Vec<String> = before.to_vec();
    parts.push(main);
    parts.extend(after.iter().cloned());
    parts.join(sep)
}

fn unset(shell: ShellFlavor, names: &[String]) -> Vec<String> {
    if names.is_empty() {
        return Vec::new();
    }
    match shell {
        ShellFlavor::Cmd => names.iter().map(|n| format!("set \"{n}=\"")).collect(),
        _ => {
            let list: Vec<String> = names.iter().map(|n| format!("Env:{n}")).collect();
            vec![format!("Remove-Item {} -ErrorAction SilentlyContinue", list.join(", "))]
        }
    }
}

pub fn plan_launch(input: &LaunchInput) -> LaunchPlan {
    let mut plan = LaunchPlan::default();
    let mut args: Vec<String> = input.args.to_vec();
    let Some(cli) = input.cli else {
        if !input.servers.is_empty() {
            plan.warnings
                .push("Este agente no tiene un formato MCP conocido: se abre sin los MCP de la sesión".into());
        }
        plan.line = launch_line(input.command, &args, input.shell);
        return plan;
    };
    let servers = if input.shell == ShellFlavor::Posix && !input.servers.is_empty() {
        plan.warnings
            .push("Los MCP de la sesión solo se pasan en PowerShell y cmd (no en shells POSIX)".into());
        Vec::new()
    } else {
        prepare(input.servers, input.is_batch)
    };
    let (opt_args, mut warnings) = generate::option_args(cli, input.options, input.opencode_v2);
    plan.warnings.append(&mut warnings);
    let dir = input.session_dir;
    let mut before: Vec<String> = Vec::new();
    let mut after: Vec<String> = Vec::new();

    match cli {
        Cli::Claude => {
            if !servers.is_empty() || input.exclusive {
                let path = dir.join("claude.json");
                plan.files.push(GeneratedFile {
                    path: path.clone(),
                    contents: generate::claude_json(&servers),
                    bom: false,
                });
                args.extend(["--mcp-config".to_string(), path.to_string_lossy().into_owned()]);
                if input.exclusive {
                    args.push("--strict-mcp-config".into());
                }
            }
        }
        Cli::Codex => {
            let o = generate::codex_overrides(&servers);
            args.extend(o.args);
            if !o.secret_env.is_empty() {
                let script = env_script(dir, input.shell, &o.secret_env);
                before.push(match input.shell {
                    ShellFlavor::Cmd => format!("call \"{}\"", script.path.display()),
                    _ => format!(". {}", ps_quote(&script.path.to_string_lossy())),
                });
                let names: Vec<String> = o.secret_env.iter().map(|(k, _)| k.clone()).collect();
                after = unset(input.shell, &names);
                plan.files.push(script);
            }
        }
        Cli::OpenCode => {
            let permissions = input.options.permissions;
            if let Some(json) =
                generate::opencode_json_with(&servers, input.opencode_v2, permissions, input.opencode_extra)
            {
                let path = dir.join("opencode.json");
                let p = path.to_string_lossy().into_owned();
                match input.shell {
                    ShellFlavor::Cmd => before.push(format!("set \"OPENCODE_CONFIG={p}\"")),
                    ShellFlavor::Posix => {}
                    ShellFlavor::PowerShell => before.push(format!("$env:OPENCODE_CONFIG = {}", ps_quote(&p))),
                }
                after = unset(input.shell, &["OPENCODE_CONFIG".to_string()]);
                if input.shell == ShellFlavor::Posix {
                    // `VAR=valor programa`: solo para ese comando.
                    let main = launch_line(input.command, &[args, opt_args].concat(), input.shell);
                    plan.line = format!("OPENCODE_CONFIG='{}' {main}", p.replace('\'', r"'\''"));
                    plan.files.push(GeneratedFile {
                        path,
                        contents: json,
                        bom: false,
                    });
                    return plan;
                }
                plan.files.push(GeneratedFile {
                    path,
                    contents: json,
                    bom: false,
                });
            }
        }
    }
    args.extend(opt_args);
    let main = launch_line(input.command, &args, input.shell);
    plan.line = wrap_line(input.shell, &before, main, &after);
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuntur_config::PermissionMode;

    fn servers() -> Vec<ResolvedServer> {
        vec![
            ResolvedServer {
                name: "db".into(),
                kind: ResolvedKind::Stdio {
                    command: "npx".into(),
                    args: vec!["-y".into(), "srv".into()],
                    env: Default::default(),
                    secret_env: [("DB_PASSWORD".to_string(), "s3creto".to_string())].into(),
                },
            },
            ResolvedServer {
                name: "local".into(),
                kind: ResolvedKind::Stdio {
                    command: r"C:\h\srv.exe".into(),
                    args: vec![],
                    env: Default::default(),
                    secret_env: Default::default(),
                },
            },
        ]
    }

    fn input<'a>(
        cli: Option<Cli>,
        shell: ShellFlavor,
        servers: &'a [ResolvedServer],
        options: &'a AgentOptions,
        exclusive: bool,
    ) -> LaunchInput<'a> {
        LaunchInput {
            cli,
            command: cli.map_or("qwen", |c| match c {
                Cli::Claude => "claude",
                Cli::Codex => "codex",
                Cli::OpenCode => "opencode",
            }),
            args: &[],
            shell,
            servers,
            options,
            session_dir: Path::new(r"C:\Datos\Kuntur\mcp\10-1"),
            exclusive,
            opencode_v2: false,
            opencode_extra: None,
            is_batch: &|c: &str| c == "npx",
        }
    }

    #[test]
    fn sin_mcp_ni_opciones_la_linea_no_cambia() {
        let none = AgentOptions::default();
        for cli in [Cli::Claude, Cli::Codex, Cli::OpenCode] {
            let p = plan_launch(&input(Some(cli), ShellFlavor::PowerShell, &[], &none, false));
            assert!(p.files.is_empty());
            assert!(!p.line.contains(' '), "{}", p.line);
        }
    }

    #[test]
    fn claude_archivo_y_flags() {
        let s = servers();
        let o = AgentOptions {
            model: Some("opus".into()),
            ..Default::default()
        };
        let p = plan_launch(&input(Some(Cli::Claude), ShellFlavor::PowerShell, &s, &o, true));
        assert_eq!(p.files.len(), 1);
        assert!(p.files[0].path.ends_with("claude.json"));
        let v: serde_json::Value = serde_json::from_str(&p.files[0].contents).unwrap();
        // npx (un .cmd) va con `cmd /c`.
        assert_eq!(v["mcpServers"]["db"]["command"], "cmd");
        assert_eq!(v["mcpServers"]["db"]["args"][1], "npx");
        assert_eq!(v["mcpServers"]["local"]["command"], r"C:\h\srv.exe");
        assert_eq!(
            p.line,
            r"claude --mcp-config C:\Datos\Kuntur\mcp\10-1\claude.json --strict-mcp-config --model opus"
        );
        assert!(!p.line.contains("s3creto"));
    }

    #[test]
    fn codex_secretos_por_script_y_se_quitan_despues() {
        let s = servers();
        let none = AgentOptions::default();
        let p = plan_launch(&input(Some(Cli::Codex), ShellFlavor::PowerShell, &s, &none, false));
        assert!(!p.line.contains("s3creto"), "{}", p.line);
        assert!(
            p.line
                .starts_with(r". 'C:\Datos\Kuntur\mcp\10-1\codex-entorno.ps1'; codex -c ")
        );
        assert!(
            p.line
                .ends_with("; Remove-Item Env:DB_PASSWORD -ErrorAction SilentlyContinue")
        );
        assert!(p.line.contains("env_vars=[''DB_PASSWORD'']"), "{}", p.line);
        assert_eq!(p.files.len(), 1);
        assert_eq!(p.files[0].contents, "$env:DB_PASSWORD = 's3creto'\r\n");
        assert!(p.files[0].bom);

        let p = plan_launch(&input(Some(Cli::Codex), ShellFlavor::Cmd, &s, &none, false));
        assert!(
            p.line
                .starts_with(r#"call "C:\Datos\Kuntur\mcp\10-1\codex-entorno.cmd" & codex -c "#)
        );
        assert!(p.line.ends_with(r#" & set "DB_PASSWORD=""#));
        assert_eq!(p.files[0].contents, "@set \"DB_PASSWORD=s3creto\"\r\n");
    }

    #[test]
    fn opencode_variable_solo_para_ese_comando() {
        let s = servers();
        let o = AgentOptions {
            permissions: Some(PermissionMode::Plan),
            ..Default::default()
        };
        let p = plan_launch(&input(Some(Cli::OpenCode), ShellFlavor::PowerShell, &s, &o, false));
        assert_eq!(
            p.line,
            r"$env:OPENCODE_CONFIG = 'C:\Datos\Kuntur\mcp\10-1\opencode.json'; opencode --agent plan; Remove-Item Env:OPENCODE_CONFIG -ErrorAction SilentlyContinue"
        );
        let p = plan_launch(&input(Some(Cli::OpenCode), ShellFlavor::Cmd, &s, &o, false));
        assert_eq!(
            p.line,
            r#"set "OPENCODE_CONFIG=C:\Datos\Kuntur\mcp\10-1\opencode.json" & opencode --agent plan & set "OPENCODE_CONFIG=""#
        );
    }

    #[test]
    fn agentes_sin_formato_y_shells_posix() {
        let s = servers();
        let none = AgentOptions::default();
        let p = plan_launch(&input(None, ShellFlavor::PowerShell, &s, &none, false));
        assert_eq!(p.line, "qwen");
        assert_eq!(p.warnings.len(), 1);
        let p = plan_launch(&input(Some(Cli::Claude), ShellFlavor::Posix, &s, &none, false));
        assert_eq!(p.line, "claude");
        assert_eq!(p.warnings.len(), 1);
    }

    #[test]
    fn escribe_con_y_sin_bom() {
        let dir = std::env::temp_dir().join(format!("kuntur-mcp-launch-{}", std::process::id()));
        let f = GeneratedFile {
            path: dir.join("a.ps1"),
            contents: "x".into(),
            bom: true,
        };
        f.write().unwrap();
        assert_eq!(std::fs::read(&f.path).unwrap(), [0xEF, 0xBB, 0xBF, b'x']);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
