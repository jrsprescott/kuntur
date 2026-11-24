//! Lectura de un archivo para la vista previa, con límites (un log de 2 GB no debe colgar la UI).

use std::io::Read;
use std::path::Path;

use crate::kind::{FileKind, kind_for};

/// Lo máximo que se lee de un archivo de texto (lo demás se avisa como "truncado").
pub const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextDoc {
    pub text: String,
    /// Se leyó solo el comienzo.
    pub truncated: bool,
}

impl TextDoc {
    pub fn line_count(&self) -> usize {
        self.text.lines().count().max(1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Markdown(TextDoc),
    Text(TextDoc),
    /// La UI dibuja la imagen desde la ruta (PNG/JPG/GIF/SVG…).
    Image,
    /// PDF y HTML: visor web o "Abrir con su programa".
    Web,
    Office,
    /// Binario: se muestra el tamaño y "Abrir con su programa".
    Binary {
        size: u64,
    },
}

impl Content {
    /// Texto del archivo, si es textual (para el diff).
    pub fn text(&self) -> Option<&str> {
        match self {
            Content::Markdown(d) | Content::Text(d) => Some(&d.text),
            _ => None,
        }
    }
}

/// Decodifica UTF-8 (con o sin BOM) o UTF-16 con BOM (lo que escribe Windows PowerShell 5);
/// cualquier otra cosa, con reemplazo.
pub fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    let utf16 = |rest: &[u8], le: bool| {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if le {
                    u16::from_le_bytes(c)
                } else {
                    u16::from_be_bytes(c)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, false);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn read_text(path: &Path, size: u64) -> std::io::Result<TextDoc> {
    let mut buf = Vec::with_capacity(size.min(MAX_TEXT_BYTES) as usize);
    std::fs::File::open(path)?.take(MAX_TEXT_BYTES).read_to_end(&mut buf)?;
    let truncated = size > MAX_TEXT_BYTES;
    let mut text = decode(&buf);
    // Sin "\r": se dibuja línea por línea.
    if text.contains('\r') {
        text = text.replace("\r\n", "\n");
    }
    Ok(TextDoc { text, truncated })
}

/// Lee el archivo y decide cómo mostrarlo.
pub fn load(path: &Path) -> std::io::Result<(FileKind, Content)> {
    let meta = std::fs::metadata(path)?;
    if meta.is_dir() {
        return Err(std::io::Error::other("es una carpeta"));
    }
    let size = meta.len();
    let mut head = [0u8; 8192];
    let n = std::fs::File::open(path)?.read(&mut head)?;
    let kind = kind_for(path, &head[..n]);
    let content = match kind {
        FileKind::Markdown => Content::Markdown(read_text(path, size)?),
        FileKind::Text => Content::Text(read_text(path, size)?),
        FileKind::Image | FileKind::Svg => Content::Image,
        FileKind::Pdf | FileKind::Html => Content::Web,
        FileKind::Office => Content::Office,
        FileKind::Other => Content::Binary { size },
    };
    Ok((kind, content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodifica_bom_y_utf16() {
        assert_eq!(decode(b"\xEF\xBB\xBFhola"), "hola");
        assert_eq!(decode(b"\xFF\xFEh\x00\xF3\x00"), "hó");
        assert_eq!(decode(b"\xFE\xFF\x00h"), "h");
        assert_eq!(decode("ñandú".as_bytes()), "ñandú");
    }

    #[test]
    fn lee_texto_markdown_y_binario() {
        let dir = tempfile::tempdir().unwrap();
        let md = dir.path().join("a.md");
        std::fs::write(&md, "# Título\r\n\r\ntexto").unwrap();
        let (kind, content) = load(&md).unwrap();
        assert_eq!(kind, FileKind::Markdown);
        assert_eq!(content.text(), Some("# Título\n\ntexto"));

        let bin = dir.path().join("x.dat");
        std::fs::write(&bin, [0u8, 1, 2, 3]).unwrap();
        assert_eq!(load(&bin).unwrap().1, Content::Binary { size: 4 });

        assert!(load(dir.path()).is_err());
        assert!(load(&dir.path().join("no-existe.txt")).is_err());
    }

    #[test]
    fn trunca_archivos_grandes() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("grande.log");
        let line = "0123456789abcdef\n".repeat(1024);
        let mut f = std::fs::File::create(&big).unwrap();
        for _ in 0..(MAX_TEXT_BYTES as usize / line.len() + 2) {
            std::io::Write::write_all(&mut f, line.as_bytes()).unwrap();
        }
        drop(f);
        match load(&big).unwrap().1 {
            Content::Text(doc) => {
                assert!(doc.truncated);
                assert_eq!(doc.text.len() as u64, MAX_TEXT_BYTES);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn archivos_de_ejemplo_del_repo() {
        let datos = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("datos");
        assert_eq!(load(&datos.join("ejemplo.md")).unwrap().0, FileKind::Markdown);
        assert_eq!(load(&datos.join("ejemplo.svg")).unwrap().1, Content::Image);
        assert_eq!(load(&datos.join("ejemplo.png")).unwrap().1, Content::Image);
        assert_eq!(load(&datos.join("ejemplo.rs")).unwrap().0, FileKind::Text);
    }
}
