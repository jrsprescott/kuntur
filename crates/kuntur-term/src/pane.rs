use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use alacritty_terminal::event::Event;
use alacritty_terminal::term::TermMode;
use portable_pty::{Child, CommandBuilder, MasterPty, native_pty_system};

use crate::agent_signal::{AgentSignal, parse_7501};
use crate::cwd::cwd_from_osc;
use crate::emulator::Emulator;
use crate::osc::OscScanner;
use crate::paste::paste_bytes;
use crate::size::GridSize;

/// Qué programa abrir en el panel.
#[derive(Clone, Debug)]
pub struct PaneOptions {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub size: GridSize,
    /// Variables de entorno extra para el programa (p. ej. `KUNTUR_PIPE`, `KUNTUR_SESSION`).
    pub env: Vec<(String, String)>,
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
            env: Vec::new(),
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

/// Un programa corriendo en un PTY (ConPTY en Windows) conectado a su emulador.
pub struct Pane {
    emulator: Arc<Mutex<Emulator>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    exited: Arc<AtomicBool>,
    title: Arc<Mutex<Option<String>>>,
    clipboard: Arc<Mutex<Option<String>>>,
    pid: Option<u32>,
    cwd: Arc<Mutex<Option<PathBuf>>>,
    last_output: Arc<Mutex<Instant>>,
    agent_signal: Arc<Mutex<Option<AgentSignal>>>,
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
        for (key, value) in &opts.env {
            cmd.env(key, value);
        }

        let child = pair.slave.spawn_command(cmd)?;
        let pid = child.process_id();
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(pair.master.take_writer()?));
        let emulator = Arc::new(Mutex::new(Emulator::new(opts.size)));
        let exited = Arc::new(AtomicBool::new(false));
        let title = Arc::new(Mutex::new(None));
        let clipboard = Arc::new(Mutex::new(None));
        let cwd = Arc::new(Mutex::new(None));
        let last_output = Arc::new(Mutex::new(Instant::now()));
        let agent_signal = Arc::new(Mutex::new(None));

        {
            let agent_signal = agent_signal.clone();
            let (emulator, writer, exited, title, clipboard, cwd, last_output) = (
                emulator.clone(),
                writer.clone(),
                exited.clone(),
                title.clone(),
                clipboard.clone(),
                cwd.clone(),
                last_output.clone(),
            );
            thread::Builder::new().name("kuntur-pty-lector".into()).spawn(move || {
                let mut buf = vec![0u8; 64 * 1024];
                let mut scanner = OscScanner::new();
                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    *last_output.lock().unwrap() = Instant::now();
                    // Antes del emulador y sin tocar los bytes: carpeta nueva (OSC 7 / 9;9) y estado de
                    // un agente (OSC 7501).
                    scanner.feed(&buf[..n], |payload| {
                        if let Some(dir) = cwd_from_osc(payload) {
                            *cwd.lock().unwrap() = Some(PathBuf::from(dir));
                        } else if let Some(signal) = parse_7501(payload) {
                            *agent_signal.lock().unwrap() = Some(signal);
                        }
                    });
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
            pid,
            cwd,
            last_output,
            agent_signal,
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

    /// PID del programa (el shell de la sesión).
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Carpeta actual informada por el shell (OSC 7 / OSC 9;9); `None` hasta el primer aviso.
    pub fn cwd(&self) -> Option<PathBuf> {
        self.cwd.lock().unwrap().clone()
    }

    /// Último estado de agente informado por OSC 7501 desde la llamada anterior (y lo olvida).
    pub fn take_agent_signal(&self) -> Option<AgentSignal> {
        self.agent_signal.lock().unwrap().take()
    }

    /// Tiempo desde la última salida del programa.
    pub fn last_output_age(&self) -> Duration {
        self.last_output.lock().unwrap().elapsed()
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
            env: Vec::new(),
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
            env: Vec::new(),
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
            env: Vec::new(),
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
            env: Vec::new(),
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

    #[test]
    fn pasa_variables_y_lee_osc_7501() {
        let opts = PaneOptions {
            program: "cmd.exe".into(),
            args: vec![
                "/c".into(),
                "echo VAR=%KUNTUR_PRUEBA%& echo \x1b]7501;state=permission;agent=claude\x07listo-7501".into(),
            ],
            cwd: None,
            size: GridSize::new(80, 24),
            env: vec![("KUNTUR_PRUEBA".into(), "valor-123".into())],
        };
        let pane = Pane::spawn(opts, || {}).unwrap();
        assert!(wait_for(&pane, "VAR=valor-123"));
        assert!(wait_for(&pane, "listo-7501"));
        let signal = pane.take_agent_signal().expect("llegó la señal");
        assert_eq!(signal.state, crate::agent_signal::SignalState::Permission);
        assert_eq!(signal.agent.as_deref(), Some("claude"));
        assert!(pane.take_agent_signal().is_none(), "se entrega una sola vez");
        let screen: Vec<String> = pane.with_emulator(|e| (0..24).map(|r| e.line_text(r)).collect());
        assert!(!screen.iter().any(|l| l.contains("state=")), "{screen:?}");
    }
}
