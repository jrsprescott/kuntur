//! Comandos que sugiere un agente: los bloques de código de shell de su último mensaje
//! (```bash, ```sh, ```powershell…). "Enviar a terminal" los escribe en la terminal acoplada **sin
//! ejecutarlos**; los peligrosos (`sudo`, `rm -rf`…) se marcan.

use crate::launch::ShellFlavor;

/// Sugerencias que se guardan (las últimas del mensaje).
pub const MAX_SUGGESTIONS: usize = 5;

const SHELL_LANGS: &[&str] = &[
    "bash",
    "sh",
    "shell",
    "zsh",
    "console",
    "shell-session",
    "powershell",
    "pwsh",
    "ps1",
    "ps",
    "cmd",
    "bat",
    "batch",
];

/// Quita el prompt de una línea de consola (`$ npm test`, `PS C:\x> dir`, `> dir`).
fn strip_prompt(line: &str) -> &str {
    let t = line.trim_start();
    if let Some(rest) = t.strip_prefix("$ ") {
        return rest;
    }
    if t.starts_with("PS ")
        && let Some(i) = t.find("> ")
    {
        return &t[i + 2..];
    }
    t.strip_prefix("> ").unwrap_or(line)
}

/// Bloques de shell del texto, en orden, sin repetir; como mucho los últimos [`MAX_SUGGESTIONS`].
pub fn suggested_commands(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let Some(info) = trimmed.strip_prefix("```") else {
            continue;
        };
        let lang = info.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        let mut body = Vec::new();
        for l in lines.by_ref() {
            if l.trim_start().starts_with("```") {
                break;
            }
            body.push(l);
        }
        if !SHELL_LANGS.contains(&lang.as_str()) {
            continue;
        }
        let console = matches!(lang.as_str(), "console" | "shell-session");
        let cmd: Vec<&str> = body
            .iter()
            .map(|l| l.trim_end())
            // En una transcripción de consola, solo las líneas con prompt son comandos.
            .filter(|l| !console || l.trim_start().starts_with("$ ") || l.trim_start().starts_with("PS "))
            .map(strip_prompt)
            .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
            .collect();
        let cmd = cmd.join("\n").trim().to_string();
        if !cmd.is_empty() && !out.contains(&cmd) {
            out.push(cmd);
        }
    }
    let skip = out.len().saturating_sub(MAX_SUGGESTIONS);
    out.drain(..skip);
    out
}

/// ¿El comando merece un aviso antes de enviarlo? (`sudo`, borrados recursivos, formatear…)
pub fn is_risky(cmd: &str) -> bool {
    let c = format!(
        " {} ",
        cmd.to_ascii_lowercase().replace(['\n', '\t', ';', '&', '|'], " ")
    );
    const RISKY: &[&str] = &[
        " sudo ",
        " rm -rf ",
        " rm -fr ",
        " rm -r ",
        " remove-item ",
        " rd /s ",
        " rmdir /s ",
        " del /s ",
        " del /q ",
        " format ",
        " mkfs",
        " dd if=",
        " git push --force",
        " git push -f ",
        " git reset --hard",
        " git clean -",
        " shutdown ",
        " chmod -r 777",
        " drop table ",
        " drop database ",
    ];
    RISKY.iter().any(|r| c.contains(r))
}

/// Un comando de varias líneas en una sola, para escribirlo en la terminal sin que se ejecute nada
/// hasta el Enter del usuario (sin bracketed paste, cada salto de línea sería un Enter). Las
/// continuaciones (`\` en sh, `` ` `` en PowerShell, `^` en cmd) se unen con un espacio; el resto, con
/// el separador del shell (`;`, o `&` en cmd).
pub fn one_line(cmd: &str, shell: ShellFlavor) -> String {
    let (cont, sep) = match shell {
        ShellFlavor::PowerShell => ('`', "; "),
        ShellFlavor::Cmd => ('^', " & "),
        ShellFlavor::Posix => ('\\', "; "),
    };
    let mut out = String::new();
    let mut joined = true;
    for line in cmd.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if !out.is_empty() {
            out.push_str(if joined { " " } else { sep });
        }
        match line.strip_suffix(cont) {
            Some(rest) => {
                out.push_str(rest.trim_end());
                joined = true;
            }
            None => {
                out.push_str(line);
                joined = false;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varias_lineas_en_una_sin_ejecutar() {
        assert_eq!(
            one_line("npm install\nnpm test", ShellFlavor::PowerShell),
            "npm install; npm test"
        );
        assert_eq!(
            one_line("npm install\nnpm test", ShellFlavor::Cmd),
            "npm install & npm test"
        );
        assert_eq!(
            one_line("docker run \\\n  -it ubuntu\nls", ShellFlavor::Posix),
            "docker run -it ubuntu; ls"
        );
        assert_eq!(
            one_line("Get-ChildItem `\n  -Recurse", ShellFlavor::PowerShell),
            "Get-ChildItem -Recurse"
        );
        assert_eq!(one_line("  dir  ", ShellFlavor::Cmd), "dir");
    }

    #[test]
    fn extrae_bloques_de_shell() {
        let msg = "Listo. Para probar:\n\n```bash\nnpm install\nnpm test\n```\n\nO en Windows:\n```powershell\n# comentario\nGet-ChildItem\n```\n\nCódigo:\n```rust\nfn main() {}\n```\n";
        assert_eq!(suggested_commands(msg), ["npm install\nnpm test", "Get-ChildItem"]);
    }

    #[test]
    fn prompts_de_consola_repetidos_y_limite() {
        let msg = "```console\n$ cargo build\n   Compiling x\n$ cargo test\n```\n```sh\ncargo build\n```\n```sh\ncargo build\n```";
        assert_eq!(suggested_commands(msg), ["cargo build\ncargo test", "cargo build"]);
        assert_eq!(suggested_commands("```pwsh\nPS C:\\x> dir\n```"), ["dir"]);
        let many: String = (0..9).map(|i| format!("```sh\necho {i}\n```\n")).collect();
        let s = suggested_commands(&many);
        assert_eq!(s.len(), MAX_SUGGESTIONS);
        assert_eq!(s.last().map(String::as_str), Some("echo 8"));
        assert!(suggested_commands("sin bloques").is_empty());
        // Bloque sin cerrar: igual cuenta hasta el final.
        assert_eq!(suggested_commands("```bash\nls -la"), ["ls -la"]);
        // Sin lenguaje: no se sabe si es shell.
        assert!(suggested_commands("```\nls\n```").is_empty());
    }

    #[test]
    fn comandos_peligrosos() {
        assert!(is_risky("sudo apt upgrade -y"));
        assert!(is_risky("rm -rf node_modules"));
        assert!(is_risky("cd x && Remove-Item -Recurse dist"));
        assert!(is_risky("git push --force origin main"));
        assert!(is_risky("git reset --hard HEAD~1"));
        assert!(!is_risky("npm test"));
        assert!(!is_risky("git push origin main"));
        assert!(!is_risky("echo formatear"));
    }
}
