//! Árbol remoto por SFTP (F7): un canal `subsystem sftp` sobre la **misma conexión** de la pestaña
//! (sin volver a autenticar), expuesto como `kuntur_files::FileSystem`.
//!
//! La interfaz es síncrona: cada operación bloquea sobre el runtime de SSH. La app la llama fuera
//! del hilo de la UI (lecturas en segundo plano e hilo de transferencias).

use std::future::Future;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};

use kuntur_files::{FileEntry, FileSystem, Listing, MAX_ENTRIES, PathStyle, sort_entries};
use russh_sftp::client::SftpSession;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::fs::File as SftpFile;
use russh_sftp::protocol::StatusCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::client::runtime;

/// Tiempo máximo de cada pedido SFTP.
const TIMEOUT_SECS: u64 = 30;
/// Enlaces simbólicos que se resuelven por carpeta (cada uno cuesta un `stat`).
const MAX_LINKS: usize = 200;

/// Ejecuta un `Future` del runtime de SSH desde código síncrono.
pub(crate) fn block<F: Future>(f: F) -> F::Output {
    match tokio::runtime::Handle::try_current() {
        // Dentro de un hilo del runtime (no debería pasar en la app): sin bloquear el reactor.
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(f)),
        Err(_) => runtime().block_on(f),
    }
}

fn io_err(e: SftpError) -> io::Error {
    match &e {
        SftpError::Status(s) if s.status_code == StatusCode::NoSuchFile => {
            io::Error::new(io::ErrorKind::NotFound, "no existe")
        }
        SftpError::Status(s) if s.status_code == StatusCode::PermissionDenied => {
            io::Error::new(io::ErrorKind::PermissionDenied, "sin permiso")
        }
        SftpError::Status(s) => io::Error::other(if s.error_message.is_empty() {
            s.status_code.to_string()
        } else {
            s.error_message.clone()
        }),
        _ => io::Error::from(e),
    }
}

fn is_not_found(e: &SftpError) -> bool {
    matches!(e, SftpError::Status(s) if s.status_code == StatusCode::NoSuchFile)
}

/// Sistema de archivos del servidor de una pestaña remota.
pub struct SftpFs {
    sftp: SftpSession,
    /// `usuario@host` (etiqueta del panel).
    label: String,
    home: Mutex<Option<String>>,
}

impl SftpFs {
    pub(crate) async fn open(stream: russh::ChannelStream<russh::client::Msg>, label: String) -> anyhow::Result<Self> {
        let sftp = SftpSession::new(stream).await?;
        sftp.set_timeout(TIMEOUT_SECS);
        Ok(Self {
            sftp,
            label,
            home: Mutex::new(None),
        })
    }

    fn entry(name: String, path: String, attrs: &russh_sftp::protocol::FileAttributes) -> FileEntry {
        let is_dir = attrs.is_dir();
        FileEntry {
            name,
            path,
            is_dir,
            size: if is_dir { 0 } else { attrs.size.unwrap_or(0) },
            modified: attrs.mtime.map(i64::from),
        }
    }
}

impl FileSystem for SftpFs {
    fn style(&self) -> PathStyle {
        PathStyle::Posix
    }

    fn remote_label(&self) -> Option<String> {
        Some(self.label.clone())
    }

    fn home(&self) -> io::Result<String> {
        if let Some(h) = self.home.lock().unwrap().clone() {
            return Ok(h);
        }
        let home = block(self.sftp.canonicalize(".")).map_err(io_err)?;
        *self.home.lock().unwrap() = Some(home.clone());
        Ok(home)
    }

    fn list(&self, dir: &str) -> io::Result<Listing> {
        let read = block(self.sftp.read_dir(dir)).map_err(io_err)?;
        let mut entries = Vec::new();
        let mut truncated = false;
        let mut links = 0;
        for e in read {
            let name = e.file_name();
            if name == "." || name == ".." {
                continue;
            }
            if entries.len() >= MAX_ENTRIES {
                truncated = true;
                break;
            }
            let path = self.join(dir, &name);
            let mut attrs = e.metadata();
            // Un enlace a una carpeta debe poder abrirse: se mira a dónde apunta.
            if e.file_type().is_symlink() && links < MAX_LINKS {
                links += 1;
                if let Ok(target) = block(self.sftp.metadata(path.as_str())) {
                    attrs = target;
                }
            }
            entries.push(Self::entry(name, path, &attrs));
        }
        sort_entries(&mut entries);
        Ok(Listing { entries, truncated })
    }

    fn stat(&self, path: &str) -> io::Result<Option<FileEntry>> {
        match block(self.sftp.metadata(path)) {
            Ok(attrs) => {
                let name = self.file_name(path).unwrap_or_else(|| "/".into());
                Ok(Some(Self::entry(name, path.to_string(), &attrs)))
            }
            Err(e) if is_not_found(&e) => Ok(None),
            Err(e) => Err(io_err(e)),
        }
    }

    fn open_read(&self, path: &str) -> io::Result<Box<dyn Read + Send>> {
        let file = block(self.sftp.open(path)).map_err(io_err)?;
        Ok(Box::new(SftpReader(Some(file))))
    }

    fn create(&self, path: &str) -> io::Result<Box<dyn Write + Send>> {
        let file = block(self.sftp.create(path)).map_err(io_err)?;
        Ok(Box::new(SftpWriter(Some(file))))
    }

    fn create_dir(&self, path: &str) -> io::Result<()> {
        block(self.sftp.create_dir(path)).map_err(io_err)
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        block(self.sftp.rename(from, to)).map_err(io_err)
    }

    fn remove_file(&self, path: &str) -> io::Result<()> {
        block(self.sftp.remove_file(path)).map_err(io_err)
    }

    fn remove_dir(&self, path: &str) -> io::Result<()> {
        block(self.sftp.remove_dir(path)).map_err(io_err)
    }
}

struct SftpReader(Option<SftpFile>);

impl Read for SftpReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.as_mut() {
            Some(f) => block(f.read(buf)),
            None => Ok(0),
        }
    }
}

impl Drop for SftpReader {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            let _ = block(f.close());
        }
    }
}

struct SftpWriter(Option<SftpFile>);

impl Write for SftpWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.0.as_mut() {
            Some(f) => block(f.write_all(buf)).map(|()| buf.len()),
            None => Err(io::Error::other("archivo cerrado")),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.0.as_mut() {
            Some(f) => block(f.flush()),
            None => Ok(()),
        }
    }
}

impl Drop for SftpWriter {
    fn drop(&mut self) {
        if let Some(mut f) = self.0.take() {
            let _ = block(async move {
                let _ = f.flush().await;
                f.close().await
            });
        }
    }
}

/// SFTP en caché de una [`crate::RemoteSession`]: se rehace si la conexión cambió.
#[derive(Default)]
pub(crate) struct SftpSlot(pub(crate) Mutex<Option<(crate::client::Connection, Arc<SftpFs>)>>);
