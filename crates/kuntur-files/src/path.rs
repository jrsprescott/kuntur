//! Rutas como texto, en estilo Windows (local) o POSIX (servidor por SFTP), y los nombres que
//! inventa Kuntur: el de un archivo en conflicto y la carpeta temporal "para el agente".

use std::path::Path;

/// Cómo se escriben las rutas de un sistema de archivos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathStyle {
    /// Disco local (Windows: `C:\…`, `\\wsl$\…`).
    Native,
    /// Servidor remoto (`/home/usuario/…`).
    Posix,
}

impl PathStyle {
    /// `dir` + `name`.
    pub fn join(self, dir: &str, name: &str) -> String {
        match self {
            PathStyle::Native => Path::new(dir).join(name).to_string_lossy().into_owned(),
            PathStyle::Posix => posix_join(dir, name),
        }
    }

    /// Carpeta que contiene `path` (`None` en la raíz).
    pub fn parent(self, path: &str) -> Option<String> {
        match self {
            PathStyle::Native => Path::new(path).parent().map(|p| p.to_string_lossy().into_owned()),
            PathStyle::Posix => posix_parent(path),
        }
    }

    /// Último componente (`None` en la raíz).
    pub fn file_name(self, path: &str) -> Option<String> {
        match self {
            PathStyle::Native => Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()),
            PathStyle::Posix => {
                let t = path.trim_end_matches('/');
                let name = t.rsplit('/').next().unwrap_or(t);
                (!name.is_empty()).then(|| name.to_string())
            }
        }
    }
}

/// Une una carpeta POSIX y un nombre sin duplicar `/`.
pub fn posix_join(dir: &str, name: &str) -> String {
    let name = name.trim_start_matches('/');
    if dir.is_empty() {
        name.to_string()
    } else if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Carpeta superior de una ruta POSIX: `/a/b` → `/a`, `/a` → `/`, `/` → `None`.
pub fn posix_parent(path: &str) -> Option<String> {
    let t = path.trim_end_matches('/');
    if t.is_empty() {
        return None;
    }
    match t.rfind('/') {
        Some(0) => Some("/".into()),
        Some(i) => Some(t[..i].to_string()),
        None => None,
    }
}

/// Nombre libre para `name` cuando ya existe en el destino: `informe (1).pdf`, `informe (2).pdf`…
/// Respeta las extensiones dobles conocidas (`datos (1).tar.gz`) y los archivos ocultos
/// (`.bashrc (1)`). `taken` dice si un nombre ya está ocupado.
pub fn conflict_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let (stem, ext) = split_ext(name);
    for n in 1..10_000 {
        let candidate = format!("{stem} ({n}){ext}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    format!("{stem} ({}){ext}", std::process::id())
}

fn split_ext(name: &str) -> (&str, &str) {
    let lower = name.to_ascii_lowercase();
    for double in [".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst"] {
        if lower.len() > double.len() && lower.ends_with(double) {
            let at = name.len() - double.len();
            return (&name[..at], &name[at..]);
        }
    }
    match name.rfind('.') {
        // `.bashrc`: el punto inicial no separa una extensión.
        Some(0) | None => (name, ""),
        Some(i) => (&name[..i], &name[i..]),
    }
}

/// Carpeta temporal en el servidor para "subir archivos para que el agente los lea" (§7):
/// `/tmp/kuntur-<sesión>`, con el nombre de la sesión reducido a `[a-z0-9-]`.
pub fn agent_dir(session_name: &str) -> String {
    let mut slug = String::new();
    for ch in session_name.chars().flat_map(char::to_lowercase) {
        let ch = match ch {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ñ' => 'n',
            c => c,
        };
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_matches('-').chars().take(40).collect();
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "/tmp/kuntur-sesion".into()
    } else {
        format!("/tmp/kuntur-{slug}")
    }
}

/// Rutas para escribir en un shell POSIX, cada una entre comillas simples y separadas por espacio.
pub fn posix_quote_all(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| format!("'{}'", p.replace('\'', r"'\''")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Etiqueta del panel: "local" o "remoto · usuario@host".
pub fn location_label(remote: Option<&str>) -> String {
    match remote {
        None => "local".into(),
        Some(dest) => format!("remoto · {dest}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rutas_posix() {
        assert_eq!(posix_join("/home/u", "a.txt"), "/home/u/a.txt");
        assert_eq!(posix_join("/", "etc"), "/etc");
        assert_eq!(posix_join("/srv/", "/x"), "/srv/x");
        assert_eq!(posix_parent("/home/u/web"), Some("/home/u".into()));
        assert_eq!(posix_parent("/home/u/web/"), Some("/home/u".into()));
        assert_eq!(posix_parent("/home"), Some("/".into()));
        assert_eq!(posix_parent("/"), None);
        assert_eq!(posix_parent("relativa"), None);
        assert_eq!(PathStyle::Posix.file_name("/srv/app/"), Some("app".into()));
        assert_eq!(PathStyle::Posix.file_name("/"), None);
        assert_eq!(PathStyle::Posix.join("/srv", "app"), "/srv/app");
    }

    #[test]
    fn rutas_locales() {
        let dir = std::env::temp_dir();
        let d = dir.to_string_lossy();
        let joined = PathStyle::Native.join(&d, "a.txt");
        assert_eq!(
            PathStyle::Native.parent(&joined).as_deref(),
            Some(d.trim_end_matches(['\\', '/']))
        );
        assert_eq!(PathStyle::Native.file_name(&joined), Some("a.txt".into()));
    }

    #[cfg(windows)]
    #[test]
    fn rutas_de_wsl_son_locales() {
        let p = r"\\wsl$\Ubuntu\home\demo\web";
        assert_eq!(PathStyle::Native.parent(p).as_deref(), Some(r"\\wsl$\Ubuntu\home\demo"));
        assert_eq!(PathStyle::Native.file_name(p).as_deref(), Some("web"));
        assert_eq!(PathStyle::Native.join(p, "a.md"), r"\\wsl$\Ubuntu\home\demo\web\a.md");
    }

    #[test]
    fn nombres_en_conflicto() {
        let none = |_: &str| false;
        assert_eq!(conflict_name("informe.pdf", none), "informe (1).pdf");
        assert_eq!(conflict_name("LEEME", none), "LEEME (1)");
        assert_eq!(conflict_name(".bashrc", none), ".bashrc (1)");
        assert_eq!(conflict_name("datos.tar.gz", none), "datos (1).tar.gz");
        assert_eq!(conflict_name("v1.2.zip", none), "v1.2 (1).zip");
        let taken = ["informe (1).pdf", "informe (2).pdf"];
        assert_eq!(conflict_name("informe.pdf", |n| taken.contains(&n)), "informe (3).pdf");
    }

    #[test]
    fn carpeta_del_agente() {
        assert_eq!(agent_dir("Sesión 1"), "/tmp/kuntur-sesion-1");
        assert_eq!(agent_dir("API · producción"), "/tmp/kuntur-api-produccion");
        assert_eq!(agent_dir("../../etc"), "/tmp/kuntur-etc");
        assert_eq!(agent_dir("  "), "/tmp/kuntur-sesion");
        assert!(agent_dir(&"x".repeat(100)).len() <= "/tmp/kuntur-".len() + 40);
    }

    #[test]
    fn comillas_posix() {
        let v = vec!["/tmp/a b.txt".to_string(), "/tmp/it's.md".to_string()];
        assert_eq!(posix_quote_all(&v), r"'/tmp/a b.txt' '/tmp/it'\''s.md'");
    }

    #[test]
    fn etiqueta_local_o_remota() {
        assert_eq!(location_label(None), "local");
        assert_eq!(location_label(Some("demo@servidor")), "remoto · demo@servidor");
    }
}
