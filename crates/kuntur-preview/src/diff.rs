//! Diff a colores (especificación §14): parseo de diff unificado (la salida de `git diff`), diff
//! entre dos versiones de un texto (crate `similar`) y `git diff` de un archivo.

use std::path::Path;

use similar::TextDiff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLineKind {
    /// `diff --git`, `index …`, `--- a/…`, `+++ b/…`, `new file mode …`
    Meta,
    /// `@@ -1,3 +1,4 @@ …`
    Hunk,
    Context,
    Added,
    Removed,
    /// `\ No newline at end of file`
    NoNewline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    /// Número de línea en la versión anterior (contexto y quitadas).
    pub old_no: Option<u32>,
    /// Número de línea en la versión nueva (contexto y agregadas).
    pub new_no: Option<u32>,
    /// Texto sin el prefijo `+`/`-`/` `.
    pub text: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiffStats {
    pub added: usize,
    pub removed: usize,
}

impl DiffStats {
    pub fn of(lines: &[DiffLine]) -> Self {
        let mut s = DiffStats::default();
        for l in lines {
            match l.kind {
                DiffLineKind::Added => s.added += 1,
                DiffLineKind::Removed => s.removed += 1,
                _ => {}
            }
        }
        s
    }
}

/// `@@ -a[,b] +c[,d] @@` → (a, c).
fn hunk_starts(line: &str) -> Option<(u32, u32)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, _) = rest.split_once(" @@")?;
    let first = |s: &str| s.split(',').next()?.parse::<u32>().ok();
    Some((first(old)?, first(new)?))
}

/// Parsea un diff unificado. Fuera de un hunk todo es `Meta`; dentro, cada línea según su
/// prefijo, con sus números de línea.
pub fn parse_unified(text: &str) -> Vec<DiffLine> {
    let mut out = Vec::new();
    let (mut old_no, mut new_no) = (0u32, 0u32);
    let mut in_hunk = false;
    for raw in text.lines() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some((a, c)) = hunk_starts(line) {
            in_hunk = true;
            old_no = a;
            new_no = c;
            out.push(DiffLine {
                kind: DiffLineKind::Hunk,
                old_no: None,
                new_no: None,
                text: line.to_string(),
            });
            continue;
        }
        let mut push = |kind, o, n, t: &str| {
            out.push(DiffLine {
                kind,
                old_no: o,
                new_no: n,
                text: t.to_string(),
            })
        };
        if !in_hunk || line.starts_with("diff --git ") {
            in_hunk = false;
            push(DiffLineKind::Meta, None, None, line);
            continue;
        }
        match line.chars().next() {
            Some('+') => {
                push(DiffLineKind::Added, None, Some(new_no), &line[1..]);
                new_no += 1;
            }
            Some('-') => {
                push(DiffLineKind::Removed, Some(old_no), None, &line[1..]);
                old_no += 1;
            }
            Some('\\') => push(DiffLineKind::NoNewline, None, None, line),
            // Contexto (" ") o línea vacía que algunos editores dejan sin el espacio.
            _ => {
                let t = line.strip_prefix(' ').unwrap_or(line);
                push(DiffLineKind::Context, Some(old_no), Some(new_no), t);
                old_no += 1;
                new_no += 1;
            }
        }
    }
    out
}

/// Diff entre dos versiones de un texto (por ejemplo, la que se vio antes y la actual), con
/// `context` líneas de contexto. Vacío si son iguales.
pub fn diff_texts(old: &str, new: &str, context: usize) -> Vec<DiffLine> {
    if old == new {
        return Vec::new();
    }
    let diff = TextDiff::from_lines(old, new);
    let unified = diff
        .unified_diff()
        .context_radius(context)
        .header("antes", "ahora")
        .to_string();
    parse_unified(&unified)
}

/// `git diff` del archivo contra `HEAD` (cambios preparados y sin preparar). `None` si no está en
/// un repositorio, si `git` no está instalado, si el archivo no tiene cambios o no está bajo
/// control de versiones.
pub fn git_diff(path: &Path) -> Option<String> {
    let dir = path.parent()?;
    let name = path.file_name()?;
    let run = |args: &[&str]| {
        crate::hidden_command("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "core.quotepath=false"])
            .args(args)
            .arg("--")
            .arg(name)
            .output()
            .ok()
    };
    let base = ["diff", "--no-color", "--no-ext-diff"];
    let mut out = run(&[base.as_slice(), &["HEAD"]].concat())?;
    if !out.status.success() {
        // Repositorio sin commits: diff contra el índice.
        out = run(&base)?;
    }
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIT: &str = "diff --git a/src/a.rs b/src/a.rs
index 1111111..2222222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,4 +1,5 @@ fn main() {
 uno
-dos
+DOS
+dos y medio
 tres

@@ -10 +11 @@
-x
+y
\\ No newline at end of file
";

    #[test]
    fn parsea_git_diff() {
        let lines = parse_unified(GIT);
        let kinds: Vec<DiffLineKind> = lines.iter().map(|l| l.kind).collect();
        use DiffLineKind::*;
        assert_eq!(
            kinds,
            vec![
                Meta, Meta, Meta, Meta, Hunk, Context, Removed, Added, Added, Context, Context, Hunk, Removed, Added,
                NoNewline
            ]
        );
        // Números de línea.
        assert_eq!((lines[5].old_no, lines[5].new_no), (Some(1), Some(1)));
        assert_eq!((lines[6].old_no, lines[6].new_no), (Some(2), None));
        assert_eq!((lines[7].old_no, lines[7].new_no), (None, Some(2)));
        assert_eq!(lines[8].new_no, Some(3));
        assert_eq!((lines[9].old_no, lines[9].new_no), (Some(3), Some(4)));
        assert_eq!(lines[7].text, "DOS");
        // Línea vacía de contexto sin el espacio.
        assert_eq!((lines[10].kind, lines[10].text.as_str()), (Context, ""));
        assert_eq!((lines[12].old_no, lines[13].new_no), (Some(10), Some(11)));
        assert_eq!(DiffStats::of(&lines), DiffStats { added: 3, removed: 2 });
    }

    #[test]
    fn diff_entre_versiones() {
        assert!(diff_texts("a\nb\n", "a\nb\n", 3).is_empty());
        let lines = diff_texts("a\nb\nc\n", "a\nB\nc\nd\n", 3);
        assert_eq!(DiffStats::of(&lines), DiffStats { added: 2, removed: 1 });
        let removed = lines.iter().find(|l| l.kind == DiffLineKind::Removed).unwrap();
        assert_eq!((removed.text.as_str(), removed.old_no), ("b", Some(2)));
        let last = lines.iter().rfind(|l| l.kind == DiffLineKind::Added).unwrap();
        assert_eq!((last.text.as_str(), last.new_no), ("d", Some(4)));
    }

    fn git_available() -> bool {
        crate::hidden_command("git")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = crate::hidden_command("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=Prueba", "-c", "user.email=prueba@example.com"])
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn git_diff_de_un_archivo() {
        if !git_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notas.txt");
        // Fuera de un repositorio.
        std::fs::write(&file, "uno\n").unwrap();
        assert_eq!(git_diff(&file), None);

        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["add", "notas.txt"]);
        git(dir.path(), &["commit", "-q", "-m", "inicio"]);
        // Sin cambios.
        assert_eq!(git_diff(&file), None);
        std::fs::write(&file, "uno\ndos\n").unwrap();
        let out = git_diff(&file).expect("hay cambios");
        let lines = parse_unified(&out);
        assert_eq!(DiffStats::of(&lines), DiffStats { added: 1, removed: 0 });
    }
}
