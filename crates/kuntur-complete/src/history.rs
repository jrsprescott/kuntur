//! Historial propio de Kuntur: comandos ejecutados, con su carpeta y su hora.
//! (No lee el historial de PSReadLine.)

use crate::paths::starts_with_ignore_case;

/// Comandos guardados como máximo (los más viejos se descartan).
pub const MAX_COMMANDS: usize = 5_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub command: String,
    pub cwd: String,
    pub time: u64,
}

#[derive(Default)]
pub struct History {
    /// Del más viejo al más nuevo; sin repetir el mismo comando en la misma carpeta.
    entries: Vec<HistoryEntry>,
}

/// Carpeta normalizada para comparar (`C:\A\` = `c:\a`).
pub fn dir_key(dir: &str) -> String {
    let d = dir.trim_end_matches(['\\', '/']);
    let d = if d.ends_with(':') { dir } else { d };
    d.to_lowercase()
}

impl History {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Agrega un comando (si ya estaba en esa carpeta, pasa a ser el más reciente). Devuelve `false`
    /// si no se guarda: vacío, de varias líneas o empezado con espacio (convención de bash/zsh para
    /// "no guardar esto").
    pub fn push(&mut self, command: &str, cwd: &str, time: u64) -> bool {
        if !should_record(command) {
            return false;
        }
        let command = command.trim_end();
        let key = dir_key(cwd);
        self.entries
            .retain(|e| !(e.command == command && dir_key(&e.cwd) == key));
        self.entries.push(HistoryEntry {
            command: command.to_string(),
            cwd: cwd.to_string(),
            time,
        });
        if self.entries.len() > MAX_COMMANDS {
            let extra = self.entries.len() - MAX_COMMANDS;
            self.entries.drain(..extra);
        }
        true
    }

    /// Lo que falta para completar `input` con el comando más reciente que empieza igual: primero
    /// los de la misma carpeta; los de otras carpetas solo si `same_dir_only` es `false`.
    pub fn continuation(&self, input: &str, cwd: Option<&str>, same_dir_only: bool) -> Option<String> {
        let key = cwd.map(dir_key);
        let fits = |e: &&HistoryEntry| {
            e.command.chars().count() > input.chars().count() && starts_with_ignore_case(&e.command, input)
        };
        let here = self
            .entries
            .iter()
            .rev()
            .filter(fits)
            .find(|e| key.as_deref() == Some(dir_key(&e.cwd).as_str()));
        let found = match here {
            Some(e) => Some(e),
            None if same_dir_only => None,
            None => self.entries.iter().rev().find(fits),
        }?;
        Some(found.command.chars().skip(input.chars().count()).collect())
    }
}

pub fn should_record(command: &str) -> bool {
    !command.trim().is_empty() && !command.starts_with(' ') && !command.contains(['\n', '\r', '\t'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_mas_reciente_y_de_la_misma_carpeta_primero() {
        let mut h = History::default();
        h.push("git status", "C:\\a", 1);
        h.push("git stash pop", "C:\\b", 2);
        h.push("git switch main", "C:\\c", 3);
        assert_eq!(
            h.continuation("git st", Some("C:\\A\\"), false).as_deref(),
            Some("atus")
        );
        assert_eq!(
            h.continuation("git st", Some("C:\\z"), false).as_deref(),
            Some("ash pop")
        );
        assert_eq!(h.continuation("git st", Some("C:\\z"), true), None);
        assert_eq!(h.continuation("GIT SW", None, false).as_deref(), Some("itch main"));
    }

    #[test]
    fn sin_repetidos_y_con_limite() {
        let mut h = History::default();
        h.push("dir", "C:\\a", 1);
        h.push("cls", "C:\\a", 2);
        h.push("dir", "c:\\a\\", 3);
        assert_eq!(h.len(), 2);
        assert_eq!(h.entries().last().unwrap().command, "dir");
        for i in 0..MAX_COMMANDS + 10 {
            h.push(&format!("echo {i}"), "C:\\a", i as u64);
        }
        assert_eq!(h.len(), MAX_COMMANDS);
    }

    #[test]
    fn no_guarda_lo_que_no_debe() {
        let mut h = History::default();
        assert!(!h.push("", "C:\\", 1));
        assert!(!h.push("   ", "C:\\", 1));
        assert!(!h.push(" secreto", "C:\\", 1));
        assert!(!h.push("a\nb", "C:\\", 1));
        assert!(h.push("dir  ", "C:\\", 1));
        assert_eq!(h.entries()[0].command, "dir");
        // Igual a lo escrito: no hay nada que sugerir.
        assert_eq!(h.continuation("dir", None, false), None);
    }

    #[test]
    fn clave_de_carpeta() {
        assert_eq!(dir_key("C:\\Users\\X\\"), "c:\\users\\x");
        assert_eq!(dir_key("C:\\"), "c:\\");
    }
}
