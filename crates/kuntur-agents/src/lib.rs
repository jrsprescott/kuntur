//! Agentes de código: catálogo (los conocidos y los de la configuración), detección de los
//! instalados (PATH) y de los que están corriendo dentro de cada sesión (árbol de procesos), la
//! línea para abrirlos y la lógica de las alertas. Sin UI.

pub mod alerts;
pub mod catalog;
pub mod launch;
pub mod running;

pub use alerts::{AgentKey, Alert, AlertKind, AlertTracker, Observation};
pub use catalog::{AgentDef, Catalog, ExtraAgent, brand_color, parse_color};
pub use kuntur_usage::AgentKind;
pub use launch::{ShellFlavor, launch_line};
pub use running::{AgentState, ProcInfo, RunningAgent, agents_under, find_agents, state_for};
