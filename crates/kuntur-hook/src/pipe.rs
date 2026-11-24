//! Named pipe local entre el gancho y Kuntur (`\\.\pipe\kuntur-<pid>`).
//!
//! - **Servidor** (Kuntur): un hilo espera clientes; cada conexión es un mensaje (el cliente escribe y
//!   cierra). Escuchan 4 instancias a la vez (ganchos de subagentes en paralelo). La primera
//!   instancia es exclusiva (otro proceso no puede adueñarse del nombre) y se rechazan clientes
//!   remotos.
//! - **Cliente** (el gancho): abre, escribe y cierra. Si el pipe no existe (Kuntur cerrado) vuelve en
//!   el acto; si está ocupado reintenta ~1 s como mucho. Nunca se queda colgado.

/// Tamaño máximo de un mensaje que acepta el servidor (lo demás se descarta).
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// Nombre del pipe de la instancia de Kuntur con ese PID.
pub fn pipe_name(pid: u32) -> String {
    format!(r"\\.\pipe\kuntur-{pid}")
}

#[cfg(windows)]
mod imp {
    use std::io::{self, Write};
    use std::os::windows::ffi::OsStrExt;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND, ReadFile};
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
    };

    use super::MAX_MESSAGE_BYTES;

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    /// Handle de una instancia del pipe (se cierra al soltarlo).
    struct Instance(HANDLE);
    // El handle es de este proceso y solo lo usa un hilo a la vez.
    unsafe impl Send for Instance {}

    impl Drop for Instance {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn create(name: &[u16], first: bool) -> io::Result<Instance> {
        let mut open = PIPE_ACCESS_INBOUND;
        if first {
            open |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        let mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
        let h = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                open,
                mode,
                PIPE_UNLIMITED_INSTANCES,
                0,
                64 * 1024,
                0,
                std::ptr::null(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Instance(h))
        }
    }

    /// Espera un cliente en la instancia. `true` si se conectó. Un cliente rápido puede conectarse,
    /// escribir y cerrar antes de `ConnectNamedPipe` (`ERROR_PIPE_CONNECTED` / `ERROR_NO_DATA`): lo
    /// que escribió sigue en el búfer y se lee igual.
    fn connect(inst: &Instance) -> bool {
        let ok = unsafe { ConnectNamedPipe(inst.0, std::ptr::null_mut()) };
        ok != 0 || matches!(unsafe { GetLastError() }, ERROR_PIPE_CONNECTED | ERROR_NO_DATA)
    }

    /// Lee hasta que el cliente cierra.
    fn read_all(inst: &Instance) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            let mut n = 0u32;
            let ok = unsafe { ReadFile(inst.0, buf.as_mut_ptr(), buf.len() as u32, &mut n, std::ptr::null_mut()) };
            if ok == 0 {
                // ERROR_BROKEN_PIPE: el cliente cerró (fin normal); cualquier otro error también termina.
                break;
            }
            if n == 0 {
                continue;
            }
            if out.len() < MAX_MESSAGE_BYTES {
                out.extend_from_slice(&buf[..n as usize]);
            }
        }
        out.truncate(MAX_MESSAGE_BYTES);
        out
    }

    /// Instancias que escuchan a la vez: hasta 4 ganchos (p. ej. subagentes en paralelo) se atienden
    /// sin esperar; los demás reintentan.
    pub const LISTENERS: usize = 4;

    /// Servidor del pipe; al soltarlo deja de escuchar.
    pub struct PipeServer {
        name: String,
        stop: Arc<AtomicBool>,
        threads: Vec<JoinHandle<()>>,
    }

    impl PipeServer {
        /// Empieza a escuchar en `name`; `on_message` recibe cada mensaje (desde los hilos del
        /// servidor). Falla si el nombre ya está en uso.
        pub fn start(name: &str, on_message: impl Fn(Vec<u8>) + Send + Sync + 'static) -> io::Result<Self> {
            let wname = wide(name);
            // La primera, exclusiva: si otro proceso ya tiene el nombre, falla aquí.
            let mut instances = vec![create(&wname, true)?];
            for _ in 1..LISTENERS {
                instances.push(create(&wname, false)?);
            }
            let stop = Arc::new(AtomicBool::new(false));
            let on_message: Arc<dyn Fn(Vec<u8>) + Send + Sync> = Arc::new(on_message);
            let mut threads = Vec::new();
            for inst in instances {
                let (stop, on_message) = (stop.clone(), on_message.clone());
                threads.push(std::thread::Builder::new().name("kuntur-pipe".into()).spawn(move || {
                    // Cada hilo reutiliza su instancia: conectar, leer, desconectar.
                    loop {
                        let connected = connect(&inst);
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        let msg = if connected { read_all(&inst) } else { Vec::new() };
                        unsafe {
                            DisconnectNamedPipe(inst.0);
                        }
                        if !msg.is_empty() {
                            on_message(msg);
                        }
                        if !connected {
                            std::thread::sleep(Duration::from_millis(50));
                        }
                    }
                })?);
            }
            Ok(Self {
                name: name.to_string(),
                stop,
                threads,
            })
        }

        pub fn name(&self) -> &str {
            &self.name
        }
    }

    impl Drop for PipeServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            // Una conexión vacía por hilo desbloquea su ConnectNamedPipe.
            for _ in 0..self.threads.len() {
                let _ = send(&self.name, b"");
            }
            for t in self.threads.drain(..) {
                let _ = t.join();
            }
        }
    }

    /// Manda un mensaje y cierra. Si Kuntur no escucha vuelve enseguida con error; si todas las
    /// instancias están ocupadas reintenta durante ~1 s como mucho.
    pub fn send(name: &str, bytes: &[u8]) -> io::Result<()> {
        let wname = wide(name);
        const ATTEMPTS: usize = 20;
        for attempt in 0..ATTEMPTS {
            match std::fs::OpenOptions::new().write(true).open(name) {
                Ok(mut f) => {
                    f.write_all(bytes)?;
                    return f.flush();
                }
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) && attempt + 1 < ATTEMPTS => unsafe {
                    WaitNamedPipeW(wname.as_ptr(), 50);
                },
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::from(io::ErrorKind::TimedOut))
    }
}

#[cfg(windows)]
pub use imp::{PipeServer, send};

#[cfg(not(windows))]
pub fn send(_name: &str, _bytes: &[u8]) -> std::io::Result<()> {
    Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
}
