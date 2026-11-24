//! SSH de Kuntur (F6): conexión con `russh`, autenticación (ssh-agent de Windows, llaves de
//! `~/.ssh`, contraseña), `known_hosts`, secretos en el Administrador de credenciales de Windows y
//! terminales remotas que alimentan el mismo emulador que las locales (`kuntur_term::Transport`).
//! Sin GPUI: la app solo crea [`RemoteSession`]s y conecta sus terminales a un `Pane`.

pub mod auth;
pub mod client;
pub mod commands;
mod console;
pub mod known_hosts;
pub mod prompt;
pub mod remote;
pub mod secrets;

pub use auth::{AuthStep, SshTarget, auth_plan, display_key};
pub use client::{Connection, SshContext, WINDOWS_AGENT_PIPE, runtime};
pub use console::OutputReader;
pub use known_hosts::{HostKeyStatus, KnownHosts, fingerprint};
pub use remote::{LinkState, RemoteSession, RemoteTerminal};
pub use secrets::{MemorySecrets, SecretStore, system_store};
