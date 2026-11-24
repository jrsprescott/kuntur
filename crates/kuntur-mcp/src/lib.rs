//! MCP por sesión (F10, §11 de la especificación). Sin UI:
//!
//! - `catalog`: validar servidores, perfiles y la lista de cada sesión; resolver secretos.
//! - `secrets`: valores secretos en el Administrador de credenciales de Windows.
//! - `generate`: configuración MCP de cada CLI (Claude Code, Codex, OpenCode) y opciones del agente
//!   (modelo, esfuerzo, permisos).
//! - `launch`: archivos temporales de la sesión + la línea que se escribe en la terminal.
//! - `probe`: "Probar" (lanzar / conectar y hacer `initialize` con tiempo límite).
//! - `session_dir`: `%LOCALAPPDATA%\Kuntur\mcp\<pid>-<sesión>\` y su limpieza.
//! - `outputs`: archivos nuevos en la carpeta `salida\` de la sesión (→ vista previa).
//! - `shortcuts`: comandos con atajo.

pub mod catalog;
pub mod generate;
pub mod launch;
pub mod outputs;
pub mod probe;
pub mod secrets;
pub mod session_dir;
pub mod shortcuts;
pub mod toml_text;

pub use catalog::{
    McpError, ResolvedKind, ResolvedServer, apply_profile, resolve, secret_account, toggle, validate_server,
};
pub use generate::{AgentOptions, Cli};
pub use launch::{GeneratedFile, LaunchPlan, plan_launch};
pub use outputs::OutputWatcher;
pub use probe::{ProbeStatus, probe};
pub use secrets::{MemorySecrets, SecretStore, system_store};
pub use session_dir::{OUTPUT_DIR, SessionDirs};
pub use shortcuts::{Shortcut, ShortcutError};
