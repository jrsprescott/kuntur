//! Autocompletado de Kuntur (estilo fish/Warp): dado lo escrito en el prompt, la carpeta actual,
//! el historial propio y la memoria de carpetas, devuelve **una** continuación sugerida que la UI
//! dibuja en gris después del cursor.
//!
//! - [`paths`]: rutas de Windows (sin distinguir mayúsculas, carpetas con `\`, comillas si hay espacios).
//! - [`history`]: comandos que empiezan igual (de la misma carpeta primero).
//! - [`folders`]: carpetas visitadas por frecuencia × recencia (como `z`/zoxide) para `cd parte`.
//! - [`memory`]: guarda historial y carpetas en `%APPDATA%\Kuntur\historial` (texto, sin SQLite).
//!
//! No depende de la UI ni del PTY: todo se prueba con carpetas temporales y datos sintéticos.

pub mod folders;
pub mod history;
pub mod memory;
pub mod paths;
pub mod tokens;

use std::path::{Path, PathBuf};

pub use folders::Folders;
pub use history::History;
pub use memory::Memory;
pub use paths::DirCache;

/// De dónde salió la sugerencia.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    History,
    Path,
    Folder,
}

/// Lo que se ofrece al usuario.
///
/// Aceptar = borrar `erase` caracteres antes del cursor y escribir `insert`. En el caso normal
/// (`erase == 0`) `insert` es exactamente lo que se ve en gris (`ghost`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub erase: usize,
    pub insert: String,
    pub ghost: String,
    pub source: Source,
}

impl Suggestion {
    pub fn append(text: impl Into<String>, source: Source) -> Self {
        let text = text.into();
        Self {
            erase: 0,
            insert: text.clone(),
            ghost: text,
            source,
        }
    }
}

/// Comandos que cambian de carpeta: solo ofrecen carpetas y usan la memoria de carpetas.
const CD_COMMANDS: &[&str] = &["cd", "chdir", "sl", "set-location", "pushd", "push-location"];

pub fn is_cd_command(word: &str) -> bool {
    CD_COMMANDS.iter().any(|c| c.eq_ignore_ascii_case(word))
}

/// Motor de sugerencias: memoria (historial + carpetas) y caché de carpetas leídas.
pub struct Completer {
    pub memory: Memory,
    cache: DirCache,
    home: Option<PathBuf>,
}

impl Completer {
    pub fn new(memory: Memory, home: Option<PathBuf>) -> Self {
        Self {
            memory,
            cache: DirCache::default(),
            home,
        }
    }

    /// La mejor continuación para `input` (lo escrito desde el inicio de la entrada hasta el cursor).
    pub fn suggest(&mut self, input: &str, cwd: Option<&Path>, now: u64) -> Option<Suggestion> {
        if input.trim().is_empty() || input.contains(['\n', '\r']) {
            return None;
        }
        let line = tokens::parse(input);
        let cd = line.tokens.first().is_some_and(|t| is_cd_command(&t.text));
        let cwd_key = cwd.map(|c| c.to_string_lossy().into_owned());

        // 1) Historial: el comando completo, como fish. Para `cd` solo de la misma carpeta (las
        //    rutas relativas de otra carpeta no sirven aquí).
        if let Some(rest) = self.memory.history.continuation(input, cwd_key.as_deref(), cd) {
            return Some(Suggestion::append(rest, Source::History));
        }

        let last = if line.ends_with_space { None } else { line.tokens.last() };
        let last = last?;
        let is_arg = line.tokens.len() >= 2;

        // 2) Ruta del último argumento (o del comando, si parece ruta: `.\scr`, `C:\x`).
        if is_arg || tokens::looks_like_path(&last.text) {
            let folders = &self.memory.folders;
            let rank = |p: &Path| folders.score(&p.to_string_lossy(), now);
            let ctx = paths::PathContext {
                cwd,
                home: self.home.as_deref(),
                dirs_only: cd,
            };
            if let Some(s) = paths::complete(&last.text, &ctx, &mut self.cache, &rank) {
                return Some(s);
            }
        }

        // 3) Memoria de carpetas: `cd astro` → la carpeta más usada que se llame así.
        if cd && line.tokens.len() == 2 {
            return self.memory.folders.suggest_for(&last.text, cwd, now);
        }
        None
    }

    /// El usuario ejecutó `command` en `cwd` (Enter en el prompt).
    pub fn record_command(&mut self, command: &str, cwd: Option<&Path>, now: u64) {
        let cwd = cwd.map(|c| c.to_string_lossy().into_owned()).unwrap_or_default();
        self.memory.record_command(command, &cwd, now);
    }

    /// El shell informó una carpeta actual nueva.
    pub fn record_visit(&mut self, dir: &Path, now: u64) {
        self.memory.record_visit(&dir.to_string_lossy(), now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kuntur-complete-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn historial_antes_que_rutas() {
        let dir = tmp("orden");
        std::fs::create_dir(dir.join("src")).unwrap();
        let mut c = Completer::new(Memory::in_memory(), None);
        c.record_command("git status --short", Some(&dir), 100);
        let s = c.suggest("git st", Some(&dir), 200).unwrap();
        assert_eq!(s, Suggestion::append("atus --short", Source::History));
        // Sin historial que sirva: ruta del argumento.
        let s = c.suggest("ls s", Some(&dir), 200).unwrap();
        assert_eq!(s.ghost, "rc\\");
        assert_eq!(s.source, Source::Path);
    }

    #[test]
    fn nada_con_entrada_vacia_o_con_espacio_final() {
        let dir = tmp("vacio");
        std::fs::create_dir(dir.join("docs")).unwrap();
        let mut c = Completer::new(Memory::in_memory(), None);
        assert_eq!(c.suggest("", Some(&dir), 1), None);
        assert_eq!(c.suggest("   ", Some(&dir), 1), None);
        assert_eq!(c.suggest("cd ", Some(&dir), 1), None);
        // La primera palabra sin pinta de ruta no se completa contra archivos.
        assert_eq!(c.suggest("do", Some(&dir), 1), None);
        assert_eq!(c.suggest(".\\do", Some(&dir), 1).unwrap().ghost, "cs\\");
    }

    #[test]
    fn cd_usa_la_memoria_de_carpetas() {
        let base = tmp("memoria");
        let web = base.join("astro-web");
        std::fs::create_dir(&web).unwrap();
        let mut c = Completer::new(Memory::in_memory(), None);
        c.record_visit(&web, 1_000);
        let otra = tmp("memoria-otra");
        let s = c.suggest("cd astro", Some(&otra), 1_100).unwrap();
        assert_eq!(s.source, Source::Folder);
        assert_eq!(s.erase, "astro".len());
        assert_eq!(s.insert, web.to_string_lossy());
    }

    #[test]
    fn cd_no_repite_historial_de_otra_carpeta() {
        let a = tmp("cd-a");
        let b = tmp("cd-b");
        let mut c = Completer::new(Memory::in_memory(), None);
        c.record_command("cd sub-de-a", Some(&a), 10);
        assert_eq!(c.suggest("cd sub", Some(&b), 20), None);
        assert_eq!(c.suggest("cd sub", Some(&a), 20).unwrap().ghost, "-de-a");
    }
}
