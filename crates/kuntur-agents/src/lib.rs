//! Agentes de código: catálogo (los conocidos y los de la configuración), detección de los
//! instalados (PATH) y de los que están corriendo dentro de cada sesión (árbol de procesos), la
//! línea para abrirlos, la lógica de las alertas y (F5b) el estado exacto por hooks / OSC 7501, los
//! subagentes, los permisos, los comandos sugeridos y los worktrees por agente. Sin UI.

pub mod alerts;
pub mod catalog;
pub mod exact;
pub mod launch;
pub mod local;
pub mod permission;
pub mod running;
pub mod suggest;
pub mod worktree;

pub use alerts::{AgentKey, Alert, AlertKind, AlertTracker, Observation};
pub use catalog::{AgentDef, Catalog, ExtraAgent, ImagePaste, PasteKey, brand_color, parse_color};
pub use exact::{ExactState, ExactTracker, PaneAgent, SubAgent};
pub use kuntur_usage::AgentKind;
pub use launch::{ShellFlavor, launch_line, quote_arg};
pub use local::{LocalModel, LocalProvider, ProviderStatus};
pub use permission::{Answer, answer_keys, can_answer};
pub use running::{AgentState, ProcInfo, RunningAgent, agents_under, find_agents, state_for};
pub use suggest::{is_risky, one_line, suggested_commands};
