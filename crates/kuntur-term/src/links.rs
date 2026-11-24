//! Enlaces en la salida (especificación §9): URLs `http(s)://` y rutas de archivos (absolutas de
//! Windows, entre comillas o relativas a la carpeta de la sesión), con `:línea[:columna]` o
//! `(línea,columna)` opcional. Lógica pura: trabaja sobre una fila de la cuadrícula y devuelve
//! columnas exactas (los caracteres anchos ocupan dos), sin tocar la UI.
//!
//! Una ruta candidata solo se convierte en enlace si existe ([`resolve`]): así "e.g." o "v0.11" no
//! se subrayan.

use std::path::{Component, Path, PathBuf};

use crate::screen::Screen;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Url,
    Path,
}

/// Un enlace encontrado en una fila: columnas `[start, end)` y el texto del destino (sin el
/// sufijo de línea).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkSpan {
    pub start: usize,
    pub end: usize,
    pub kind: LinkKind,
    pub target: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// Destino ya comprobado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    Url(String),
    File {
        path: PathBuf,
        line: Option<u32>,
        column: Option<u32>,
    },
}

impl Link {
    /// Texto para "Copiar ruta" / "Copiar URL".
    pub fn display(&self) -> String {
        match self {
            Link::Url(u) => u.clone(),
            Link::File { path, .. } => path.display().to_string(),
        }
    }
}

/// Caracteres que nunca forman parte de una ruta o URL sin comillas.
fn is_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | '|' | '*' | '?' | '\0')
}

/// Caracteres que terminan una URL.
fn is_url_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | '\0')
}

fn starts_with_ci(s: &[char], at: usize, prefix: &str) -> bool {
    let p: Vec<char> = prefix.chars().collect();
    s.len() >= at + p.len()
        && s[at..at + p.len()]
            .iter()
            .zip(&p)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// `X:\` o `X:/` en `at`.
fn is_drive(s: &[char], at: usize) -> bool {
    s.len() > at + 2 && s[at].is_ascii_alphabetic() && s[at + 1] == ':' && matches!(s[at + 2], '\\' | '/')
}

/// Quita la puntuación final que casi nunca es parte del enlace y los cierres sin su apertura.
fn trim_end(s: &[char], start: usize, mut end: usize) -> usize {
    loop {
        if end <= start {
            return end;
        }
        let c = s[end - 1];
        let open = match c {
            ')' => Some('('),
            ']' => Some('['),
            '}' => Some('{'),
            _ => None,
        };
        if matches!(c, '.' | ',' | ';' | ':' | '!' | '?') {
            end -= 1;
        } else if let Some(open) = open {
            let opens = s[start..end].iter().filter(|&&x| x == open).count();
            let closes = s[start..end].iter().filter(|&&x| x == c).count();
            if closes > opens {
                end -= 1;
            } else {
                return end;
            }
        } else {
            return end;
        }
    }
}

fn digits(s: &[char]) -> Option<u32> {
    if s.is_empty() || s.len() > 9 || !s.iter().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.iter().collect::<String>().parse().ok()
}

/// Separa el sufijo de línea: `ruta:12`, `ruta:12:5`, `ruta(12)`, `ruta(12,5)`.
/// Devuelve (fin de la ruta, línea, columna).
fn split_suffix(s: &[char], start: usize, end: usize) -> (usize, Option<u32>, Option<u32>) {
    let t = &s[start..end];
    // Estilo MSVC / TypeScript: archivo(12,5)
    if t.last() == Some(&')')
        && let Some(open) = t.iter().rposition(|&c| c == '(')
    {
        let inner = &t[open + 1..t.len() - 1];
        let mut parts = inner.split(|&c| c == ',');
        let line = parts.next().and_then(digits);
        let col = parts.next().and_then(digits);
        if line.is_some() && parts.next().is_none() && (col.is_some() || !inner.contains(&',')) {
            return (start + open, line, col);
        }
    }
    // Estilo grep / rustc: archivo:12 o archivo:12:5 (sin confundir la unidad "C:").
    let mut nums: Vec<u32> = Vec::new();
    let mut cut = t.len();
    while nums.len() < 2 {
        let Some(colon) = t[..cut].iter().rposition(|&c| c == ':') else {
            break;
        };
        match digits(&t[colon + 1..cut]) {
            Some(n) if colon > 1 => {
                nums.push(n);
                cut = colon;
            }
            _ => break,
        }
    }
    match nums.as_slice() {
        [line] => (start + cut, Some(*line), None),
        [col, line] => (start + cut, Some(*line), Some(*col)),
        _ => (end, None, None),
    }
}

/// ¿Parece una ruta? Absoluta, con separador de carpetas o `nombre.ext`.
fn looks_like_path(t: &[char]) -> bool {
    if t.is_empty() || t.iter().all(|&c| matches!(c, '.' | '/' | '\\')) {
        return false;
    }
    if is_drive(t, 0) || t.contains(&'\\') || t.contains(&'/') {
        return true;
    }
    // nombre.ext: extensión de 1–10 letras o números, con al menos una letra.
    match t.iter().rposition(|&c| c == '.') {
        Some(dot) if dot > 0 && dot + 1 < t.len() => {
            let ext = &t[dot + 1..];
            ext.len() <= 10
                && ext.iter().all(|c| c.is_alphanumeric())
                && ext.iter().any(|c| c.is_alphabetic())
                && t[..dot].iter().any(|c| c.is_alphanumeric())
        }
        _ => false,
    }
}

/// Enlaces de una línea de texto; los índices son posiciones de carácter.
pub fn find_links(s: &[char]) -> Vec<LinkSpan> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        // 1) Ruta absoluta entre comillas (puede tener espacios): "C:\Program Files\x.txt"
        if matches!(s[i], '"' | '\'')
            && is_drive(s, i + 1)
            && let Some(close) = s[i + 1..].iter().position(|&c| c == s[i])
        {
            let (a, b) = (i + 1, i + 1 + close);
            let (path_end, line, column) = split_suffix(s, a, b);
            out.push(LinkSpan {
                start: a,
                end: b,
                kind: LinkKind::Path,
                target: s[a..path_end].iter().collect(),
                line,
                column,
            });
            i = b + 1;
            continue;
        }
        if is_separator(s[i]) {
            i += 1;
            continue;
        }
        // 2) Una "palabra" sin separadores.
        let a = i;
        while i < s.len() && !is_separator(s[i]) {
            i += 1;
        }
        let b = i;
        if let Some(span) = link_in_token(s, a, b) {
            // Una URL puede seguir más allá de la palabra ("?", "*" y "|" son válidos en ella).
            i = i.max(span.end);
            out.push(span);
        }
    }
    out
}

fn link_in_token(s: &[char], a: usize, b: usize) -> Option<LinkSpan> {
    // URL en cualquier parte de la palabra (por ejemplo "url=https://…").
    for p in a..b {
        if (p == a || !s[p - 1].is_alphanumeric())
            && (starts_with_ci(s, p, "https://") || starts_with_ci(s, p, "http://"))
        {
            let mut url_end = p;
            while url_end < s.len() && !is_url_separator(s[url_end]) {
                url_end += 1;
            }
            let end = trim_end(s, p, url_end);
            let scheme = if starts_with_ci(s, p, "https://") { 8 } else { 7 };
            if end <= p + scheme {
                return None;
            }
            return Some(LinkSpan {
                start: p,
                end,
                kind: LinkKind::Url,
                target: s[p..end].iter().collect(),
                line: None,
                column: None,
            });
        }
    }
    // Inicio de la ruta: una unidad "X:\" dentro de la palabra, o lo que sigue al último "=",
    // sin aperturas sueltas al principio.
    let mut start = (a..b)
        .find(|&p| is_drive(s, p) && (p == a || !s[p - 1].is_alphanumeric()))
        .unwrap_or_else(|| s[a..b].iter().rposition(|&c| c == '=').map_or(a, |e| a + e + 1));
    while start < b && matches!(s[start], '(' | '[' | '{') {
        start += 1;
    }
    let end = trim_end(s, start, b);
    if end <= start {
        return None;
    }
    let (path_end, line, column) = split_suffix(s, start, end);
    let path_end = trim_end(s, start, path_end);
    let path = &s[start..path_end];
    if !looks_like_path(path) || s[start..end].windows(3).any(|w| w == [':', '/', '/']) {
        return None;
    }
    Some(LinkSpan {
        start,
        end,
        kind: LinkKind::Path,
        target: path.iter().collect(),
        line,
        column,
    })
}

/// Caracteres de una fila de la pantalla con la columna de cada uno (se salta la mitad derecha
/// de los caracteres anchos).
pub fn row_chars(screen: &Screen, row: usize) -> (Vec<char>, Vec<usize>) {
    let mut chars = Vec::with_capacity(screen.cols);
    let mut cols = Vec::with_capacity(screen.cols);
    let mut skip = false;
    for c in 0..screen.cols {
        if skip {
            skip = false;
            continue;
        }
        let cell = screen.cell(c, row);
        chars.push(if cell.ch == '\0' { ' ' } else { cell.ch });
        cols.push(c);
        skip = cell.wide;
    }
    (chars, cols)
}

/// Enlaces de una fila de la pantalla, en columnas de la cuadrícula.
pub fn links_in_row(screen: &Screen, row: usize) -> Vec<LinkSpan> {
    if row >= screen.rows {
        return Vec::new();
    }
    let (chars, cols) = row_chars(screen, row);
    find_links(&chars)
        .into_iter()
        .map(|mut l| {
            let last = l.end - 1;
            let width = if screen.cell(cols[last], row).wide { 2 } else { 1 };
            l.start = cols[l.start];
            l.end = cols[last] + width;
            l
        })
        .collect()
}

/// El enlace que cubre la columna `col` de la fila, si hay alguno.
pub fn link_at(screen: &Screen, row: usize, col: usize) -> Option<LinkSpan> {
    links_in_row(screen, row)
        .into_iter()
        .find(|l| (l.start..l.end).contains(&col))
}

/// Quita `.` y resuelve `..` sin tocar el disco.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Convierte un enlace encontrado en un destino real: las URLs pasan tal cual; las rutas, si son
/// absolutas o relativas a `cwd` (la carpeta de la sesión), **solo si existen**. Para las rutas
/// de `git diff` ("a/src/x.rs", "b/src/x.rs") se prueba también sin el prefijo.
pub fn resolve(span: &LinkSpan, cwd: Option<&Path>, exists: impl Fn(&Path) -> bool) -> Option<Link> {
    if span.kind == LinkKind::Url {
        return Some(Link::Url(span.target.clone()));
    }
    let raw = Path::new(&span.target);
    let mut candidates = Vec::new();
    if raw.is_absolute() {
        candidates.push(raw.to_path_buf());
    } else if let Some(cwd) = cwd {
        candidates.push(cwd.join(raw));
        for prefix in ["a/", "b/"] {
            if let Some(rest) = span.target.strip_prefix(prefix) {
                candidates.push(cwd.join(rest));
            }
        }
    }
    candidates
        .into_iter()
        .map(|p| normalize(&p))
        .find(|p| exists(p))
        .map(|path| Link::File {
            path,
            line: span.line,
            column: span.column,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::Emulator;
    use crate::palette::Palette;
    use crate::size::GridSize;

    fn links(s: &str) -> Vec<(String, Option<u32>, Option<u32>, LinkKind)> {
        let chars: Vec<char> = s.chars().collect();
        find_links(&chars)
            .into_iter()
            .map(|l| (l.target, l.line, l.column, l.kind))
            .collect()
    }

    fn one(s: &str) -> (String, Option<u32>, Option<u32>, LinkKind) {
        let mut v = links(s);
        assert_eq!(v.len(), 1, "en {s:?}: {v:?}");
        v.remove(0)
    }

    #[test]
    fn url_con_puntuacion_final() {
        assert_eq!(one("ver https://example.com/a?b=1.").0, "https://example.com/a?b=1");
        assert_eq!(one("(http://example.com/x)").0, "http://example.com/x");
        assert_eq!(
            one("http://es.wikipedia.org/wiki/Cóndor_(ave), fin").0,
            "http://es.wikipedia.org/wiki/Cóndor_(ave)"
        );
        assert_eq!(one("url=https://example.com/p").0, "https://example.com/p");
        assert_eq!(one("https://example.com").3, LinkKind::Url);
        assert!(links("https:// nada").is_empty());
    }

    #[test]
    fn ruta_absoluta_de_windows() {
        assert_eq!(one(r"Guardado en C:\proyecto\notas.md.").0, r"C:\proyecto\notas.md");
        assert_eq!(one("d:/datos/x.txt").0, "d:/datos/x.txt");
        // El prompt de PowerShell.
        assert_eq!(one(r"PS C:\Windows\System32> ").0, r"C:\Windows\System32");
        assert_eq!(one(r"--out=C:\tmp\salida.log").0, r"C:\tmp\salida.log");
    }

    #[test]
    fn ruta_entre_comillas_con_espacios() {
        let chars: Vec<char> = r#"abrir "C:\Program Files\app\léeme.txt" ya"#.chars().collect();
        let l = &find_links(&chars)[0];
        assert_eq!(l.target, r"C:\Program Files\app\léeme.txt");
        assert_eq!((l.start, l.end), (7, 37));
    }

    #[test]
    fn rutas_relativas() {
        assert_eq!(one(r"src\main.rs").0, r"src\main.rs");
        assert_eq!(one(r".\docs\a.md").0, r".\docs\a.md");
        assert_eq!(one(r"..\b\c").0, r"..\b\c");
        assert_eq!(one("modified:   crates/x/lib.rs").0, "crates/x/lib.rs");
        assert_eq!(one("Cargo.toml").0, "Cargo.toml");
        assert_eq!(one("(ver docs/PLAN.md)").0, "docs/PLAN.md");
    }

    #[test]
    fn no_son_rutas() {
        assert!(links("hola mundo 1.5 v0.11 --help").is_empty());
        assert!(links("... . / \\").is_empty());
        assert!(links("C: sin barra").is_empty());
    }

    #[test]
    fn sufijos_de_linea_y_columna() {
        assert_eq!(
            one("src/main.rs:12:5: error"),
            ("src/main.rs".into(), Some(12), Some(5), LinkKind::Path)
        );
        assert_eq!(
            one(r"C:\x\a.rs:7"),
            (r"C:\x\a.rs".into(), Some(7), None, LinkKind::Path)
        );
        assert_eq!(
            one(r"app.cs(12,5): error CS1002"),
            ("app.cs".into(), Some(12), Some(5), LinkKind::Path)
        );
        assert_eq!(
            one("main.cpp(40): aviso"),
            ("main.cpp".into(), Some(40), None, LinkKind::Path)
        );
    }

    #[test]
    fn varios_en_una_linea() {
        let v = links(r"diff a/x.rs b/x.rs y https://example.com");
        assert_eq!(v.len(), 3);
        assert_eq!(v[2].3, LinkKind::Url);
    }

    #[test]
    fn columnas_con_caracteres_anchos() {
        let pal = Palette::dark();
        let mut emu = Emulator::new(GridSize::new(40, 2));
        emu.feed("漢字 src/a.rs".as_bytes());
        let screen = Screen::capture(&emu, &pal);
        let l = &links_in_row(&screen, 0)[0];
        // "漢字 " ocupa 5 columnas.
        assert_eq!((l.start, l.end), (5, 13));
        assert!(link_at(&screen, 0, 7).is_some());
        assert!(link_at(&screen, 0, 2).is_none());
    }

    #[test]
    fn resolver_solo_si_existe() {
        let cwd = Path::new(r"C:\proyecto");
        let span = |t: &str| {
            let chars: Vec<char> = t.chars().collect();
            find_links(&chars).remove(0)
        };
        let existe = |p: &Path| p == Path::new(r"C:\proyecto\src\main.rs") || p == Path::new(r"C:\otro\b.txt");
        assert_eq!(
            resolve(&span(r".\src\main.rs:3"), Some(cwd), existe),
            Some(Link::File {
                path: PathBuf::from(r"C:\proyecto\src\main.rs"),
                line: Some(3),
                column: None
            })
        );
        // ".." se resuelve sin tocar el disco.
        assert!(resolve(&span(r"..\otro\b.txt"), Some(cwd), existe).is_some());
        // Rutas de git diff.
        assert!(resolve(&span("b/src/main.rs"), Some(cwd), existe).is_some());
        // No existe, o relativa sin carpeta conocida.
        assert_eq!(resolve(&span("nada.txt"), Some(cwd), existe), None);
        assert_eq!(resolve(&span(r"src\main.rs"), None, existe), None);
        assert_eq!(
            resolve(&span("https://example.com"), None, |_| false),
            Some(Link::Url("https://example.com".into()))
        );
    }
}
