//! "Compartir ▸" del menú del árbol (ajustes v0.2): enlace de GitHub de una ruta y dirección
//! `usuario@host:ruta` de una ruta remota. Lógica pura: la app consulta git aparte.

/// `https://github.com/dueño/repo` a partir de la URL del remoto (`https://github.com/d/r.git`,
/// `git@github.com:d/r.git`, `ssh://git@github.com/d/r`). `None` si no es de GitHub.
pub fn github_repo_url(remote: &str) -> Option<String> {
    let r = remote.trim();
    let rest = r
        .strip_prefix("https://github.com/")
        .or_else(|| r.strip_prefix("http://github.com/"))
        .or_else(|| r.strip_prefix("git@github.com:"))
        .or_else(|| r.strip_prefix("ssh://git@github.com/"))
        .or_else(|| {
            // https://usuario@github.com/d/r.git
            let after = r.strip_prefix("https://")?;
            let (_, path) = after.split_once("@github.com/")?;
            Some(path)
        })?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut parts = rest.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!("https://github.com/{owner}/{repo}"))
}

/// Enlace de GitHub a `rel_path` (relativa a la raíz del repositorio, con `/` o `\`) en la rama
/// `branch`: `…/tree/rama/ruta` para carpetas y `…/blob/rama/ruta` para archivos.
pub fn github_link(remote: &str, branch: &str, rel_path: &str, is_dir: bool) -> Option<String> {
    let repo = github_repo_url(remote)?;
    let branch = branch.trim();
    if branch.is_empty() || branch == "HEAD" {
        return None;
    }
    let rel = rel_path.replace('\\', "/");
    let rel = rel.trim_matches('/');
    let rel = if rel == "." { "" } else { rel };
    let kind = if is_dir { "tree" } else { "blob" };
    let encoded: Vec<String> = rel.split('/').filter(|s| !s.is_empty()).map(encode_segment).collect();
    Some(if encoded.is_empty() {
        format!("{repo}/{kind}/{branch}")
    } else {
        format!("{repo}/{kind}/{branch}/{}", encoded.join("/"))
    })
}

/// Escapa lo que no puede ir tal cual en un segmento de URL (espacios, `#`, `?`, `%`, no ASCII).
fn encode_segment(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'+' | b'@' | b'(' | b')' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `usuario@host:ruta` (como `scp`) a partir de la etiqueta del sistema remoto (`usuario@host` o
/// `usuario@host:puerto`; el puerto no va en esta forma).
pub fn remote_address(label: &str, path: &str) -> String {
    let host = match label.rsplit_once(':') {
        Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => h,
        _ => label,
    };
    format!("{host}:{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repositorio_de_github_desde_el_remoto() {
        for remote in [
            "https://github.com/demo/kuntur.git",
            "https://github.com/demo/kuntur",
            "https://github.com/demo/kuntur/",
            "git@github.com:demo/kuntur.git",
            "ssh://git@github.com/demo/kuntur.git",
            "https://alguien@github.com/demo/kuntur.git",
        ] {
            assert_eq!(
                github_repo_url(remote).as_deref(),
                Some("https://github.com/demo/kuntur"),
                "{remote}"
            );
        }
        assert_eq!(github_repo_url("https://gitlab.com/demo/kuntur.git"), None);
        assert_eq!(github_repo_url("https://github.com/demo"), None);
    }

    #[test]
    fn enlace_a_carpeta_y_a_archivo() {
        let r = "git@github.com:demo/kuntur.git";
        assert_eq!(
            github_link(r, "main", "crates/kuntur-files", true).as_deref(),
            Some("https://github.com/demo/kuntur/tree/main/crates/kuntur-files")
        );
        assert_eq!(
            github_link(r, "main", r"docs\plan de trabajo.md", false).as_deref(),
            Some("https://github.com/demo/kuntur/blob/main/docs/plan%20de%20trabajo.md")
        );
        assert_eq!(
            github_link(r, "fase/f3", ".", true).as_deref(),
            Some("https://github.com/demo/kuntur/tree/fase/f3")
        );
        assert_eq!(github_link(r, "HEAD", "src", true), None, "sin rama (HEAD suelto)");
        assert_eq!(github_link("https://ejemplo.test/x.git", "main", "src", true), None);
    }

    #[test]
    fn direccion_remota_estilo_scp() {
        assert_eq!(
            remote_address("deploy@srv.test", "/var/www"),
            "deploy@srv.test:/var/www"
        );
        assert_eq!(
            remote_address("demo@127.0.0.1:2222", "/home/demo"),
            "demo@127.0.0.1:/home/demo"
        );
    }
}
