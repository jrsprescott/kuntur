use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use alacritty_terminal::event::Event;
use alacritty_terminal::term::TermMode;
use portable_pty::{Child, CommandBuilder, MasterPty, native_pty_system};

use crate::emulator::Emulator;
use crate::paste::paste_bytes;
use crate::size::GridSize;

/// Qué programa abrir en el panel.
#[derive(Clone, Debug)]
pub struct PaneOptions {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub size: GridSize,
}

impl PaneOptions {
    /// PowerShell 7 si está instalado; si no, Windows PowerShell.
    pub fn default_shell(size: GridSize) -> Self {
        let program = if find_in_path("pwsh.exe") {
            "pwsh.exe"
        } else {
            "powershell.exe"
        };
        Self {
            program: program.into(),
            args: vec!["-NoLogo".into()],
            cwd: None,
            size,
        }
    }
}

fn find_in_path(exe: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
        .unwrap_or(false)
}

/// Un programa corriendo en un PTY (ConPTY en Windows) conectado a su emulador.
pub struct Pane {
    emulator: Arc<Mutex<Emulator>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    exited: Arc<AtomicBool>,
    title: Arc<Mutex<Option<String>>>,
    clipboard: Arc<Mutex<Option<String>>>,
}

impl Pane {
    /// Abre el programa. `on_change` se llama (desde otro hilo) cada vez que hay salida nueva.
    pub fn spawn(opts: PaneOptions, on_change: impl Fn() + Send + 'static) -> anyhow::Result<Pane> {
        let pair = native_pty_system().openpty(opts.size.into())?;

        let mut cmd = CommandBuilder::new(&opts.program);
        // portable-pty arma el entorno releyendo el registro y SUMA el PATH del sistema y del usuario.
        // Con PATH grandes supera los 32.767 caracteres y Windows no deja lanzar procesos desde el
        // shell ("no hay suficientes recursos de memoria"). Usamos exactamente el entorno de Kuntur.
        cmd.env_clear();
        for (key, value) in std::env::vars_os() {
            cmd.env(key, value);
        }
        cmd.args(&opts.args);
        if let Some(dir) = &opts.cwd {
            cmd.cwd(dir);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "Kuntur");

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(pair.master.take_writer()?));
        let emulator = Arc::new(Mutex::new(Emulator::new(opts.size)));
        let exited = Arc::new(AtomicBool::new(false));
        let title = Arc::new(Mutex::new(None));
        let clipboard = Arc::new(Mutex::new(None));

        {
            let (emulator, writer, exited, title, clipboard) = (
                emulator.clone(),
                writer.clone(),
                exited.clone(),
                title.clone(),
                clipboard.clone(),
            );
            thread::Builder::new().name("kuntur-pty-lector".into()).spawn(move || {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    let events = {
                        let mut emu = emulator.lock().unwrap();
                        emu.feed(&buf[..n]);
                        emu.take_events()
                    };
                    for event in events {
                        match event {
                            // Respuestas que el programa pidió (posición del cursor, atributos…).
                            Event::PtyWrite(s) => {
                                let mut w = writer.lock().unwrap();
                                let _ = w.write_all(s.as_bytes());
                                let _ = w.flush();
                            }
                            Event::ClipboardStore(_, text) => *clipboard.lock().unwrap() = Some(text),
                            Event::Title(t) => *title.lock().unwrap() = Some(t),
                            _ => {}
                        }
                    }
                    on_change();
                }
                exited.store(true, Ordering::SeqCst);
                on_change();
            })?;
        }

        Ok(Pane {
            emulator,
            writer,
            master: Mutex::new(pair.master),
            child: Mutex::new(child),
            exited,
            title,
            clipboard,
        })
    }

    /// Envía bytes al programa (teclas).
    pub fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        let mut w = self.writer.lock().unwrap();
        w.write_all(bytes)?;
        w.flush()
    }

    /// Pega texto respetando el modo bracketed paste del programa.
    pub fn paste(&self, text: &str) -> std::io::Result<()> {
        let bracketed = self.emulator.lock().unwrap().mode().contains(TermMode::BRACKETED_PASTE);
        self.write(&paste_bytes(text, bracketed))
    }

    pub fn resize(&self, size: GridSize) -> anyhow::Result<()> {
        self.master.lock().unwrap().resize(size.into())?;
        self.emulator.lock().unwrap().resize(size);
        Ok(())
    }

    /// Acceso al emulador (selección, scroll, captura de pantalla).
    pub fn with_emulator<R>(&self, f: impl FnOnce(&mut Emulator) -> R) -> R {
        f(&mut self.emulator.lock().unwrap())
    }

    /// Texto que un programa pidió copiar con OSC 52 (la UI lo pasa al portapapeles de Windows).
    pub fn take_clipboard_request(&self) -> Option<String> {
        self.clipboard.lock().unwrap().take()
    }

    pub fn title(&self) -> Option<String> {
        self.title.lock().unwrap().clone()
    }

    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst) || self.child.lock().unwrap().try_wait().ok().flatten().is_some()
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        let _ = self.child.lock().unwrap().kill();
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use std::time::{Duration, Instant};

    fn wait_for(pane: &Pane, needle: &str) -> bool {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if pane.with_emulator(|e| e.contains_text(needle)) {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn ejecuta_un_programa_y_muestra_su_salida() {
        let opts = PaneOptions {
            program: "cmd.exe".into(),
            args: vec!["/c".into(), "echo hola-kuntur".into()],
            cwd: None,
            size: GridSize::new(80, 24),
        };
        let pane = Pane::spawn(opts, || {}).expect("no se pudo abrir el PTY");
        assert!(wait_for(&pane, "hola-kuntur"));
    }

    #[test]
    fn envia_teclas_a_un_shell_interactivo() {
        let opts = PaneOptions {
            program: "cmd.exe".into(),
            args: vec!["/q".into(), "/k".into(), "prompt $G".into()],
            cwd: None,
            size: GridSize::new(80, 24),
        };
        let pane = Pane::spawn(opts, || {}).unwrap();
        pane.write(b"echo teclado-ok\r").unwrap();
        assert!(wait_for(&pane, "teclado-ok"));
    }

    /// Regresión: portable-pty relee el registro y concatena el PATH del sistema y del usuario.
    /// Con PATH grandes pasa de 32.767 caracteres y Windows no deja lanzar procesos desde el shell
    /// ("no hay suficientes recursos de memoria"). El hijo debe recibir el PATH de Kuntur tal cual.
    #[test]
    fn el_hijo_hereda_el_path_sin_duplicarlo() {
        let esperado = std::env::var_os("PATH").unwrap_or_default().encode_wide().count();
        let opts = PaneOptions {
            program: "powershell.exe".into(),
            args: vec![
                "-NoProfile".into(),
                "-Command".into(),
                "'PATHLEN=' + $env:Path.Length".into(),
            ],
            cwd: None,
            size: GridSize::new(80, 24),
        };
        let pane = Pane::spawn(opts, || {}).unwrap();
        assert!(wait_for(&pane, "PATHLEN="), "PowerShell no respondió");
        let linea = pane.with_emulator(|e| {
            (0..24)
                .map(|r| e.line_text(r))
                .find(|l| l.contains("PATHLEN="))
                .unwrap_or_default()
        });
        let recibido: usize = linea.trim().trim_start_matches("PATHLEN=").parse().unwrap();
        assert_eq!(recibido, esperado, "el PATH del hijo no coincide con el de Kuntur");
    }
}
