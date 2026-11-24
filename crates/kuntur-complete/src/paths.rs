//! Completar rutas contra el sistema de archivos.
//!
//! - Sin distinguir mayúsculas (Windows): `c:\win` → `dows\`.
//! - Las carpetas terminan en `\` (o `/` si el usuario escribe con `/`).
//! - Si el resultado tiene espacios u otros caracteres especiales de PowerShell y la palabra no
//!   tenía comillas, aceptar reescribe la palabra entre comillas simples.
//! - Entre varias, primero las carpetas más visitadas (memoria de carpetas), luego en orden alfabético.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::{Source, Suggestion};

/// Cuánto vale lo leído de una carpeta antes de volver a leerla.
const TTL: Duration = Duration::from_secs(2);
/// Carpetas guardadas en la caché como máximo.
const MAX_CACHED: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub hidden: bool,
}

/// Lecturas recientes de carpetas: escribir letra a letra no relee la carpeta cada vez.
#[derive(Default)]
pub struct DirCache {
    map: HashMap<PathBuf, (Instant, Arc<[Entry]>)>,
}

impl DirCache {
    pub fn list(&mut self, dir: &Path) -> Arc<[Entry]> {
        if let Some((at, entries)) = self.map.get(dir)
            && at.elapsed() < TTL
        {
            return entries.clone();
        }
        let entries: Arc<[Entry]> = read_dir(dir).into();
        if self.map.len() >= MAX_CACHED {
            self.map.retain(|_, (at, _)| at.elapsed() < TTL);
        }
        if self.map.len() < MAX_CACHED {
            self.map.insert(dir.to_path_buf(), (Instant::now(), entries.clone()));
        }
        entries
    }
}

fn read_dir(dir: &Path) -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let meta = e.metadata().ok();
            // Un enlace a carpeta cuenta como carpeta.
            let is_dir = e.file_type().ok().is_some_and(|t| t.is_dir()) || e.path().is_dir();
            Some(Entry {
                hidden: meta.as_ref().is_some_and(is_hidden),
                name,
                is_dir,
            })
        })
        .collect()
}

#[cfg(windows)]
fn is_hidden(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}

#[cfg(not(windows))]
fn is_hidden(_meta: &std::fs::Metadata) -> bool {
    false
}

pub struct PathContext<'a> {
    pub cwd: Option<&'a Path>,
    pub home: Option<&'a Path>,
    /// Solo carpetas (para `cd`).
    pub dirs_only: bool,
}

/// Sugerencia para la palabra `word` (tal como se escribió, quizá con una comilla al inicio).
/// `rank` da un puntaje extra a una ruta completa (frecuencia × recencia).
pub fn complete(
    word: &str,
    ctx: &PathContext,
    cache: &mut DirCache,
    rank: &dyn Fn(&Path) -> f64,
) -> Option<Suggestion> {
    let (quote, body) = match word.chars().next() {
        Some(q @ ('\'' | '"')) => (Some(q), &word[1..]),
        _ => (None, word),
    };
    // Comillas ya cerradas o escapes: no se toca.
    if quote.is_some_and(|q| body.contains(q)) || body.contains('`') {
        return None;
    }
    let cut = body.rfind(['\\', '/']).map_or(0, |i| i + 1);
    let (dir_part, prefix) = body.split_at(cut);
    if prefix.is_empty() {
        return None;
    }
    let dir = resolve_dir(dir_part, ctx)?;
    let entries = cache.list(&dir);
    let mut matches: Vec<&Entry> = entries
        .iter()
        .filter(|e| (!ctx.dirs_only || e.is_dir) && starts_with_ignore_case(&e.name, prefix))
        .filter(|e| e.is_dir || e.name.chars().count() > prefix.chars().count())
        .collect();
    if matches.is_empty() {
        return None;
    }
    let score = |e: &Entry| if e.is_dir { rank(&dir.join(&e.name)) } else { 0.0 };
    matches.sort_by(|a, b| {
        a.hidden
            .cmp(&b.hidden)
            .then(score(b).total_cmp(&score(a)))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    let best = matches[0];

    let sep = if body.contains('/') && !body.contains('\\') {
        '/'
    } else {
        '\\'
    };
    let rest: String = best.name.chars().skip(prefix.chars().count()).collect();
    let tail = if best.is_dir { sep.to_string() } else { String::new() };
    let ghost = format!("{rest}{tail}");
    if ghost.is_empty() {
        return None;
    }

    if quote.is_none() && needs_quotes(&best.name) {
        // `cd C:\Prog` → `cd 'C:\Program Files\'`: aceptar borra la palabra escrita entera y la
        // reescribe entre comillas (si solo se agregara el resto, PowerShell cortaría en el
        // espacio). En gris se ve el reemplazo completo, como en la memoria de carpetas.
        let quoted = ps_quote(&format!("{dir_part}{}{tail}", best.name));
        return Some(Suggestion {
            erase: word.chars().count(),
            ghost: format!("  → {quoted}"),
            insert: quoted,
            source: Source::Path,
        });
    }
    // Con comilla abierta: un archivo la cierra; una carpeta la deja abierta para seguir escribiendo.
    let close = match quote {
        Some(q) if !best.is_dir => q.to_string(),
        _ => String::new(),
    };
    Some(Suggestion::append(format!("{ghost}{close}"), Source::Path))
}

fn resolve_dir(dir_part: &str, ctx: &PathContext) -> Option<PathBuf> {
    if dir_part.is_empty() {
        return ctx.cwd.map(Path::to_path_buf);
    }
    if let Some(rest) = dir_part.strip_prefix('~') {
        let rest = rest.trim_start_matches(['\\', '/']);
        return ctx.home.map(|h| h.join(rest));
    }
    let p = Path::new(dir_part);
    if is_absolute(dir_part) {
        Some(p.to_path_buf())
    } else {
        ctx.cwd.map(|c| c.join(p))
    }
}

/// `C:\`, `\\servidor\x`, `/x` y `\x` (raíz de la unidad actual) cuentan como absolutas.
fn is_absolute(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':') || s.starts_with(['\\', '/'])
}

pub fn starts_with_ignore_case(name: &str, prefix: &str) -> bool {
    let mut n = name.chars();
    prefix
        .chars()
        .all(|p| n.next().is_some_and(|c| c.to_lowercase().eq(p.to_lowercase())))
}

/// Caracteres que en PowerShell obligan a usar comillas.
fn needs_quotes(name: &str) -> bool {
    name.contains(|c: char| c.is_whitespace() || "'\"`$&|;,(){}@#<>".contains(c))
}

/// Entre comillas simples de PowerShell si hace falta (`'` se duplica).
pub fn ps_quote(path: &str) -> String {
    if needs_quotes(path) {
        format!("'{}'", path.replace('\'', "''"))
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kuntur-paths-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["Windows", "WinSxS", "Program Files", "proyectos", "Datos de usuario"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::write(dir.join("notas.txt"), "x").unwrap();
        std::fs::write(dir.join("Win.ini"), "x").unwrap();
        dir
    }

    fn run(word: &str, cwd: &Path, dirs_only: bool) -> Option<Suggestion> {
        let ctx = PathContext {
            cwd: Some(cwd),
            home: None,
            dirs_only,
        };
        complete(word, &ctx, &mut DirCache::default(), &|_| 0.0)
    }

    #[test]
    fn completa_sin_distinguir_mayusculas() {
        let t = tree("mayus");
        let s = run("win", &t, true).unwrap();
        // `Windows` antes que `WinSxS` (orden alfabético sin mayúsculas).
        assert_eq!(s, Suggestion::append("dows\\", Source::Path));
        assert_eq!(run("PROY", &t, true).unwrap().ghost, "ectos\\");
    }

    #[test]
    fn ruta_absoluta_y_relativa() {
        let t = tree("abs");
        let word = format!("{}\\proy", t.display());
        assert_eq!(run(&word, Path::new("C:\\"), true).unwrap().ghost, "ectos\\");
        assert_eq!(run(".\\proy", &t, true).unwrap().ghost, "ectos\\");
        assert_eq!(run("./proy", &t, true).unwrap().ghost, "ectos/");
    }

    #[test]
    fn solo_carpetas_para_cd_y_archivos_para_lo_demas() {
        let t = tree("archivos");
        assert_eq!(run("not", &t, true), None);
        assert_eq!(run("not", &t, false).unwrap().ghost, "as.txt");
        // Carpeta con el mismo nombre que lo escrito: falta solo la barra.
        assert_eq!(run("Windows", &t, true).unwrap().ghost, "\\");
    }

    #[test]
    fn entrecomilla_si_hay_espacios() {
        let t = tree("comillas");
        let word = format!("{}\\Prog", t.display());
        let s = run(&word, &t, true).unwrap();
        assert_eq!(s.erase, word.chars().count(), "borra la palabra escrita entera");
        assert_eq!(s.insert, format!("'{}\\Program Files\\'", t.display()));
        assert_eq!(s.ghost, format!("  → {}", s.insert), "en gris se ve el reemplazo");
        // Relativa: `cd Datos` → `cd 'Datos de usuario\'`.
        let s = run("Datos", &t, true).unwrap();
        assert_eq!((s.erase, s.insert.as_str()), (5, "'Datos de usuario\\'"));
        // Si el usuario ya abrió comillas, solo se agrega lo que falta.
        let s = run("'Datos", &t, true).unwrap();
        assert_eq!(s, Suggestion::append(" de usuario\\", Source::Path));
        assert_eq!(run("'nota", &t, false).unwrap().ghost, "s.txt'");
    }

    #[test]
    fn sin_coincidencias_o_sin_prefijo() {
        let t = tree("nada");
        assert_eq!(run("zzz", &t, false), None);
        assert_eq!(run(".\\", &t, false), None);
        assert_eq!(run("'cerrado'", &t, false), None);
        assert_eq!(run("no-existe\\x", &t, false), None);
    }

    #[test]
    fn prefiere_las_carpetas_mas_visitadas() {
        let t = tree("rank");
        let ctx = PathContext {
            cwd: Some(&t),
            home: None,
            dirs_only: true,
        };
        let rank = |p: &Path| if p.ends_with("WinSxS") { 5.0 } else { 0.0 };
        let s = complete("win", &ctx, &mut DirCache::default(), &rank).unwrap();
        assert_eq!(s.ghost, "SxS\\");
    }

    #[test]
    fn carpeta_personal_con_virgulilla() {
        let t = tree("home");
        let ctx = PathContext {
            cwd: None,
            home: Some(&t),
            dirs_only: true,
        };
        let s = complete("~\\proy", &ctx, &mut DirCache::default(), &|_| 0.0).unwrap();
        assert_eq!(s.ghost, "ectos\\");
    }

    #[test]
    fn comillas_de_powershell() {
        assert_eq!(ps_quote("C:\\a"), "C:\\a");
        assert_eq!(ps_quote("C:\\a b"), "'C:\\a b'");
        assert_eq!(ps_quote("C:\\o'brien x"), "'C:\\o''brien x'");
    }
}
