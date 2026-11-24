//! Imágenes como Warp (ajustes v0.2): lógica sin GPUI, con pruebas.
//!
//! El usuario pega con Ctrl+V (o arrastra un archivo de imagen) y Kuntur decide cómo le llega al
//! agente de esa terminal:
//! - **Tecla del agente** (Claude Code local): el agente lee el portapapeles él mismo y muestra
//!   `[Image #N]`; Kuntur solo le envía su tecla (Alt+V en Windows, Ctrl+V en Linux/Mac).
//! - **Ruta local**: sin agente, o un agente que no lee imágenes del portapapeles: se guarda un PNG
//!   en la carpeta temporal de Kuntur y se pega su ruta (sin Enter).
//! - **Ruta remota**: en una pestaña SSH el agente no ve el portapapeles de esta PC: el PNG se sube
//!   por SFTP a la carpeta "para el agente" y se pega la ruta del servidor.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use kuntur_agents::{ImagePaste, ShellFlavor, quote_arg};

/// Lado máximo (px) de la imagen que se guarda: más grande no aporta al modelo y pesa más.
pub const MAX_SIDE: u32 = 2000;
/// Las capturas guardadas de más de 7 días se borran al arrancar.
pub const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const PREFIX: &str = "captura-";
/// Formatos que se aceptan como imagen (al pegar archivos o arrastrarlos).
const EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

/// Cómo le llega la imagen al agente.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// Escribir estos bytes (la tecla de pegar imagen del agente).
    Key(Vec<u8>),
    /// Guardar el PNG aquí y pegar su ruta.
    LocalPath,
    /// Subir el PNG al servidor y pegar la ruta remota.
    RemotePath,
}

/// `agent`: cómo pega imágenes el agente que corre en esa terminal (`None` = solo el shell).
pub fn choose(agent: Option<ImagePaste>, remote: bool) -> Delivery {
    if remote {
        return Delivery::RemotePath;
    }
    match agent {
        Some(ImagePaste::Key(key)) => Delivery::Key(key.bytes()),
        Some(ImagePaste::Path) | None => Delivery::LocalPath,
    }
}

/// Quién recibe el texto de la ruta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// La entrada de un agente: la ruta tal cual (entre comillas dobles si tiene espacios).
    Agent,
    /// El shell: comillas según el shell.
    Shell(ShellFlavor),
}

/// Texto que se pega para una o varias rutas; termina en espacio para seguir escribiendo.
pub fn paths_text(paths: &[String], target: Target) -> String {
    let parts: Vec<String> = paths
        .iter()
        .map(|p| match target {
            Target::Agent if p.contains(char::is_whitespace) => format!("\"{p}\""),
            Target::Agent => p.clone(),
            Target::Shell(flavor) => quote_arg(p, flavor),
        })
        .collect();
    format!("{} ", parts.join(" "))
}

/// ¿El archivo es una imagen que se puede entregar (PNG, JPG, GIF, WebP)?
pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// `captura-AAAAMMDD-HHMMSS.png`.
pub fn capture_name(y: i16, mo: i8, d: i8, h: i8, mi: i8, s: i8) -> String {
    format!("{PREFIX}{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}.png")
}

/// Nombre de la captura de ahora (hora local).
pub fn capture_name_now() -> String {
    let now = jiff::Timestamp::now().to_zoned_datetime(jiff::tz::TimeZone::system());
    capture_name(
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    )
}

/// `dir/name` o, si ya existe, `dir/<base>-2.png`, `-3`…
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let stem = name.strip_suffix(".png").unwrap_or(name);
    (2..)
        .map(|n| dir.join(format!("{stem}-{n}.png")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Tamaño reducido (manteniendo la proporción) si algún lado pasa de `max`; `None` si entra.
pub fn fit_within(w: u32, h: u32, max: u32) -> Option<(u32, u32)> {
    if w <= max && h <= max {
        return None;
    }
    let scale = max as f64 / w.max(h) as f64;
    let fit = |v: u32| ((v as f64 * scale).round() as u32).max(1);
    Some((fit(w), fit(h)))
}

/// Imagen lista para guardar: siempre PNG, como mucho [`MAX_SIDE`] px de lado.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Decodifica (PNG, JPG, GIF, WebP o BMP —el portapapeles de Windows entrega BMP—), achica si hace
/// falta y codifica en PNG. Un PNG que ya entra se guarda tal cual.
pub fn prepare_png(bytes: &[u8]) -> Result<Prepared, String> {
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    let img = image::load_from_memory_with_format(bytes, format).map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    match fit_within(w, h, MAX_SIDE) {
        None if format == image::ImageFormat::Png => Ok(Prepared {
            png: bytes.to_vec(),
            width: w,
            height: h,
        }),
        fit => {
            let img = match fit {
                Some((nw, nh)) => img.resize_exact(nw, nh, image::imageops::FilterType::Triangle),
                None => img,
            };
            let mut png = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .map_err(|e| e.to_string())?;
            Ok(Prepared {
                png,
                width: img.width(),
                height: img.height(),
            })
        }
    }
}

/// Carpeta de las capturas de Kuntur (temporal del sistema: Windows, Linux y Mac).
pub fn captures_dir() -> PathBuf {
    std::env::temp_dir().join("kuntur").join("capturas")
}

/// Guarda el PNG con un nombre libre en `dir`.
pub fn save(dir: &Path, png: &[u8], name: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = unique_path(dir, name);
    std::fs::write(&path, png)?;
    Ok(path)
}

/// Nombres de las capturas de Kuntur más viejas que [`MAX_AGE`] (no toca otros archivos).
pub fn stale(entries: &[(String, SystemTime)], now: SystemTime) -> Vec<String> {
    entries
        .iter()
        .filter(|(name, _)| name.starts_with(PREFIX) && name.ends_with(".png"))
        .filter(|(_, modified)| now.duration_since(*modified).is_ok_and(|age| age > MAX_AGE))
        .map(|(name, _)| name.clone())
        .collect()
}

/// Borra las capturas viejas de `dir`; devuelve cuántas.
pub fn cleanup(dir: &Path) -> usize {
    let Ok(read) = std::fs::read_dir(dir) else {
        return 0;
    };
    let entries: Vec<(String, SystemTime)> = read
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((e.file_name().to_string_lossy().into_owned(), modified))
        })
        .collect();
    stale(&entries, SystemTime::now())
        .into_iter()
        .filter(|name| std::fs::remove_file(dir.join(name)).is_ok())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuntur_agents::PasteKey;

    #[test]
    fn claude_local_recibe_su_tecla_y_lo_demas_va_por_ruta() {
        let claude = Some(ImagePaste::Key(PasteKey::Alt('v')));
        assert_eq!(choose(claude, false), Delivery::Key(b"\x1bv".to_vec()));
        assert_eq!(choose(Some(ImagePaste::Path), false), Delivery::LocalPath);
        assert_eq!(choose(None, false), Delivery::LocalPath);
        // Por SSH el agente no ve el portapapeles local, aunque sea Claude Code.
        assert_eq!(choose(claude, true), Delivery::RemotePath);
        assert_eq!(choose(None, true), Delivery::RemotePath);
    }

    #[test]
    fn rutas_con_las_comillas_de_quien_las_recibe() {
        let p = |s: &str| vec![s.to_string()];
        assert_eq!(paths_text(&p(r"C:\tmp\a.png"), Target::Agent), r"C:\tmp\a.png ");
        assert_eq!(
            paths_text(&p(r"C:\mi carpeta\a.png"), Target::Agent),
            "\"C:\\mi carpeta\\a.png\" "
        );
        assert_eq!(
            paths_text(&p(r"C:\mi carpeta\a.png"), Target::Shell(ShellFlavor::PowerShell)),
            r"'C:\mi carpeta\a.png' "
        );
        assert_eq!(
            paths_text(&p("/tmp/kuntur-web/a b.png"), Target::Shell(ShellFlavor::Posix)),
            "'/tmp/kuntur-web/a b.png' "
        );
        assert_eq!(
            paths_text(&[String::from("/a.png"), String::from("/b.png")], Target::Agent),
            "/a.png /b.png "
        );
    }

    #[test]
    fn solo_formatos_de_imagen() {
        assert!(is_image_path(Path::new("x/captura.PNG")));
        assert!(is_image_path(Path::new("foto.jpeg")));
        assert!(is_image_path(Path::new("a.webp")));
        assert!(!is_image_path(Path::new("informe.pdf")));
        assert!(!is_image_path(Path::new("sin-extension")));
    }

    #[test]
    fn nombre_de_la_captura() {
        assert_eq!(capture_name(2026, 11, 24, 9, 5, 7), "captura-20261124-090507.png");
        assert!(capture_name_now().starts_with("captura-"));
    }

    #[test]
    fn achica_manteniendo_la_proporcion() {
        assert_eq!(fit_within(1920, 1080, 2000), None);
        assert_eq!(fit_within(4000, 1000, 2000), Some((2000, 500)));
        assert_eq!(fit_within(1000, 3000, 2000), Some((667, 2000)));
        assert_eq!(fit_within(5000, 1, 2000), Some((2000, 1)));
    }

    fn synthetic(w: u32, h: u32, format: image::ImageFormat) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 120, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), format)
            .unwrap();
        out
    }

    #[test]
    fn png_chico_se_guarda_tal_cual_y_lo_demas_se_convierte() {
        let png = synthetic(40, 30, image::ImageFormat::Png);
        let p = prepare_png(&png).unwrap();
        assert_eq!((p.width, p.height), (40, 30));
        assert_eq!(p.png, png);

        // BMP (lo que entrega el portapapeles de Windows) → PNG.
        let bmp = synthetic(20, 10, image::ImageFormat::Bmp);
        let p = prepare_png(&bmp).unwrap();
        assert_eq!(image::guess_format(&p.png).unwrap(), image::ImageFormat::Png);
        assert_eq!((p.width, p.height), (20, 10));
    }

    #[test]
    fn imagen_grande_se_achica_a_2000() {
        let big = synthetic(3000, 600, image::ImageFormat::Png);
        let p = prepare_png(&big).unwrap();
        assert_eq!((p.width, p.height), (2000, 400));
        let back = image::load_from_memory(&p.png).unwrap();
        assert_eq!((back.width(), back.height()), (2000, 400));
    }

    #[test]
    fn datos_que_no_son_imagen_dan_error() {
        assert!(prepare_png(b"esto no es una imagen").is_err());
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kuntur-test-capturas-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn guarda_sin_pisar_otra_captura() {
        let dir = tmp("guardar");
        let a = save(&dir, b"png", "captura-20261124-090507.png").unwrap();
        let b = save(&dir, b"png", "captura-20261124-090507.png").unwrap();
        assert_ne!(a, b);
        assert!(b.ends_with("captura-20261124-090507-2.png"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn solo_borra_capturas_viejas_de_kuntur() {
        let now = SystemTime::now();
        let old = now - Duration::from_secs(8 * 24 * 3600);
        let new = now - Duration::from_secs(3600);
        let entries = vec![
            ("captura-20261101-100000.png".to_string(), old),
            ("captura-20261124-100000.png".to_string(), new),
            ("otro-archivo.png".to_string(), old),
            ("captura-notas.txt".to_string(), old),
        ];
        assert_eq!(stale(&entries, now), ["captura-20261101-100000.png"]);

        let dir = tmp("limpiar");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("captura-20261124-100000.png"), b"x").unwrap();
        assert_eq!(cleanup(&dir), 0, "una captura reciente no se borra");
        assert!(dir.join("captura-20261124-100000.png").exists());
        assert_eq!(cleanup(&tmp("no-existe")), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
