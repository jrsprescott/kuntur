//! Historial y memoria de carpetas guardados en texto, en `%APPDATA%\Kuntur\historial`:
//! - `comandos.tsv`: una línea por comando (`hora⇥carpeta⇥comando`), solo se agrega al final; al
//!   cargar se compacta si creció demasiado.
//! - `carpetas.tsv`: `ruta⇥visitas⇥última` (se reescribe entero; son pocas líneas).
//!
//! Sin carpeta (`in_memory`) no se escribe nada: así corren las pruebas y la prueba de humo.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::folders::{Folder, Folders};
use crate::history::{History, MAX_COMMANDS};

const COMMANDS_FILE: &str = "comandos.tsv";
const FOLDERS_FILE: &str = "carpetas.tsv";

#[derive(Default)]
pub struct Memory {
    pub history: History,
    pub folders: Folders,
    dir: Option<PathBuf>,
}

impl Memory {
    /// `%APPDATA%\Kuntur\historial`
    pub fn default_dir() -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Kuntur").join("historial"))
    }

    /// Sin archivos.
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// Carga lo guardado en `dir` (si no existe, empieza vacío). Nunca falla.
    pub fn open(dir: &Path) -> Self {
        let mut m = Self {
            dir: Some(dir.to_path_buf()),
            ..Self::default()
        };
        let mut lines = 0;
        if let Ok(text) = std::fs::read_to_string(dir.join(COMMANDS_FILE)) {
            for line in text.lines() {
                lines += 1;
                let mut parts = line.splitn(3, '\t');
                if let (Some(t), Some(cwd), Some(cmd)) = (parts.next(), parts.next(), parts.next())
                    && let Ok(t) = t.parse()
                {
                    m.history.push(cmd, cwd, t);
                }
            }
        }
        // Muchas líneas repetidas o viejas: se reescribe solo con lo vigente.
        if lines > MAX_COMMANDS + MAX_COMMANDS / 2 || lines > 2 * m.history.len() + 100 {
            let _ = m.rewrite_commands();
        }
        if let Ok(text) = std::fs::read_to_string(dir.join(FOLDERS_FILE)) {
            for line in text.lines() {
                let mut parts = line.splitn(3, '\t');
                if let (Some(path), Some(v), Some(t)) = (parts.next(), parts.next(), parts.next())
                    && let (Ok(visits), Ok(last)) = (v.parse(), t.parse())
                {
                    m.folders.insert(Folder {
                        path: path.to_string(),
                        visits,
                        last,
                    });
                }
            }
        }
        m
    }

    pub fn record_command(&mut self, command: &str, cwd: &str, now: u64) {
        if !self.history.push(command, cwd, now) {
            return;
        }
        if let Some(dir) = &self.dir {
            let line = format!("{now}\t{cwd}\t{}\n", command.trim_end());
            let _ = std::fs::create_dir_all(dir).and_then(|_| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dir.join(COMMANDS_FILE))?
                    .write_all(line.as_bytes())
            });
        }
    }

    pub fn record_visit(&mut self, path: &str, now: u64) {
        if path.contains(['\t', '\n']) {
            return;
        }
        self.folders.visit(path, now);
        if let Some(dir) = &self.dir {
            let mut text = String::new();
            for f in self.folders.entries() {
                text.push_str(&format!("{}\t{}\t{}\n", f.path, f.visits, f.last));
            }
            let _ = write_atomic(&dir.join(FOLDERS_FILE), &text);
        }
    }

    fn rewrite_commands(&self) -> std::io::Result<()> {
        let Some(dir) = &self.dir else { return Ok(()) };
        let mut text = String::new();
        for e in self.history.entries() {
            text.push_str(&format!("{}\t{}\t{}\n", e.time, e.cwd, e.command));
        }
        write_atomic(&dir.join(COMMANDS_FILE), &text)
    }
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tsv.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kuntur-memory-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn guarda_y_vuelve_a_cargar() {
        let dir = tmp("ida-vuelta");
        let mut m = Memory::open(&dir);
        assert!(m.history.is_empty());
        m.record_command("git status", "C:\\demo", 10);
        m.record_command(" oculto", "C:\\demo", 11);
        m.record_command("cargo test", "C:\\demo", 12);
        m.record_visit("C:\\demo", 10);
        m.record_visit("C:\\demo", 20);
        let m2 = Memory::open(&dir);
        let cmds: Vec<_> = m2.history.entries().iter().map(|e| e.command.as_str()).collect();
        assert_eq!(cmds, vec!["git status", "cargo test"]);
        assert_eq!(m2.folders.entries()[0].visits, 2);
        assert_eq!(m2.folders.entries()[0].last, 20);
    }

    #[test]
    fn archivos_rotos_no_rompen_nada() {
        let dir = tmp("rotos");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(COMMANDS_FILE), "basura\nx\ty\n12\tC:\\a\tdir\n").unwrap();
        std::fs::write(dir.join(FOLDERS_FILE), "C:\\a\tno-numero\t1\nC:\\b\t3\t4\n").unwrap();
        let m = Memory::open(&dir);
        assert_eq!(m.history.len(), 1);
        assert_eq!(m.folders.len(), 1);
    }

    #[test]
    fn compacta_los_repetidos_al_cargar() {
        let dir = tmp("compacta");
        std::fs::create_dir_all(&dir).unwrap();
        let text: String = (0..500).map(|i| format!("{i}\tC:\\a\tdir\n")).collect();
        std::fs::write(dir.join(COMMANDS_FILE), text).unwrap();
        let m = Memory::open(&dir);
        assert_eq!(m.history.len(), 1);
        let after = std::fs::read_to_string(dir.join(COMMANDS_FILE)).unwrap();
        assert_eq!(after.lines().count(), 1);
    }

    #[test]
    fn en_memoria_no_escribe() {
        let mut m = Memory::in_memory();
        m.record_command("dir", "C:\\", 1);
        m.record_visit("C:\\", 1);
        assert_eq!(m.history.len(), 1);
        assert_eq!(m.folders.len(), 1);
    }
}
