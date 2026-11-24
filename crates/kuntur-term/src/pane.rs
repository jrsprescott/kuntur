use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use alacritty_terminal::event::Event;
use alacritty_terminal::term::TermMode;
use portable_pty::{CommandBuilder, native_pty_system};

use crate::cwd::CwdScanner;
use crate::emulator::Emulator;
use crate::paste::paste_bytes;
use crate::size::GridSize;
use crate::transport::{LocalPty, Transport};

/// Qué programa abrir en el panel.
#[derive(Clone, Debug)]
pub struct PaneOptions {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub size: GridSize,
}

impl PaneOptions {
    /// PowerShell 7 si está instalado; si no, Windows PowerShell. Envuelve el `prompt` (el del perfil
    /// del usuario, si lo hay) para que además informe la carpeta actual con OSC 9;9, como Windows
    /// Terminal; lo que el usuario ve no cambia.
    pub fn default_shell(size: GridSize) -> Self {
        let program = if find_in_path("pwsh.exe") {
            "pwsh.exe"
        } else {
            "powershell.exe"
        };
        Self {
            program: program.into(),
            args: vec![
                "-NoLogo".into(),
                "-NoExit".into(),
                "-Command".into(),
                PROMPT_HOOK.into(),
            ],
            cwd: None,
            size,
        }
    }
}

/// Script de `-Command`: sin comillas dobles (para no depender de cómo cada PowerShell parte la línea
/// de comandos). Antepone `ESC ] 9 ; 9 ; "<carpeta>" ESC \` a la salida del prompt original.
const PROMPT_HOOK: &str = concat!(
    "$global:__kunturPrompt = $function:prompt; ",
    "function global:prompt { ",
    "$l = $executionContext.SessionState.Path.CurrentLocation; ",
    "$o = (& $global:__kunturPrompt) -join ''; ",
    "if ($l.Provider.Name -eq 'FileSystem') { ",
    "$e = [string][char]27; $q = [string][char]34; ",
    "$e + ']9;9;' + $q + $l.ProviderPath + $q + $e + '\\' + $o ",
    "} else { $o } }"
);

fn find_in_path(exe: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
        .unwrap_or(false)
}

/// Un programa conectado a su emulador: local (ConPTY) o remoto (cualquier [`Transport`]).
pub struct Pane {
    emulator: Arc<Mutex<Emulator>>,
    transport: Arc<dyn Transport>,
    exited: Arc<AtomicBool>,
    title: Arc<Mutex<Option<String>>>,
    clipboard: Arc<Mutex<Option<String>>>,
    cwd: Arc<Mutex<Option<PathBuf>>>,
    last_output: Arc<Mutex<Instant>>,
}

impl Pane {
    /// Abre el programa en un PTY local. `on_change` se llama (desde otro hilo) cada vez que hay
    /// salida nueva.
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

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let transport = Arc::new(LocalPty::new(pair.master, writer, child));
        Self::with_transport(transport, reader, opts.size, on_change)
    }

    /// Conecta un emulador nuevo a un transporte cualquiera: `reader` trae la salida del programa
    /// (su fin = el programa terminó) y `transport` lleva las teclas, el tamaño y las respuestas.
    pub fn with_transport(
        transport: Arc<dyn Transport>,
        mut reader: Box<dyn Read + Send>,
        size: GridSize,
        on_change: impl Fn() + Send + 'static,
    ) -> anyhow::Result<Pane> {
        let emulator = Arc::new(Mutex::new(Emulator::new(size)));
        let exited = Arc::new(AtomicBool::new(false));
        let title = Arc::new(Mutex::new(None));
        let clipboard = Arc::new(Mutex::new(None));
        let cwd = Arc::new(Mutex::new(None));
        let last_output = Arc::new(Mutex::new(Instant::now()));

        {
            let (emulator, transport, exited, title, clipboard, cwd, last_output) = (
                emulator.clone(),
                transport.clone(),
                exited.clone(),
                title.clone(),
                clipboard.clone(),
                cwd.clone(),
                last_output.clone(),
            );
            thread::Builder::new().name("kuntur-pty-lector".into()).spawn(move || {
                let mut buf = vec![0u8; 64 * 1024];
                let mut scanner = CwdScanner::new();
                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    *last_output.lock().unwrap() = Instant::now();
                    // Antes del emulador y sin tocar los bytes: solo mira si pasó una carpeta nueva.
                    if let Some(dir) = scanner.feed(&buf[..n]) {
                        *cwd.lock().unwrap() = Some(PathBuf::from(dir));
                    }
                    let events = {
                        let mut emu = emulator.lock().unwrap();
                        emu.feed(&buf[..n]);
                        emu.take_events()
                    };
                    for event in events {
                        match event {
                            // Respuestas que el programa pidió (posición del cursor, atributos…).
                            Event::PtyWrite(s) => {
                                let _ = transport.write(s.as_bytes());
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
            transport,
            exited,
            title,
            clipboard,
            cwd,
            last_output,
        })
    }

    /// Envía bytes al programa (teclas).
    pub fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        self.transport.write(bytes)
    }

    /// Pega texto respetando el modo bracketed paste del programa.
    pub fn paste(&self, text: &str) -> std::io::Result<()> {
        let bracketed = self.emulator.lock().unwrap().mode().contains(TermMode::BRACKETED_PASTE);
        self.write(&paste_bytes(text, bracketed))
    }

    pub fn resize(&self, size: GridSize) -> anyhow::Result<()> {
        self.transport.resize(size)?;
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

    /// PID del programa (el shell de la sesión); `None` si es remoto.
    pub fn pid(&self) -> Option<u32> {
        self.transport.pid()
    }

    /// Carpeta actual informada por el shell (OSC 7 / OSC 9;9); `None` hasta el primer aviso.
    pub fn cwd(&self) -> Option<PathBuf> {
        self.cwd.lock().unwrap().clone()
    }

    /// Tiempo desde la última salida del programa.
    pub fn last_output_age(&self) -> Duration {
        self.last_output.lock().unwrap().elapsed()
    }

    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst) || self.transport.has_exited()
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        self.transport.kill();
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;

    /// Transporte falso: guarda lo escrito y los tamaños.
    #[derive(Default)]
    struct Recorder {
        written: Mutex<Vec<u8>>,
        sizes: Mutex<Vec<GridSize>>,
        killed: AtomicBool,
    }

    impl Transport for Recorder {
        fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
            self.written.lock().unwrap().extend_from_slice(bytes);
            Ok(())
        }
        fn resize(&self, size: GridSize) -> anyhow::Result<()> {
            self.sizes.lock().unwrap().push(size);
            Ok(())
        }
        fn kill(&self) {
            self.killed.store(true, Ordering::SeqCst);
        }
    }

    fn wait(cond: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if cond() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn un_transporte_cualquiera_alimenta_el_emulador() {
        let rec = Arc::new(Recorder::default());
        // Texto, la carpeta por OSC 7 (como un bash remoto) y una consulta de posición del cursor.
        let salida = b"hola remoto\r\n\x1b]7;file://servidor/var/www\x07\x1b[6n".to_vec();
        let pane = Pane::with_transport(
            rec.clone(),
            Box::new(std::io::Cursor::new(salida)),
            GridSize::new(80, 24),
            || {},
        )
        .unwrap();
        assert!(wait(|| pane.has_exited()), "el fin del lector termina el panel");
        assert!(pane.with_emulator(|e| e.contains_text("hola remoto")));
        assert_eq!(pane.cwd().as_deref(), Some(std::path::Path::new("/var/www")));
        assert_eq!(pane.pid(), None);
        // La respuesta del emulador (posición del cursor) vuelve por el transporte.
        assert!(String::from_utf8_lossy(&rec.written.lock().unwrap()).contains("R"));
        pane.write(b"ls\r").unwrap();
        assert!(rec.written.lock().unwrap().ends_with(b"ls\r"));
        pane.resize(GridSize::new(100, 30)).unwrap();
        assert_eq!(rec.sizes.lock().unwrap().as_slice(), &[GridSize::new(100, 30)]);
        drop(pane);
        assert!(
            rec.killed.load(Ordering::SeqCst),
            "cerrar el panel cierra el transporte"
        );
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStrExt;

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

    fn wait_cwd(pane: &Pane) -> Option<PathBuf> {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if let Some(dir) = pane.cwd() {
                return Some(dir);
            }
            thread::sleep(Duration::from_millis(50));
        }
        None
    }

    #[test]
    fn la_carpeta_actual_llega_por_osc_9_9() {
        let opts = PaneOptions {
            program: "cmd.exe".into(),
            args: vec!["/c".into(), "echo \x1b]9;9;C:\\Windows\x07listo-osc".into()],
            cwd: None,
            size: GridSize::new(80, 24),
        };
        let pane = Pane::spawn(opts, || {}).unwrap();
        assert!(pane.pid().is_some_and(|p| p > 0));
        assert_eq!(wait_cwd(&pane).as_deref(), Some(std::path::Path::new(r"C:\Windows")));
        assert!(wait_for(&pane, "listo-osc"));
        assert!(
            !pane.with_emulator(|e| e.contains_text("9;9")),
            "la secuencia no se ve en pantalla"
        );
        assert!(pane.last_output_age() < Duration::from_secs(10));
    }

    #[test]
    fn el_shell_por_defecto_informa_su_carpeta() {
        let dir = std::env::current_dir().unwrap();
        let mut opts = PaneOptions::default_shell(GridSize::new(120, 30));
        assert!(opts.args.iter().any(|a| a == "-NoExit"));
        opts.cwd = Some(dir.clone());
        let pane = Pane::spawn(opts, || {}).unwrap();
        let got = wait_cwd(&pane).expect("PowerShell no informó la carpeta");
        let norm = |p: &std::path::Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
        assert_eq!(norm(&got), norm(&dir));
        assert!(
            !pane.with_emulator(|e| e.contains_text("9;9")),
            "el prompt no debe mostrar la secuencia"
        );
    }

    /// El mismo script en Windows PowerShell 5.1 (el que se usa si no hay pwsh), que parte la línea de
    /// comandos distinto.
    #[test]
    fn el_script_del_prompt_sirve_en_windows_powershell() {
        let dir = std::env::current_dir().unwrap();
        let mut opts = PaneOptions::default_shell(GridSize::new(120, 30));
        opts.program = "powershell.exe".into();
        opts.args.insert(1, "-NoProfile".into());
        opts.cwd = Some(dir.clone());
        let pane = Pane::spawn(opts, || {}).unwrap();
        let got = wait_cwd(&pane).expect("Windows PowerShell no informó la carpeta");
        assert!(
            got.to_string_lossy()
                .eq_ignore_ascii_case(dir.to_string_lossy().trim_end_matches('\\'))
        );
        assert!(wait_for(&pane, "PS "), "el prompt por defecto sigue visible");
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
