//! Catálogo de agentes conocidos y detección de los instalados.

use std::ffi::OsStr;

use kuntur_usage::AgentKind;

/// Datos fijos de un agente para la UI y la detección.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentInfo {
    pub kind: AgentKind,
    pub name: &'static str,
    /// Nombres del ejecutable (minúsculas). El primero, sin extensión, es el comando a buscar en el PATH.
    pub exe_names: &'static [&'static str],
    /// Nombres de paquete o carpeta que identifican al agente cuando corre como script de Node
    /// (`node …\@openai\codex\bin\codex.js`).
    pub script_names: &'static [&'static str],
    /// Color de marca 0xRRGGBB.
    pub color: u32,
    pub letter: char,
}

pub const CATALOG: &[AgentInfo] = &[
    AgentInfo {
        kind: AgentKind::ClaudeCode,
        name: "Claude Code",
        exe_names: &["claude", "claude.exe"],
        script_names: &["claude", "claude-code"],
        color: 0xD97757,
        letter: 'C',
    },
    AgentInfo {
        kind: AgentKind::Codex,
        name: "Codex",
        exe_names: &["codex", "codex.exe"],
        script_names: &["codex"],
        color: 0x10A37F,
        letter: 'X',
    },
    AgentInfo {
        kind: AgentKind::OpenCode,
        name: "OpenCode",
        exe_names: &["opencode", "opencode.exe"],
        script_names: &["opencode", "opencode-ai"],
        color: 0xF5A623,
        letter: 'O',
    },
    AgentInfo {
        kind: AgentKind::Qwen,
        name: "Qwen Code",
        exe_names: &["qwen", "qwen.exe"],
        script_names: &["qwen", "qwen-code"],
        color: 0x615CED,
        letter: 'Q',
    },
    AgentInfo {
        kind: AgentKind::Gemini,
        name: "Gemini CLI",
        exe_names: &["gemini", "gemini.exe"],
        script_names: &["gemini", "gemini-cli"],
        color: 0x4285F4,
        letter: 'G',
    },
];

/// Datos de un agente del catálogo (`None` para `Other`).
pub fn info(kind: AgentKind) -> Option<&'static AgentInfo> {
    CATALOG.iter().find(|i| i.kind == kind)
}

/// Agentes instalados según el PATH del proceso.
pub fn installed() -> Vec<AgentKind> {
    std::env::var_os("PATH").map(|p| installed_in(&p)).unwrap_or_default()
}

/// Agentes con un ejecutable (`.exe`, `.cmd`, `.ps1`) en alguna carpeta de `path_var`.
pub fn installed_in(path_var: &OsStr) -> Vec<AgentKind> {
    let dirs: Vec<_> = std::env::split_paths(path_var).collect();
    CATALOG
        .iter()
        .filter(|agent| {
            let command = base_name(agent.exe_names[0]);
            dirs.iter().any(|dir| {
                [".exe", ".cmd", ".ps1"]
                    .iter()
                    .any(|ext| dir.join(format!("{command}{ext}")).is_file())
            })
        })
        .map(|agent| agent.kind)
        .collect()
}

/// Nombre en minúsculas sin `.exe`.
fn base_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).to_string()
}

/// Intérpretes con los que se distribuyen agentes como paquetes npm.
const SCRIPT_HOSTS: [&str; 3] = ["node", "bun", "deno"];

/// Qué agente es un proceso, por su nombre y línea de comando (`cmdline[0]` es el programa).
pub fn match_process(name: &str, cmdline: &[String]) -> Option<AgentKind> {
    let base = base_name(name);
    if let Some(agent) = CATALOG
        .iter()
        .find(|a| a.exe_names.iter().any(|e| base_name(e) == base))
    {
        return Some(agent.kind);
    }
    if !SCRIPT_HOSTS.contains(&base.as_str()) {
        return None;
    }
    // El script es el primer argumento que no es una opción del intérprete; se mira cada tramo de su
    // ruta (`…\@openai\codex\bin\codex.js` → "codex"), nunca los argumentos del script.
    let script = cmdline.iter().skip(1).find(|a| !a.starts_with('-'))?;
    script
        .split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .map(|segment| {
            let lower = segment.to_ascii_lowercase();
            match lower.rsplit_once('.') {
                Some((stem, "js" | "mjs" | "cjs" | "ts")) => stem.to_string(),
                _ => lower,
            }
        })
        .find_map(|segment| CATALOG.iter().find(|a| a.script_names.contains(&segment.as_str())))
        .map(|agent| agent.kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reconoce_ejecutables_por_nombre() {
        assert_eq!(
            match_process("claude.exe", &cmd(&["claude.exe"])),
            Some(AgentKind::ClaudeCode)
        );
        assert_eq!(match_process("Codex.EXE", &[]), Some(AgentKind::Codex));
        assert_eq!(match_process("opencode.exe", &[]), Some(AgentKind::OpenCode));
        assert_eq!(match_process("qwen", &[]), Some(AgentKind::Qwen));
        assert_eq!(match_process("gemini.exe", &[]), Some(AgentKind::Gemini));
        assert_eq!(match_process("pwsh.exe", &cmd(&["pwsh.exe", "-NoLogo"])), None);
        assert_eq!(match_process("claudette.exe", &[]), None);
    }

    #[test]
    fn reconoce_agentes_que_corren_en_node() {
        let codex = cmd(&[
            r"C:\Program Files\nodejs\node.exe",
            r"C:\Users\u\AppData\Roaming\npm\node_modules\@openai\codex\bin\codex.js",
            "--model",
            "x",
        ]);
        assert_eq!(match_process("node.exe", &codex), Some(AgentKind::Codex));
        let gemini = cmd(&[
            "node",
            "--no-warnings",
            "/usr/lib/node_modules/@google/gemini-cli/dist/index.js",
        ]);
        assert_eq!(match_process("node", &gemini), Some(AgentKind::Gemini));
        let claude = cmd(&["node.exe", r"C:\npm\node_modules\@anthropic-ai\claude-code\cli.js"]);
        assert_eq!(match_process("node.exe", &claude), Some(AgentKind::ClaudeCode));
        let qwen = cmd(&["node", "/x/node_modules/@qwen-code/qwen-code/cli.js"]);
        assert_eq!(match_process("node", &qwen), Some(AgentKind::Qwen));
        // Un script cualquiera, aunque un argumento se llame "codex", no es un agente.
        let otro = cmd(&["node.exe", r"C:\src\servidor\index.js", "codex"]);
        assert_eq!(match_process("node.exe", &otro), None);
        let carpeta_parecida = cmd(&["node.exe", r"C:\src\codex-tools\run.js"]);
        assert_eq!(match_process("node.exe", &carpeta_parecida), None);
        assert_eq!(match_process("node.exe", &cmd(&["node.exe"])), None);
    }

    #[test]
    fn catalogo_completo() {
        for kind in [
            AgentKind::ClaudeCode,
            AgentKind::Codex,
            AgentKind::OpenCode,
            AgentKind::Qwen,
            AgentKind::Gemini,
        ] {
            let i = info(kind).expect("en el catálogo");
            assert_eq!(i.kind, kind);
            assert!(i.letter.is_ascii_uppercase());
        }
        assert_eq!(info(AgentKind::Other), None);
    }

    #[test]
    fn detecta_instalados_en_el_path() {
        let base = std::env::temp_dir().join(format!("kuntur-agents-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (a, b) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("codex.cmd"), "").unwrap();
        std::fs::write(b.join("claude.exe"), "").unwrap();
        std::fs::write(b.join("gemini.ps1"), "").unwrap();
        std::fs::write(b.join("qwen.txt"), "").unwrap();
        std::fs::create_dir_all(b.join("opencode.exe")).unwrap(); // una carpeta no cuenta
        let path = std::env::join_paths([&a, &b, &base.join("no-existe")]).unwrap();
        let found = installed_in(&path);
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(found, vec![AgentKind::ClaudeCode, AgentKind::Codex, AgentKind::Gemini]);
    }
}
