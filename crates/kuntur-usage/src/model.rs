//! Modelo común a todos los lectores de historial.

/// Agentes de código que Kuntur conoce.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AgentKind {
    ClaudeCode,
    Codex,
    OpenCode,
    Qwen,
    Gemini,
    Other,
}

impl AgentKind {
    /// Nombre para mostrar.
    pub fn name(self) -> &'static str {
        match self {
            AgentKind::ClaudeCode => "Claude Code",
            AgentKind::Codex => "Codex",
            AgentKind::OpenCode => "OpenCode",
            AgentKind::Qwen => "Qwen Code",
            AgentKind::Gemini => "Gemini CLI",
            AgentKind::Other => "Otro",
        }
    }
}

/// Un turno (o sesión, si la fuente solo da totales) con sus tokens.
///
/// `input` son los tokens de entrada **sin** caché; la caché va aparte en `cache_read` / `cache_write`
/// (los lectores normalizan las fuentes que la incluyen dentro de la entrada, como Codex).
#[derive(Clone, Debug, PartialEq)]
pub struct UsageRecord {
    pub agent: AgentKind,
    pub model: String,
    /// Segundos Unix (UTC).
    pub ts: i64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub session_id: String,
    pub title: Option<String>,
    /// Carpeta del proyecto (solo el último segmento se muestra en la UI).
    pub project: Option<String>,
}

impl UsageRecord {
    /// Tokens de entrada + salida (sin caché): la medida de "total" de las vistas.
    pub fn total(&self) -> u64 {
        self.input + self.output
    }
}

/// Período de la vista de uso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Days7,
    Days30,
    Days90,
    All,
}

impl Period {
    /// Cantidad de días del período (`None` en `All`).
    pub fn days(self) -> Option<i64> {
        match self {
            Period::Days7 => Some(7),
            Period::Days30 => Some(30),
            Period::Days90 => Some(90),
            Period::All => None,
        }
    }
}
