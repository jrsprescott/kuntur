//! De dónde salen y adónde van los bytes de un panel: un PTY local (ConPTY) o un canal remoto
//! (SSH, en `kuntur-ssh`). El emulador no distingue: [`crate::Pane`] lee de un `Read` y escribe y
//! redimensiona a través de [`Transport`].

use std::io::Write;
use std::sync::Mutex;

use portable_pty::{Child, MasterPty};

use crate::size::GridSize;

/// Lado de escritura y control de un panel. Lo usa el hilo lector (respuestas al programa) y la UI
/// (teclas, tamaño), por eso es `Send + Sync`.
pub trait Transport: Send + Sync {
    /// Envía bytes al programa (teclas, pegado, respuestas del emulador).
    fn write(&self, bytes: &[u8]) -> std::io::Result<()>;
    /// Nuevo tamaño de la grilla.
    fn resize(&self, size: GridSize) -> anyhow::Result<()>;
    /// PID del programa local; `None` en transportes remotos.
    fn pid(&self) -> Option<u32> {
        None
    }
    /// ¿Terminó el programa? (además del fin del lector, que `Pane` ya detecta).
    fn has_exited(&self) -> bool {
        false
    }
    /// Termina el programa o cierra el canal (al cerrar la sesión).
    fn kill(&self) {}
}

/// Programa local en un PTY (ConPTY en Windows).
pub struct LocalPty {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    pid: Option<u32>,
}

impl LocalPty {
    pub fn new(
        master: Box<dyn MasterPty + Send>,
        writer: Box<dyn Write + Send>,
        child: Box<dyn Child + Send + Sync>,
    ) -> Self {
        let pid = child.process_id();
        Self {
            writer: Mutex::new(writer),
            master: Mutex::new(master),
            child: Mutex::new(child),
            pid,
        }
    }
}

impl Transport for LocalPty {
    fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        let mut w = self.writer.lock().unwrap();
        w.write_all(bytes)?;
        w.flush()
    }

    fn resize(&self, size: GridSize) -> anyhow::Result<()> {
        self.master.lock().unwrap().resize(size.into())?;
        Ok(())
    }

    fn pid(&self) -> Option<u32> {
        self.pid
    }

    fn has_exited(&self) -> bool {
        self.child.lock().unwrap().try_wait().ok().flatten().is_some()
    }

    fn kill(&self) {
        let _ = self.child.lock().unwrap().kill();
    }
}
