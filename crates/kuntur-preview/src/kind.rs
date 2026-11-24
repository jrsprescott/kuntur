//! Tipo de archivo por su extensión (y, si no se conoce, por su contenido).

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Markdown,
    /// PNG, JPG, GIF, WebP, BMP, ICO (lo dibuja `img` de GPUI).
    Image,
    Svg,
    /// Texto o código: se muestra con números de línea.
    Text,
    Pdf,
    Html,
    /// PPTX, DOCX, XLSX (y los formatos de LibreOffice): PDF con LibreOffice si está instalado.
    Office,
    /// Binario u otro: solo "Abrir con su programa".
    Other,
}

impl FileKind {
    /// ¿Tiene sentido un diff de texto?
    pub fn is_textual(self) -> bool {
        matches!(
            self,
            FileKind::Markdown | FileKind::Text | FileKind::Svg | FileKind::Html
        )
    }
}

const TEXT_EXT: &[&str] = &[
    "txt",
    "log",
    "rs",
    "toml",
    "json",
    "jsonc",
    "yaml",
    "yml",
    "xml",
    "csv",
    "tsv",
    "ini",
    "cfg",
    "conf",
    "env",
    "py",
    "js",
    "mjs",
    "cjs",
    "ts",
    "tsx",
    "jsx",
    "css",
    "scss",
    "less",
    "c",
    "h",
    "cpp",
    "hpp",
    "cc",
    "cs",
    "java",
    "kt",
    "go",
    "rb",
    "php",
    "lua",
    "sh",
    "bash",
    "zsh",
    "ps1",
    "psm1",
    "psd1",
    "bat",
    "cmd",
    "sql",
    "diff",
    "patch",
    "gitignore",
    "gitattributes",
    "lock",
    "dockerfile",
    "makefile",
    "vue",
    "svelte",
    "swift",
    "dart",
    "r",
    "pl",
    "ex",
    "exs",
    "erl",
    "hs",
    "ml",
    "fs",
    "zig",
    "nim",
    "proto",
    "graphql",
    "tex",
    "rst",
    "adoc",
    "org",
];

/// Tipo según la extensión; `None` si la extensión no dice nada (se mira el contenido).
fn kind_by_extension(path: &Path) -> Option<FileKind> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    // Archivos sin extensión pero conocidos.
    if matches!(
        name.as_str(),
        "makefile" | "dockerfile" | "license" | "readme" | ".gitignore"
    ) {
        return Some(FileKind::Text);
    }
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    Some(match ext.as_str() {
        "md" | "markdown" | "mdx" => FileKind::Markdown,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" => FileKind::Image,
        "svg" => FileKind::Svg,
        "pdf" => FileKind::Pdf,
        "html" | "htm" | "xhtml" => FileKind::Html,
        "pptx" | "ppt" | "docx" | "doc" | "xlsx" | "xls" | "odt" | "ods" | "odp" | "rtf" => FileKind::Office,
        e if TEXT_EXT.contains(&e) => FileKind::Text,
        "exe" | "dll" | "zip" | "7z" | "rar" | "gz" | "tar" | "bin" | "iso" | "msi" | "mp3" | "mp4" | "wav" | "avi"
        | "mkv" | "mov" | "ttf" | "otf" | "woff" | "woff2" | "pdb" | "obj" | "o" | "a" | "lib" | "class" | "jar"
        | "sqlite" | "db" => FileKind::Other,
        _ => return None,
    })
}

/// ¿Parece texto? Sin bytes nulos en el comienzo (salvo UTF-16 con BOM).
pub fn looks_textual(head: &[u8]) -> bool {
    if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
        return true;
    }
    !head.contains(&0)
}

/// Tipo de archivo: por extensión y, si no se reconoce, por los primeros bytes (`head`).
pub fn kind_for(path: &Path, head: &[u8]) -> FileKind {
    kind_by_extension(path).unwrap_or(if looks_textual(head) {
        FileKind::Text
    } else {
        FileKind::Other
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn por_extension() {
        assert_eq!(kind_for(Path::new("a/LÉEME.MD"), b""), FileKind::Markdown);
        assert_eq!(kind_for(Path::new("x.png"), b""), FileKind::Image);
        assert_eq!(kind_for(Path::new("x.JPeG"), b""), FileKind::Image);
        assert_eq!(kind_for(Path::new("logo.svg"), b""), FileKind::Svg);
        assert_eq!(kind_for(Path::new("main.rs"), b""), FileKind::Text);
        assert_eq!(kind_for(Path::new("doc.pdf"), b""), FileKind::Pdf);
        assert_eq!(kind_for(Path::new("index.html"), b""), FileKind::Html);
        assert_eq!(kind_for(Path::new("informe.docx"), b""), FileKind::Office);
        assert_eq!(kind_for(Path::new("app.exe"), b"MZ"), FileKind::Other);
        assert_eq!(kind_for(Path::new("Makefile"), b""), FileKind::Text);
    }

    #[test]
    fn por_contenido_si_la_extension_no_dice_nada() {
        assert_eq!(kind_for(Path::new("notas"), b"hola\n"), FileKind::Text);
        assert_eq!(kind_for(Path::new("datos.xyz"), b"\x00\x01\x02"), FileKind::Other);
        assert_eq!(kind_for(Path::new("u16.xyz"), b"\xFF\xFEh\x00"), FileKind::Text);
    }

    #[test]
    fn textuales_para_diff() {
        assert!(FileKind::Markdown.is_textual());
        assert!(!FileKind::Image.is_textual());
    }
}
