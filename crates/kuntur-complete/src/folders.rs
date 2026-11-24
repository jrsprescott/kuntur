//! Memoria de carpetas (como `z`/zoxide): cada carpeta visitada suma una visita; el puntaje es
//! visitas × peso por recencia. `cd astro` sugiere la carpeta más usada cuyo nombre coincide
//! (búsqueda difusa con `nucleo-matcher`).

use std::path::Path;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::history::dir_key;
use crate::paths::{ps_quote, starts_with_ignore_case};
use crate::{Source, Suggestion};

/// Carpetas recordadas como máximo (se olvidan las de menor puntaje).
pub const MAX_FOLDERS: usize = 1_000;

const HOUR: u64 = 3_600;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;

#[derive(Clone, Debug, PartialEq)]
pub struct Folder {
    pub path: String,
    pub visits: u32,
    pub last: u64,
}

impl Folder {
    /// Frecuencia × recencia (mismos pesos que zoxide).
    pub fn score(&self, now: u64) -> f64 {
        let age = now.saturating_sub(self.last);
        let weight = if age < HOUR {
            4.0
        } else if age < DAY {
            2.0
        } else if age < WEEK {
            0.5
        } else {
            0.25
        };
        self.visits as f64 * weight
    }
}

#[derive(Default)]
pub struct Folders {
    entries: Vec<Folder>,
}

impl Folders {
    pub fn entries(&self) -> &[Folder] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Carga una entrada guardada (sin juntar visitas).
    pub fn insert(&mut self, folder: Folder) {
        let key = dir_key(&folder.path);
        self.entries.retain(|f| dir_key(&f.path) != key);
        self.entries.push(folder);
    }

    pub fn visit(&mut self, path: &str, now: u64) {
        if path.trim().is_empty() {
            return;
        }
        let key = dir_key(path);
        match self.entries.iter_mut().find(|f| dir_key(&f.path) == key) {
            Some(f) => {
                f.visits = f.visits.saturating_add(1);
                f.last = now;
                f.path = path.to_string();
            }
            None => self.entries.push(Folder {
                path: path.to_string(),
                visits: 1,
                last: now,
            }),
        }
        if self.entries.len() > MAX_FOLDERS {
            self.entries.sort_by(|a, b| b.score(now).total_cmp(&a.score(now)));
            self.entries.truncate(MAX_FOLDERS);
        }
    }

    pub fn score(&self, path: &str, now: u64) -> f64 {
        let key = dir_key(path);
        self.entries
            .iter()
            .find(|f| dir_key(&f.path) == key)
            .map_or(0.0, |f| f.score(now))
    }

    /// La carpeta recordada que mejor responde a `query` (lo escrito después de `cd`), sin contar
    /// la carpeta actual ni las que ya no existen.
    pub fn best(&self, query: &str, cwd: Option<&Path>, now: u64) -> Option<&Folder> {
        let query = query.trim_matches(['\'', '"']);
        if query.is_empty() {
            return None;
        }
        let here = cwd.map(|c| dir_key(&c.to_string_lossy()));
        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let mut buf = Vec::new();
        let mut found: Vec<(&Folder, u32)> = self
            .entries
            .iter()
            .filter(|f| here.as_deref() != Some(dir_key(&f.path).as_str()))
            .filter_map(|f| {
                // Ruta escrita desde el principio (`cd D:\pro`) o parte del nombre (`cd astro`).
                if starts_with_ignore_case(&f.path, query) {
                    return Some((f, u32::MAX));
                }
                let name = last_component(&f.path);
                pattern
                    .score(Utf32Str::new(name, &mut buf), &mut matcher)
                    .map(|s| (f, s))
            })
            .collect();
        found.sort_by(|(a, sa), (b, sb)| b.score(now).total_cmp(&a.score(now)).then(sb.cmp(sa)));
        found.into_iter().map(|(f, _)| f).find(|f| Path::new(&f.path).is_dir())
    }

    /// Sugerencia para `cd <query>`: si la ruta empieza con lo escrito, solo agrega el resto; si
    /// no, aceptar reemplaza la palabra por la ruta completa (y en gris se ve `→ ruta`).
    pub fn suggest_for(&self, query: &str, cwd: Option<&Path>, now: u64) -> Option<Suggestion> {
        let folder = self.best(query, cwd, now)?;
        let path = &folder.path;
        let plain = !query.starts_with(['\'', '"']) && ps_quote(path) == *path;
        if plain && starts_with_ignore_case(path, query) {
            let rest: String = path.chars().skip(query.chars().count()).collect();
            return (!rest.is_empty()).then(|| Suggestion::append(rest, Source::Folder));
        }
        Some(Suggestion {
            erase: query.chars().count(),
            insert: ps_quote(path),
            ghost: format!("  → {path}"),
            source: Source::Folder,
        })
    }
}

fn last_component(path: &str) -> &str {
    let p = path.trim_end_matches(['\\', '/']);
    p.rsplit(['\\', '/']).next().unwrap_or(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(name: &str, subs: &[&str]) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!("kuntur-folders-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for s in subs {
            std::fs::create_dir_all(base.join(s)).unwrap();
        }
        base
    }

    #[test]
    fn frecuencia_por_recencia() {
        let f = Folder {
            path: "x".into(),
            visits: 10,
            last: 0,
        };
        assert_eq!(f.score(10), 40.0);
        assert_eq!(f.score(2 * HOUR), 20.0);
        assert_eq!(f.score(2 * DAY), 5.0);
        assert_eq!(f.score(30 * DAY), 2.5);
    }

    #[test]
    fn visitar_suma_y_no_duplica() {
        let mut m = Folders::default();
        m.visit("C:\\Proy", 1);
        m.visit("c:\\proy\\", 2);
        m.visit("C:\\Otra", 3);
        assert_eq!(m.len(), 2);
        assert_eq!(m.entries()[0].visits, 2);
        assert_eq!(m.entries()[0].last, 2);
        assert!(m.score("C:\\PROY", 2) > m.score("C:\\Otra", 3) - 1.0);
        assert_eq!(m.score("C:\\nunca", 3), 0.0);
    }

    #[test]
    fn cd_parte_del_nombre_elige_la_mas_usada() {
        let base = dirs("z", &["astro-web", "astro-docs", "otra"]);
        let web = base.join("astro-web").to_string_lossy().into_owned();
        let docs = base.join("astro-docs").to_string_lossy().into_owned();
        let mut m = Folders::default();
        m.visit(&docs, 100);
        for t in 0..3 {
            m.visit(&web, 100 + t);
        }
        let s = m.suggest_for("astro", None, 200).unwrap();
        assert_eq!(s.insert, web);
        assert_eq!(s.erase, 5);
        assert_eq!(s.ghost, format!("  → {web}"));
        // Desde la propia carpeta, sugiere la otra.
        assert_eq!(m.best("astro", Some(Path::new(&web)), 200).unwrap().path, docs);
        // Difusa: "asweb" coincide con "astro-web".
        assert_eq!(m.best("asweb", None, 200).unwrap().path, web);
        assert!(m.best("zzz", None, 200).is_none());
    }

    #[test]
    fn ruta_escrita_desde_el_principio_se_completa() {
        let base = dirs("prefijo", &["proyecto"]);
        let p = base.join("proyecto").to_string_lossy().into_owned();
        let mut m = Folders::default();
        m.visit(&p, 1);
        let typed = &p[..p.len() - 4];
        assert_eq!(
            m.suggest_for(typed, None, 2),
            Some(Suggestion::append("ecto", Source::Folder))
        );
    }

    #[test]
    fn olvida_las_que_ya_no_existen() {
        let base = dirs("borrada", &["viva"]);
        let mut m = Folders::default();
        m.visit(&base.join("muerta-astro").to_string_lossy(), 1);
        assert!(m.best("astro", None, 2).is_none());
    }
}
