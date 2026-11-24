//! Comandos que Kuntur manda al servidor por un canal `exec`. Nunca modifican el servidor: solo
//! abren un shell en una carpeta o preguntan qué agentes hay instalados.

/// Agentes de IA por línea de comandos que se buscan en el servidor (§5).
pub const REMOTE_AGENTS: [&str; 10] = [
    "claude",
    "codex",
    "opencode",
    "kimi",
    "qwen",
    "gemini",
    "deepseek",
    "crush",
    "aider",
    "cursor-agent",
];

/// Marca de cada línea de la respuesta: el resto de la salida (mensajes del `.profile`, avisos) se
/// ignora.
const AGENT_MARK: &str = "kuntur-agente:";

/// Comillas simples de `sh`: `'it'\''s'`.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Shell interactivo del usuario en una carpeta (terminal acoplada o reconexión): si la carpeta ya
/// no existe, se abre en la de inicio.
pub fn shell_in_dir(dir: &str) -> String {
    format!(
        "cd -- {} 2>/dev/null || cd; exec \"${{SHELL:-/bin/sh}}\" -l",
        sh_quote(dir)
    )
}

/// Busca los agentes con `command -v` en un shell de inicio de sesión (que carga el `PATH` de
/// `~/.profile`, donde suelen quedar `~/.local/bin` y los de npm).
pub fn detect_agents_command() -> String {
    let list = REMOTE_AGENTS.join(" ");
    let script =
        format!("for a in {list}; do command -v \"$a\" >/dev/null 2>&1 && echo \"{AGENT_MARK}$a\"; done; true");
    format!("\"${{SHELL:-/bin/sh}}\" -lc {} </dev/null", sh_quote(&script))
}

/// Agentes encontrados en la salida de [`detect_agents_command`], en el orden de la lista.
pub fn parse_agents(output: &str) -> Vec<String> {
    let found: Vec<&str> = output
        .lines()
        .filter_map(|l| l.trim().strip_prefix(AGENT_MARK))
        .collect();
    REMOTE_AGENTS
        .iter()
        .filter(|a| found.contains(a))
        .map(|a| a.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comillas_de_sh() {
        assert_eq!(sh_quote("/var/www"), "'/var/www'");
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        assert_eq!(sh_quote("$(rm -rf /)"), "'$(rm -rf /)'");
    }

    #[test]
    fn shell_en_una_carpeta() {
        assert_eq!(
            shell_in_dir("/var/www/mi sitio"),
            "cd -- '/var/www/mi sitio' 2>/dev/null || cd; exec \"${SHELL:-/bin/sh}\" -l"
        );
    }

    #[test]
    fn comando_de_agentes() {
        let cmd = detect_agents_command();
        assert!(cmd.starts_with("\"${SHELL:-/bin/sh}\" -lc '"));
        for a in REMOTE_AGENTS {
            assert!(cmd.contains(a));
        }
    }

    #[test]
    fn parsea_solo_las_lineas_marcadas() {
        let out = "Bienvenido a Ubuntu\nkuntur-agente:codex\r\nkuntur-agente:claude\nclaude\nkuntur-agente:otro\n";
        assert_eq!(parse_agents(out), vec!["claude", "codex"]);
        assert!(parse_agents("").is_empty());
    }
}
