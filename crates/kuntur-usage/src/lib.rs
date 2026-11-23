//! Uso de los agentes de código: lectores del historial local (Claude Code, Codex, OpenCode) y
//! agregaciones por período para la vista "Uso de agentes". Sin UI: la app lo llama en un hilo aparte.

pub mod aggregate;
pub mod model;

pub use aggregate::{ModelRow, Summary, TaskRow, Totals, pct_change, provider_of, summarize};
pub use model::{AgentKind, Period, UsageRecord};
