//! Subsistema SFTP del servidor de prueba: sirve una carpeta temporal como si fuera la raÃ­z `/`
//! del servidor. Solo para las pruebas (en proceso, sin red real).

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use russh_sftp::protocol::{
    Attrs, Data, File as SftpFile, FileAttributes, Handle, Name, OpenFlags, SftpHandle, SftpString, Status, StatusCode,
    Version,
};

enum Open {
    Dir { path: PathBuf, sent: bool },
    File(File),
}

pub struct SftpServer {
    root: PathBuf,
    handles: HashMap<Vec<u8>, Open>,
    next: u64,
}

impl SftpServer {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            handles: HashMap::new(),
            next: 0,
        }
    }

    /// Ruta POSIX normalizada (`.`, `..`, sin `/` repetidas).
    fn normalize(path: &str) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for p in path.split('/') {
            match p {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                p => parts.push(p),
            }
        }
        format!("/{}", parts.join("/"))
    }

    fn local(&self, path: &SftpString) -> PathBuf {
        let norm = Self::normalize(path);
        let mut out = self.root.clone();
        for part in norm.split('/').filter(|p| !p.is_empty()) {
            out.push(part);
        }
        out
    }

    fn handle(&mut self, open: Open) -> SftpHandle {
        self.next += 1;
        let h = format!("h{}", self.next).into_bytes();
        self.handles.insert(h.clone(), open);
        SftpHandle::from(h)
    }

    fn ok(id: u32) -> Status {
        Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".into(),
            language_tag: "en-US".into(),
        }
    }
}

fn code(e: std::io::Error) -> StatusCode {
    match e.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        _ => StatusCode::Failure,
    }
}

fn attrs(path: &Path) -> Result<FileAttributes, StatusCode> {
    std::fs::metadata(path).map(|m| FileAttributes::from(&m)).map_err(code)
}

impl russh_sftp::server::Handler for SftpServer {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(&mut self, _version: u32, _ext: HashMap<String, String>) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, path: SftpString) -> Result<Name, Self::Error> {
        let p = path.as_str();
        // La carpeta personal del usuario de prueba es `/home/kuntur`.
        let resolved = if p.is_empty() || p == "." {
            "/home/kuntur".to_string()
        } else {
            Self::normalize(p)
        };
        Ok(Name {
            id,
            files: vec![SftpFile::dummy(resolved)],
        })
    }

    async fn opendir(&mut self, id: u32, path: SftpString) -> Result<Handle, Self::Error> {
        let local = self.local(&path);
        if !local.is_dir() {
            return Err(StatusCode::NoSuchFile);
        }
        let handle = self.handle(Open::Dir {
            path: local,
            sent: false,
        });
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: SftpHandle) -> Result<Name, Self::Error> {
        let Some(Open::Dir { path, sent }) = self.handles.get_mut(handle.as_bytes()) else {
            return Err(StatusCode::Failure);
        };
        if *sent {
            return Err(StatusCode::Eof);
        }
        *sent = true;
        let mut files = vec![
            SftpFile::new(".", FileAttributes::dummy()),
            SftpFile::new("..", FileAttributes::dummy()),
        ];
        for e in std::fs::read_dir(path).map_err(code)?.flatten() {
            if let Ok(m) = e.metadata() {
                files.push(SftpFile::new(
                    e.file_name().to_string_lossy().into_owned(),
                    FileAttributes::from(&m),
                ));
            }
        }
        Ok(Name { id, files })
    }

    async fn open(
        &mut self,
        id: u32,
        filename: SftpString,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let local = self.local(&filename);
        let file = OpenOptions::from(pflags).open(&local).map_err(code)?;
        let handle = self.handle(Open::File(file));
        Ok(Handle { id, handle })
    }

    async fn read(&mut self, id: u32, handle: SftpHandle, offset: u64, len: u32) -> Result<Data, Self::Error> {
        let Some(Open::File(f)) = self.handles.get_mut(handle.as_bytes()) else {
            return Err(StatusCode::Failure);
        };
        f.seek(SeekFrom::Start(offset)).map_err(code)?;
        let mut data = vec![0u8; len as usize];
        let n = f.read(&mut data).map_err(code)?;
        if n == 0 {
            return Err(StatusCode::Eof);
        }
        data.truncate(n);
        Ok(Data { id, data })
    }

    async fn write(&mut self, id: u32, handle: SftpHandle, offset: u64, data: Vec<u8>) -> Result<Status, Self::Error> {
        let Some(Open::File(f)) = self.handles.get_mut(handle.as_bytes()) else {
            return Err(StatusCode::Failure);
        };
        f.seek(SeekFrom::Start(offset)).map_err(code)?;
        f.write_all(&data).map_err(code)?;
        Ok(Self::ok(id))
    }

    async fn close(&mut self, id: u32, handle: SftpHandle) -> Result<Status, Self::Error> {
        self.handles.remove(handle.as_bytes());
        Ok(Self::ok(id))
    }

    async fn stat(&mut self, id: u32, path: SftpString) -> Result<Attrs, Self::Error> {
        Ok(Attrs {
            id,
            attrs: attrs(&self.local(&path))?,
        })
    }

    async fn lstat(&mut self, id: u32, path: SftpString) -> Result<Attrs, Self::Error> {
        self.stat(id, path).await
    }

    async fn fstat(&mut self, id: u32, handle: SftpHandle) -> Result<Attrs, Self::Error> {
        match self.handles.get(handle.as_bytes()) {
            Some(Open::File(f)) => Ok(Attrs {
                id,
                attrs: FileAttributes::from(&f.metadata().map_err(code)?),
            }),
            Some(Open::Dir { path, .. }) => Ok(Attrs {
                id,
                attrs: attrs(path)?,
            }),
            None => Err(StatusCode::Failure),
        }
    }

    async fn setstat(&mut self, id: u32, _path: SftpString, _attrs: FileAttributes) -> Result<Status, Self::Error> {
        Ok(Self::ok(id))
    }

    async fn fsetstat(&mut self, id: u32, _h: SftpHandle, _attrs: FileAttributes) -> Result<Status, Self::Error> {
        Ok(Self::ok(id))
    }

    async fn mkdir(&mut self, id: u32, path: SftpString, _attrs: FileAttributes) -> Result<Status, Self::Error> {
        std::fs::create_dir(self.local(&path)).map_err(code)?;
        Ok(Self::ok(id))
    }

    async fn rmdir(&mut self, id: u32, path: SftpString) -> Result<Status, Self::Error> {
        std::fs::remove_dir(self.local(&path)).map_err(code)?;
        Ok(Self::ok(id))
    }

    async fn remove(&mut self, id: u32, filename: SftpString) -> Result<Status, Self::Error> {
        std::fs::remove_file(self.local(&filename)).map_err(code)?;
        Ok(Self::ok(id))
    }

    async fn rename(&mut self, id: u32, oldpath: SftpString, newpath: SftpString) -> Result<Status, Self::Error> {
        std::fs::rename(self.local(&oldpath), self.local(&newpath)).map_err(code)?;
        Ok(Self::ok(id))
    }
}
