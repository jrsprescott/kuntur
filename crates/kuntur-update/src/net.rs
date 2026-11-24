//! Red del actualizador: la lista de versiones y la descarga del instalador.
//!
//! - Solo `api.github.com` y las descargas de GitHub; User-Agent `Kuntur/<versión>`; sin cookies
//!   ni datos del usuario.
//! - La descarga va a un archivo `.parcial` y calcula el SHA-256 mientras llega; solo si coincide
//!   se renombra al nombre final. Si no coincide se borra: nunca queda un instalador sin verificar.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::release::{Installer, Release, parse_releases};

/// Límite de la respuesta de la lista de versiones.
const LIST_LIMIT: u64 = 4 * 1024 * 1024;
/// Límite del instalador (el de hoy pesa ~13 MB).
const INSTALLER_LIMIT: u64 = 512 * 1024 * 1024;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .user_agent(crate::user_agent())
        .build()
        .into()
}

/// Pide la lista de versiones publicadas.
pub fn fetch_releases(url: &str) -> Result<Vec<Release>, String> {
    let mut resp = agent()
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(|e| format!("no se pudo consultar las versiones: {e}"))?;
    let text = resp
        .body_mut()
        .with_config()
        .limit(LIST_LIMIT)
        .read_to_string()
        .map_err(|e| format!("respuesta incompleta: {e}"))?;
    parse_releases(&text).map_err(|e| format!("respuesta inesperada de GitHub: {e}"))
}

/// Por qué falló una descarga.
#[derive(Debug, PartialEq, Eq)]
pub enum DownloadError {
    Cancelled,
    /// El SHA-256 del archivo no es el publicado.
    Checksum {
        expected: String,
        got: String,
    },
    Io(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::Cancelled => write!(f, "descarga cancelada"),
            DownloadError::Checksum { .. } => {
                write!(f, "el instalador descargado no coincide con el publicado (SHA-256)")
            }
            DownloadError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Copia `reader` en `writer` calculando el SHA-256 (hex en minúsculas); `progress(bytes)` tras
/// cada bloque; para si `cancel` se activa.
pub fn copy_hashing(
    mut reader: impl Read,
    mut writer: impl Write,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<(u64, String), DownloadError> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::Cancelled);
        }
        let n = reader.read(&mut buf).map_err(|e| DownloadError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        writer
            .write_all(&buf[..n])
            .map_err(|e| DownloadError::Io(e.to_string()))?;
        total += n as u64;
        progress(total);
    }
    writer.flush().map_err(|e| DownloadError::Io(e.to_string()))?;
    Ok((total, hex(&hasher.finalize())))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 de un archivo ya descargado.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut f, &mut hasher)?;
    Ok(hex(&hasher.finalize()))
}

/// Guarda el contenido de `reader` en `dir/<name>` solo si su SHA-256 es `expected`.
pub fn save_verified(
    reader: impl Read,
    dir: &Path,
    name: &str,
    expected: &str,
    cancel: &AtomicBool,
    progress: impl FnMut(u64),
) -> Result<PathBuf, DownloadError> {
    // El nombre viene de GitHub: solo el último componente, sin rutas.
    let safe = Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty() && n != "." && n != "..")
        .ok_or_else(|| DownloadError::Io("nombre de instalador inválido".into()))?;
    std::fs::create_dir_all(dir).map_err(|e| DownloadError::Io(e.to_string()))?;
    let part = dir.join(format!("{safe}.parcial"));
    let file = std::fs::File::create(&part).map_err(|e| DownloadError::Io(e.to_string()))?;
    let result = copy_hashing(reader, std::io::BufWriter::new(file), cancel, progress);
    let (_, got) = match result {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
    };
    if !got.eq_ignore_ascii_case(expected) {
        let _ = std::fs::remove_file(&part);
        return Err(DownloadError::Checksum {
            expected: expected.to_ascii_lowercase(),
            got,
        });
    }
    let dest = dir.join(&safe);
    let _ = std::fs::remove_file(&dest);
    std::fs::rename(&part, &dest).map_err(|e| DownloadError::Io(e.to_string()))?;
    Ok(dest)
}

/// Descarga el instalador a `dir`, verificando su SHA-256. `progress(bytes, total)`.
pub fn download(
    installer: &Installer,
    dir: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<PathBuf, DownloadError> {
    let resp = agent()
        .get(&installer.url)
        .call()
        .map_err(|e| DownloadError::Io(format!("no se pudo descargar: {e}")))?;
    let total = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .or((installer.size > 0).then_some(installer.size));
    let reader = resp.into_body().into_with_config().limit(INSTALLER_LIMIT).reader();
    save_verified(reader, dir, &installer.name, &installer.sha256, cancel, |n| {
        progress(n, total)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SHA-256 de "abc" (vector de prueba de FIPS 180-2).
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kuntur-update-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn calcula_el_sha256_mientras_copia() {
        let mut out = Vec::new();
        let mut seen = 0;
        let (n, sha) = copy_hashing(&b"abc"[..], &mut out, &AtomicBool::new(false), |b| seen = b).unwrap();
        assert_eq!((n, seen, out.as_slice()), (3, 3, &b"abc"[..]));
        assert_eq!(sha, ABC);
    }

    #[test]
    fn guarda_solo_si_el_hash_coincide() {
        let dir = tmp("ok");
        let path = save_verified(
            &b"abc"[..],
            &dir,
            "kuntur-9.9.9-x86_64.msi",
            &ABC.to_uppercase(),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(path, dir.join("kuntur-9.9.9-x86_64.msi"));
        assert_eq!(sha256_file(&path).unwrap(), ABC);
        assert!(!dir.join("kuntur-9.9.9-x86_64.msi.parcial").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hash_distinto_borra_el_archivo() {
        let dir = tmp("mal");
        let err = save_verified(&b"abd"[..], &dir, "k.msi", ABC, &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(matches!(err, DownloadError::Checksum { .. }));
        assert!(!dir.join("k.msi").exists() && !dir.join("k.msi.parcial").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelar_y_nombres_con_ruta() {
        let dir = tmp("cancelar");
        let err = save_verified(&b"abc"[..], &dir, "k.msi", ABC, &AtomicBool::new(true), |_| {}).unwrap_err();
        assert_eq!(err, DownloadError::Cancelled);
        assert!(!dir.join("k.msi.parcial").exists());
        // Un nombre con carpetas no sale de `dir`.
        let p = save_verified(
            &b"abc"[..],
            &dir,
            "../../fuera.msi",
            ABC,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(p, dir.join("fuera.msi"));
        assert!(save_verified(&b"abc"[..], &dir, "..", ABC, &AtomicBool::new(false), |_| {}).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
