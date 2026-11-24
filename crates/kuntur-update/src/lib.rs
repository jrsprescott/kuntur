//! Aviso de actualización de Kuntur (ajustes v0.2, §7 del plan).
//!
//! - [`release`]: lógica pura. Lee la lista de versiones de GitHub, elige la más nueva que la
//!   instalada, su instalador `.msi` y su SHA-256.
//! - [`net`]: la única conexión de red propia de Kuntur. Consulta la API pública de GitHub (el
//!   repositorio es público: sin credenciales) y descarga el instalador mientras calcula su
//!   SHA-256. User-Agent `Kuntur/<versión>`, sin datos del usuario.
//! - [`signature`]: firma Authenticode del instalador descargado (Windows).
//!
//! Nada de esto toca la interfaz: la app lo llama fuera del hilo de la UI.

pub mod net;
pub mod release;
pub mod signature;

pub use release::{Asset, Release, Update, choose_update, parse_releases};
pub use semver::Version;

/// Repositorio de las versiones publicadas.
pub const REPO: &str = "jrsprescott/kuntur";

/// Lista de versiones (las más nuevas primero; incluye las preliminares y los borradores no se
/// muestran sin credenciales).
pub fn releases_url() -> String {
    format!("https://api.github.com/repos/{REPO}/releases?per_page=20")
}

/// Versión de esta compilación.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// User-Agent de las peticiones: solo el nombre y la versión (GitHub exige uno).
pub fn user_agent() -> String {
    format!("Kuntur/{CURRENT_VERSION}")
}
