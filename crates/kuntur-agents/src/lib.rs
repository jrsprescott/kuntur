//! Agentes de código: catálogo de los conocidos, detección de los instalados (PATH) y de los que
//! están corriendo dentro de cada sesión (árbol de procesos). Sin UI.

pub mod catalog;
pub mod running;

pub use catalog::{AgentInfo, CATALOG, info, installed, installed_in, match_process};
pub use kuntur_usage::AgentKind;
pub use running::{AgentState, ProcInfo, RunningAgent, agents_under, find_agents, state_for};
