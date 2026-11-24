//! Configuración de Kuntur: temas (`themes/*.toml`) y ajustes persistentes.

pub mod settings;
pub mod theme;

pub use settings::{AgentConfig, SessionConfig, Settings, SidebarMode};
pub use theme::{Rgba8, Theme, ThemeId, builtin};
