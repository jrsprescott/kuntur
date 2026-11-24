//! Línea que se escribe en la terminal para abrir un agente ("clic en un agente instalado", §3):
//! el comando y sus argumentos con comillas seguras según el shell de la sesión.

/// Familia del shell que recibe la línea.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellFlavor {
    /// PowerShell 7 o Windows PowerShell (el predeterminado de Kuntur).
    PowerShell,
    Cmd,
    /// bash, zsh, sh, WSL…
    Posix,
}

impl ShellFlavor {
    /// Según el programa del shell (`None` = el predeterminado, PowerShell).
    pub fn from_program(program: Option<&str>) -> Self {
        let Some(program) = program else {
            return ShellFlavor::PowerShell;
        };
        let file = program.trim().rsplit(['\\', '/']).next().unwrap_or(program);
        let base = file.to_ascii_lowercase();
        let base = base.strip_suffix(".exe").unwrap_or(&base);
        match base {
            "pwsh" | "powershell" => ShellFlavor::PowerShell,
            "cmd" => ShellFlavor::Cmd,
            _ => ShellFlavor::Posix,
        }
    }
}

/// Comando + argumentos listos para escribir (sin el Enter). Lo que no es un "token simple" va
/// entre comillas, para que el shell no interprete nada (variables, `;`, `&`, `|`…).
pub fn launch_line(command: &str, args: &[String], shell: ShellFlavor) -> String {
    let program = quote(command, shell);
    let mut parts = Vec::with_capacity(args.len() + 2);
    // En PowerShell, un programa entre comillas es solo un texto: hace falta `&` para ejecutarlo.
    if shell == ShellFlavor::PowerShell && program != command {
        parts.push("&".to_string());
    }
    parts.push(program);
    parts.extend(args.iter().map(|a| quote(a, shell)));
    parts.join(" ")
}

fn is_plain(text: &str, extra: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./\\:".contains(c) || extra.contains(c))
}

/// Un argumento listo para el shell: tal cual si es un "token simple"; si no, entre comillas.
pub fn quote_arg(text: &str, shell: ShellFlavor) -> String {
    quote(text, shell)
}

fn quote(text: &str, shell: ShellFlavor) -> String {
    match shell {
        ShellFlavor::PowerShell if is_plain(text, "=") => text.to_string(),
        // Entre comillas simples PowerShell no expande nada; la comilla simple se duplica.
        ShellFlavor::PowerShell => format!("'{}'", text.replace('\'', "''")),
        ShellFlavor::Cmd if is_plain(text, "+@") => text.to_string(),
        // cmd no tiene escape para `"` dentro de comillas: se quita. `%VAR%` se sigue expandiendo
        // en la línea interactiva (cmd no permite evitarlo); es raro en argumentos de agentes.
        ShellFlavor::Cmd => format!("\"{}\"", text.replace('"', "")),
        ShellFlavor::Posix if is_plain(text, "=+@,%") => text.to_string(),
        ShellFlavor::Posix => format!("'{}'", text.replace('\'', r"'\''")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn familia_del_shell() {
        assert_eq!(ShellFlavor::from_program(None), ShellFlavor::PowerShell);
        assert_eq!(ShellFlavor::from_program(Some("pwsh.exe")), ShellFlavor::PowerShell);
        assert_eq!(
            ShellFlavor::from_program(Some(r"C:\Windows\System32\WindowsPowerShell\v1.0\PowerShell.exe")),
            ShellFlavor::PowerShell
        );
        assert_eq!(ShellFlavor::from_program(Some("cmd.exe")), ShellFlavor::Cmd);
        assert_eq!(ShellFlavor::from_program(Some("wsl.exe")), ShellFlavor::Posix);
        assert_eq!(ShellFlavor::from_program(Some("/bin/bash")), ShellFlavor::Posix);
    }

    #[test]
    fn comandos_simples_sin_comillas() {
        assert_eq!(launch_line("claude", &[], ShellFlavor::PowerShell), "claude");
        assert_eq!(
            launch_line(
                "codex",
                &args(&["--model", "gpt-5", "-c", "x=1"]),
                ShellFlavor::PowerShell
            ),
            "codex --model gpt-5 -c x=1"
        );
        assert_eq!(
            launch_line("antigravity", &args(&["."]), ShellFlavor::Cmd),
            "antigravity ."
        );
    }

    #[test]
    fn powershell_comillas_y_ampersand() {
        assert_eq!(
            launch_line(
                r"C:\Program Files\Agente\agente.exe",
                &args(&["hola mundo"]),
                ShellFlavor::PowerShell
            ),
            r"& 'C:\Program Files\Agente\agente.exe' 'hola mundo'"
        );
        // Nada se expande ni se encadena.
        assert_eq!(
            launch_line(
                "aider",
                &args(&["$env:SECRETO", "a;b", "it's"]),
                ShellFlavor::PowerShell
            ),
            "aider '$env:SECRETO' 'a;b' 'it''s'"
        );
    }

    #[test]
    fn cmd_y_posix() {
        assert_eq!(
            launch_line(r"C:\Program Files\a.exe", &args(&["x & y", "q\"t"]), ShellFlavor::Cmd),
            r#""C:\Program Files\a.exe" "x & y" "qt""#
        );
        assert_eq!(
            launch_line("aider", &args(&["$HOME", "it's", "--x=1"]), ShellFlavor::Posix),
            r"aider '$HOME' 'it'\''s' --x=1"
        );
        assert_eq!(launch_line("aider", &args(&[""]), ShellFlavor::Posix), "aider ''");
    }
}
