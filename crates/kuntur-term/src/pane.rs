use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use alacritty_terminal::event::Event;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::TermMode;
use portable_pty::{Child, CommandBuilder, MasterPty, native_pty_system};

use crate::cwd::{CwdScanner, OscEvent};
use crate::emulator::{Emulator, InputLine};
use crate::paste::paste_bytes;
use crate::shell::{ShellPhase, ShellState};
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
    /// PowerShell 7 si está instalado; si no, Windows PowerShell. Envuelve el `prompt` (el del perfil
    /// del usuario, si lo hay) para que además informe la carpeta actual con OSC 9;9, como Windows
    /// Terminal, y las marcas de prompt OSC 133; lo que el usuario ve no cambia.
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
/// de comandos). Envuelve el `prompt` original (el del perfil del usuario, si lo hay); lo que el
/// usuario ve no cambia. Al texto del prompt le agrega, invisibles:
/// - `ESC ] 133 ; D ; <código> ESC \` si terminó un comando desde el prompt anterior (`Get-History`
///   cambió; el primer prompt no cuenta: el propio `-Command` puede entrar al historial); el código
///   es 0 si `$?` es verdadero, si no `$LASTEXITCODE` (o 1),
/// - `ESC ] 9 ; 9 ; "<carpeta>" ESC \` (carpeta actual, como Windows Terminal),
/// - `ESC ] 133 ; A ESC \` antes del prompt y `ESC ] 133 ; B ESC \` después: ahí empieza lo que
///   escribe el usuario (ver [`crate::shell`]).
const PROMPT_HOOK: &str = concat!(
    "$global:__kunturPrompt = $function:prompt; $global:__kunturId = -1; ",
    "function global:prompt { ",
    "$ok = $?; $c = $global:LASTEXITCODE; ",
    "$e = [string][char]27; $t = $e + '\\'; $d = ''; $w = ''; ",
    "$h = [int](Get-History -Count 1).Id; $p = $global:__kunturId; $global:__kunturId = $h; ",
    "if ($p -ge 0 -and $h -ne $p) { ",
    "$n = if ($ok) { 0 } elseif ($c) { $c } else { 1 }; ",
    "$d = $e + ']133;D;' + $n + $t } ",
    "$l = $executionContext.SessionState.Path.CurrentLocation; ",
    "$o = (& $global:__kunturPrompt) -join ''; ",
    "if ($l.Provider.Name -eq 'FileSystem') { ",
    "$q = [string][char]34; $w = $e + ']9;9;' + $q + $l.ProviderPath + $q + $t } ",
    "$d + $w + $e + ']133;A' + $t + $o + $e + ']133;B' + $t }"
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
    shell: Arc<Mutex<ShellState>>,
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
        let shell = Arc::new(Mutex::new(ShellState::default()));

        {
            let shell = shell.clone();
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
                let mut scanner = CwdScanner::new();
                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    *last_output.lock().unwrap() = Instant::now();
                    // Sin tocar los bytes: mira si pasó una carpeta nueva o una marca OSC 133. El
                    // emulador se alimenta por tramos para saber dónde quedó el cursor en cada marca.
                    let chunk = &buf[..n];
                    let events = {
                        let mut emu = emulator.lock().unwrap();
                        let mut fed = 0;
                        for (end, ev) in scanner.feed_events(chunk) {
                            match ev {
                                OscEvent::Cwd(dir) => *cwd.lock().unwrap() = Some(PathBuf::from(dir)),
                                OscEvent::Mark(mark) => {
                                    emu.feed(&chunk[fed..end]);
                                    fed = end;
                                    shell.lock().unwrap().apply(mark, emu.cursor_abs());
                                }
                            }
                        }
                        emu.feed(&chunk[fed..]);
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
            shell,
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

    /// Estado de la integración de shell (OSC 133).
    pub fn shell_state(&self) -> ShellState {
        self.shell.lock().unwrap().clone()
    }

    /// Bloques visibles en pantalla: (fila inicial, fila final, código de salida), recortados a
    /// las filas visibles.
    pub fn visible_blocks(&self) -> Vec<(usize, usize, Option<i32>)> {
        let blocks = self.shell.lock().unwrap().blocks.clone();
        self.with_emulator(|e| {
            let rows = e.term().screen_lines() as i64;
            blocks
                .iter()
                .filter_map(|b| {
                    let (top, bottom) = (e.abs_to_row(b.start), e.abs_to_row(b.end));
                    (bottom >= 0 && top < rows).then(|| (top.max(0) as usize, bottom.min(rows - 1) as usize, b.exit))
                })
                .collect()
        })
    }

    /// Lo que el usuario lleva escrito en el prompt; `None` si el shell no está esperando entrada
    /// (corre un comando, `claude`, `vim`…), si no informa marcas OSC 133 o si no se puede leer.
    pub fn prompt_input(&self) -> Option<InputLine> {
        let start = self.shell.lock().unwrap().at_input()?;
        self.with_emulator(|e| e.input_since(start))
    }

    /// El usuario envió la línea con Enter: desde aquí corre un comando (marca `C` de Kuntur).
    pub fn mark_command_start(&self) {
        let at = self.with_emulator(|e| e.cursor_abs());
        let mut shell = self.shell.lock().unwrap();
        if shell.phase == ShellPhase::Input {
            shell.command_started(at);
        }
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

    fn wait_until(mut f: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if f() {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    }

    /// OSC 133 de verdad a través de ConPTY con Windows PowerShell 5.1: Kuntur sabe dónde empieza
    /// la entrada, lee lo escrito y recibe el código de salida del comando.
    #[test]
    fn powershell_marca_el_prompt_con_osc_133() {
        let mut opts = PaneOptions::default_shell(GridSize::new(120, 30));
        opts.program = "powershell.exe".into();
        opts.args.insert(1, "-NoProfile".into());
        opts.cwd = Some(std::env::current_dir().unwrap());
        let pane = Pane::spawn(opts, || {}).unwrap();
        assert!(
            wait_until(|| pane.prompt_input().is_some()),
            "no llegó la marca B: {:?}",
            pane.shell_state()
        );
        assert_eq!(
            pane.prompt_input().unwrap().text,
            "",
            "la entrada empieza vacía (sin el prompt)"
        );
        pane.write(b"echo kuntur-133").unwrap();
        assert!(wait_until(|| pane
            .prompt_input()
            .is_some_and(|l| l.text == "echo kuntur-133")));
        let l = pane.prompt_input().unwrap();
        assert!(l.at_end);
        assert!(!pane.with_emulator(|e| e.contains_text("133;")), "las marcas no se ven");

        pane.mark_command_start();
        assert_eq!(pane.shell_state().phase, ShellPhase::Running);
        assert_eq!(pane.prompt_input(), None, "mientras corre no hay entrada");
        pane.write(b"\r").unwrap();
        assert!(wait_until(|| pane.shell_state().finished == 1));
        assert_eq!(pane.shell_state().last_exit, Some(0));
        // El comando enviado con Enter desde Kuntur queda como bloque visible, con su código.
        let blocks = pane.visible_blocks();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].2, Some(0));
        assert!(blocks[0].1 > blocks[0].0, "comando + salida: {blocks:?}");
        assert!(wait_until(|| pane.prompt_input().is_some()));

        pane.write(b"cmd /c exit 3\r").unwrap();
        assert!(wait_until(|| pane.shell_state().finished == 2));
        assert_eq!(pane.shell_state().last_exit, Some(3));
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
