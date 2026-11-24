//! Carpetas de datos de Kuntur en cada sistema (ajustes v0.2, §10 del plan).
//!
//! - **Configuración** (lo que se guarda y viaja con el usuario: `config.toml`, historial, uso):
//!   Windows `%APPDATA%\Kuntur`, Mac `~/Library/Application Support/Kuntur`, Linux
//!   `$XDG_CONFIG_HOME/kuntur` (o `~/.config/kuntur`).
//! - **Local** (temporales, cachés, descargas del actualizador): Windows `%LOCALAPPDATA%\Kuntur`,
//!   Mac `~/Library/Caches/Kuntur`, Linux `$XDG_CACHE_HOME/kuntur` (o `~/.cache/kuntur`).
//!
//! La lógica recibe el sistema y las variables de entorno: se prueba igual en cualquier PC.

use std::ffi::OsString;
use std::path::PathBuf;

/// Sistema para el que se calculan las carpetas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    /// Linux y otros Unix.
    Unix,
}

impl Os {
    pub const fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Mac
        } else {
            Os::Unix
        }
    }
}

/// Qué carpeta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Config,
    Local,
}

/// Variable no vacía.
fn var(env: &impl Fn(&str) -> Option<OsString>, name: &str) -> Option<PathBuf> {
    env(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Carpeta de Kuntur de ese tipo en ese sistema, con las variables que da `env`.
pub fn resolve(os: Os, kind: Kind, env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    match os {
        Os::Windows => {
            let name = match kind {
                Kind::Config => "APPDATA",
                Kind::Local => "LOCALAPPDATA",
            };
            var(&env, name).map(|d| d.join("Kuntur"))
        }
        Os::Mac => {
            let home = var(&env, "HOME")?;
            let lib = home.join("Library");
            Some(match kind {
                Kind::Config => lib.join("Application Support").join("Kuntur"),
                Kind::Local => lib.join("Caches").join("Kuntur"),
            })
        }
        Os::Unix => {
            let (xdg, fallback) = match kind {
                Kind::Config => ("XDG_CONFIG_HOME", ".config"),
                Kind::Local => ("XDG_CACHE_HOME", ".cache"),
            };
            // XDG pide ignorar rutas relativas.
            let base = var(&env, xdg)
                .filter(|p| p.is_absolute() || p.starts_with("/"))
                .or_else(|| var(&env, "HOME").map(|h| h.join(fallback)))?;
            Some(base.join("kuntur"))
        }
    }
}

fn real_env(name: &str) -> Option<OsString> {
    std::env::var_os(name)
}

/// Carpeta de configuración de Kuntur en este sistema.
pub fn config_dir() -> Option<PathBuf> {
    resolve(Os::current(), Kind::Config, real_env)
}

/// Carpeta local (temporales y cachés) de Kuntur en este sistema; si no se puede saber, la carpeta
/// temporal del sistema + `kuntur`.
pub fn local_dir() -> PathBuf {
    resolve(Os::current(), Kind::Local, real_env).unwrap_or_else(|| std::env::temp_dir().join("kuntur"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| OsString::from(v))
    }

    #[test]
    fn windows_usa_appdata_y_localappdata() {
        let e = [
            ("APPDATA", r"C:\Users\demo\AppData\Roaming"),
            ("LOCALAPPDATA", r"C:\Users\demo\AppData\Local"),
        ];
        assert_eq!(
            resolve(Os::Windows, Kind::Config, env(&e)),
            Some(PathBuf::from(r"C:\Users\demo\AppData\Roaming").join("Kuntur"))
        );
        assert_eq!(
            resolve(Os::Windows, Kind::Local, env(&e)),
            Some(PathBuf::from(r"C:\Users\demo\AppData\Local").join("Kuntur"))
        );
        assert_eq!(resolve(Os::Windows, Kind::Config, env(&[])), None);
    }

    #[test]
    fn mac_usa_library() {
        let e = [("HOME", "/Users/demo")];
        assert_eq!(
            resolve(Os::Mac, Kind::Config, env(&e)),
            Some(
                PathBuf::from("/Users/demo/Library")
                    .join("Application Support")
                    .join("Kuntur")
            )
        );
        assert_eq!(
            resolve(Os::Mac, Kind::Local, env(&e)),
            Some(PathBuf::from("/Users/demo/Library").join("Caches").join("Kuntur"))
        );
    }

    #[test]
    fn linux_respeta_xdg_y_si_no_usa_home() {
        let xdg = [
            ("HOME", "/home/demo"),
            ("XDG_CONFIG_HOME", "/home/demo/conf"),
            ("XDG_CACHE_HOME", "/tmp/c"),
        ];
        assert_eq!(
            resolve(Os::Unix, Kind::Config, env(&xdg)),
            Some(PathBuf::from("/home/demo/conf").join("kuntur"))
        );
        assert_eq!(
            resolve(Os::Unix, Kind::Local, env(&xdg)),
            Some(PathBuf::from("/tmp/c").join("kuntur"))
        );

        let home = [("HOME", "/home/demo"), ("XDG_CONFIG_HOME", "relativa")];
        assert_eq!(
            resolve(Os::Unix, Kind::Config, env(&home)),
            Some(PathBuf::from("/home/demo").join(".config").join("kuntur"))
        );
        assert_eq!(
            resolve(Os::Unix, Kind::Local, env(&home)),
            Some(PathBuf::from("/home/demo").join(".cache").join("kuntur"))
        );
        assert_eq!(resolve(Os::Unix, Kind::Config, env(&[])), None);
    }
}
