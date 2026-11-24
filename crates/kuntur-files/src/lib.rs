//! Panel Archivos: listado de una carpeta (carpetas primero), tamaños y fechas para mostrar, el
//! trait [`FileSystem`] (local, SFTP, memoria) y la cola de [`transfer`]encias. Sin UI: la app lo
//! llama fuera del hilo de la interfaz.
//!
//! Más adelante: portapapeles de archivos (CF_HDROP) y arrastre hacia el Explorador
//! (ver docs/PLAN.md §5.6).

use std::cmp::Ordering;
use std::path::Path;

pub mod fs;
pub mod memory;
pub mod path;
pub mod transfer;

pub use fs::{FileSystem, LocalFs};
pub use memory::MemoryFs;
pub use path::{PathStyle, agent_dir, conflict_name, location_label, posix_quote_all};
pub use transfer::{Conflict, Direction, Job, JobId, JobState, Queue, Summary, Transfers};

use jiff::Timestamp;
use jiff::tz::TimeZone;

/// Tope de entradas por carpeta: una carpeta enorme no debe congelar ni inflar la app.
pub const MAX_ENTRIES: usize = 2000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    /// Ruta completa en el estilo de su sistema de archivos (Windows o POSIX).
    pub path: String,
    pub is_dir: bool,
    /// Bytes (0 en carpetas).
    pub size: u64,
    /// Última modificación, segundos Unix.
    pub modified: Option<i64>,
}

impl FileEntry {
    /// Entrada a partir de los metadatos locales.
    pub fn from_metadata(name: String, path: String, meta: &std::fs::Metadata) -> Self {
        let is_dir = meta.is_dir();
        Self {
            name,
            path,
            is_dir,
            size: if is_dir { 0 } else { meta.len() },
            modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
        }
    }
}

/// Resultado de listar una carpeta.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub entries: Vec<FileEntry>,
    /// Había más de [`MAX_ENTRIES`] entradas y se cortó la lista.
    pub truncated: bool,
}

/// Lista `dir` (sin recursión): carpetas primero, luego archivos, por nombre sin distinguir
/// mayúsculas. En Windows omite lo oculto o de sistema (`desktop.ini`, `$RECYCLE.BIN`…).
pub fn list_dir(dir: &Path) -> std::io::Result<Listing> {
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry in std::fs::read_dir(dir)?.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if is_hidden(&meta) {
            continue;
        }
        if entries.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        entries.push(FileEntry::from_metadata(
            entry.file_name().to_string_lossy().into_owned(),
            entry.path().to_string_lossy().into_owned(),
            &meta,
        ));
    }
    sort_entries(&mut entries);
    Ok(Listing { entries, truncated })
}

#[cfg(windows)]
fn is_hidden(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    meta.file_attributes() & (HIDDEN | SYSTEM) != 0
}

#[cfg(not(windows))]
fn is_hidden(_meta: &std::fs::Metadata) -> bool {
    false
}

/// Carpetas primero; dentro de cada grupo, por nombre sin distinguir mayúsculas (y luego tal cual,
/// para que el orden sea estable).
pub fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| compare_names(&a.name, &b.name)));
}

fn compare_names(a: &str, b: &str) -> Ordering {
    let lower = |s: &str| s.chars().flat_map(char::to_lowercase).collect::<String>();
    lower(a).cmp(&lower(b)).then_with(|| a.cmp(b))
}

/// Tamaño legible con coma decimal: "0 B", "812 B", "4,2 KB", "13 MB", "1,5 GB".
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    // Un decimal solo por debajo de 10 ("4,2 KB"); desde 10, entero ("13 MB").
    let text = if value < 10.0 {
        let rounded = (value * 10.0).round() / 10.0;
        if rounded >= 10.0 {
            "10".to_string()
        } else {
            format!("{rounded:.1}").replace('.', ",")
        }
    } else {
        format!("{}", value.round() as u64)
    };
    format!("{text} {}", UNITS[unit])
}

const MONTHS: [&str; 12] = [
    "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
];

/// Fecha de modificación corta en la zona `tz`: "14:05" (hoy), "ayer", "12 nov" (este año) o
/// "12 nov 2025".
pub fn format_modified(ts: i64, now: i64, tz: &TimeZone) -> String {
    let (Ok(t), Ok(n)) = (Timestamp::from_second(ts), Timestamp::from_second(now)) else {
        return String::new();
    };
    let (t, n) = (t.to_zoned_datetime(tz.clone()), n.to_zoned_datetime(tz.clone()));
    let (day, today) = (t.date(), n.date());
    if day == today {
        return format!("{:02}:{:02}", t.hour(), t.minute());
    }
    if today.yesterday().is_ok_and(|y| y == day) {
        return "ayer".to_string();
    }
    let month = MONTHS[(day.month() - 1) as usize];
    if day.year() == today.year() {
        format!("{} {month}", day.day())
    } else {
        format!("{} {month} {}", day.day(), day.year())
    }
}

/// Ruta para mostrar: la carpeta personal se abrevia como `~` (`~\proyectos\web`).
pub fn display_path(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home
        && let Ok(rest) = path.strip_prefix(home)
    {
        let rest = rest.to_string_lossy();
        return if rest.is_empty() {
            "~".to_string()
        } else {
            format!("~{}{rest}", std::path::MAIN_SEPARATOR)
        };
    }
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_dir: bool) -> FileEntry {
        FileEntry {
            name: name.into(),
            path: name.into(),
            is_dir,
            size: 0,
            modified: None,
        }
    }

    #[test]
    fn carpetas_primero_y_por_nombre_sin_mayusculas() {
        let mut v = vec![
            entry("zeta.txt", false),
            entry("src", true),
            entry("Alfa.md", false),
            entry("Docs", true),
            entry("alfa.md", false),
            entry("beta", false),
        ];
        sort_entries(&mut v);
        let names: Vec<_> = v.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Docs", "src", "Alfa.md", "alfa.md", "beta", "zeta.txt"]);
    }

    #[test]
    fn tamanos_legibles() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(812), "812 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1,0 KB");
        assert_eq!(format_size(4300), "4,2 KB");
        assert_eq!(format_size(10_239), "10 KB");
        assert_eq!(format_size(13 * 1024 * 1024 + 300_000), "13 MB");
        assert_eq!(format_size(1_610_612_736), "1,5 GB");
        assert_eq!(format_size(u64::MAX), "16777216 TB");
    }

    #[test]
    fn fechas_relativas_en_hora_local() {
        let tz = TimeZone::fixed(jiff::tz::offset(-5));
        // 2026-11-23 15:00 en UTC-5 = 20:00 UTC.
        let now = "2026-11-23T20:00:00Z".parse::<Timestamp>().unwrap().as_second();
        let at = |s: &str| s.parse::<Timestamp>().unwrap().as_second();
        // 09:05 local de hoy.
        assert_eq!(format_modified(at("2026-11-23T14:05:00Z"), now, &tz), "09:05");
        // 01:00 UTC del 23 = 20:00 local del 22 → ayer.
        assert_eq!(format_modified(at("2026-11-23T01:00:00Z"), now, &tz), "ayer");
        assert_eq!(format_modified(at("2026-11-12T15:00:00Z"), now, &tz), "12 nov");
        assert_eq!(format_modified(at("2025-01-03T15:00:00Z"), now, &tz), "3 ene 2025");
    }

    #[test]
    fn abrevia_la_carpeta_personal() {
        let home = Path::new("/home/u");
        assert_eq!(display_path(Path::new("/home/u"), Some(home)), "~");
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(
            display_path(&home.join("proyectos").join("web"), Some(home)),
            format!("~{sep}proyectos{sep}web")
        );
        assert_eq!(display_path(Path::new("/srv/x"), Some(home)), "/srv/x");
        assert_eq!(display_path(Path::new("/home/usuario2"), Some(home)), "/home/usuario2");
        assert_eq!(display_path(Path::new("/srv"), None), "/srv");
    }

    #[test]
    fn lista_una_carpeta_real() {
        let base = std::env::temp_dir().join(format!("kuntur-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("sub")).unwrap();
        std::fs::write(base.join("b.txt"), "hola").unwrap();
        std::fs::write(base.join("A.txt"), "").unwrap();
        let listing = list_dir(&base).unwrap();
        let _ = std::fs::remove_dir_all(&base);
        let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["sub", "A.txt", "b.txt"]);
        assert_eq!(listing.entries[2].size, 4);
        assert!(listing.entries[0].is_dir && listing.entries[0].size == 0);
        assert!(listing.entries[1].modified.is_some());
        assert!(!listing.truncated);
        assert!(list_dir(&base).is_err());
    }
}
