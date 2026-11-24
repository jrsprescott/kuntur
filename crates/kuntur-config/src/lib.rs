//! Configuración de Kuntur: temas (`themes/*.toml`) y ajustes persistentes.

pub mod connections;
pub mod mcp;
pub mod settings;

pub use connections::{ConnectionConfig, ConnectionError, parse_destination, parse_port};
pub mod theme;

pub use mcp::{CommandTarget, McpProfileConfig, McpServerConfig, McpTransport, PermissionMode, ShortcutCommand};

pub use settings::{AgentConfig, SessionConfig, Settings, SidebarMode};
pub use theme::{Rgba8, Theme, ThemeId, builtin};
