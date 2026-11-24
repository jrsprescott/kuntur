//! Vista previa de archivos de Kuntur (especificación §9 y §14), sin UI: qué tipo de archivo es,
//! cómo leerlo con límites, el diff a colores (parseo de diff unificado, diff entre dos versiones y
//! `git diff`), la vigilancia de cambios (`notify`) y la detección de LibreOffice.

pub mod diff;
pub mod kind;
pub mod load;
pub mod office;
pub mod watch;

pub use diff::{DiffLine, DiffLineKind, DiffStats, diff_texts, git_diff, parse_unified};
pub use kind::{FileKind, kind_for};
pub use load::{Content, MAX_TEXT_BYTES, TextDoc, load};
pub use office::{convert_to_pdf, find_libreoffice};
pub use watch::{FileWatcher, same_file};

/// URL `file:///` de una ruta absoluta (para el visor web), con los caracteres especiales
/// codificados (`%`, `#`, `?`, espacios, no ASCII…).
pub fn file_url(path: &std::path::Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file:///");
    for b in s.trim_start_matches('/').bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Proceso sin ventana de consola (la app es de subsistema Windows: sin esto, `git` o
/// LibreOffice abrirían una consola negra un instante).
pub(crate) fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_de_archivo() {
        assert_eq!(
            file_url(std::path::Path::new(r"C:\Mis datos\informe #1.pdf")),
            "file:///C:/Mis%20datos/informe%20%231.pdf"
        );
        assert_eq!(
            file_url(std::path::Path::new(r"D:\año\a.html")),
            "file:///D:/a%C3%B1o/a.html"
        );
    }
}
