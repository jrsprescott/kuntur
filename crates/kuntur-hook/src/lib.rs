//! Puente entre los agentes y Kuntur: el evento compacto que mandan los hooks y la línea de estado
//! de Claude Code, el named pipe por el que viaja y el instalador de los ganchos. El binario
//! `kuntur-hook.exe` (en `main.rs`) es lo que invoca Claude Code.

pub mod event;
pub mod install;
pub mod pipe;

pub use event::{EventKind, HookEvent, StatusLimits, decode};
#[cfg(windows)]
pub use pipe::PipeServer;
pub use pipe::{pipe_name, send};

/// Variable con el nombre del pipe de la instancia de Kuntur que abrió la terminal.
pub const ENV_PIPE: &str = "KUNTUR_PIPE";
/// Variable con el identificador de la terminal (principal o acoplada) dentro de Kuntur.
pub const ENV_SESSION: &str = "KUNTUR_SESSION";
