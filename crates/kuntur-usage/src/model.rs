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
    /// Costo del registro: real si la fuente lo trae (OpenCode), estimado con la tabla de precios,
    /// o desconocido.
    pub cost: Cost,
    /// Identidad de la respuesta para no contarla dos veces cuando aparece en varios archivos
    /// (Claude Code copia el historial al reanudar una sesión): `message.id|requestId`.
    pub key: Option<String>,
}

/// Costo en micro-dólares (USD × 10⁻⁶): enteros para sumar sin errores de redondeo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cost {
    #[default]
    Unknown,
    /// Calculado con la tabla de precios (precio de API, no lo que cobra una suscripción).
    Estimated(u64),
    /// Informado por la propia fuente.
    Real(u64),
}

impl Cost {
    /// Micro-dólares, si se conoce.
    pub fn micros(self) -> Option<u64> {
        match self {
            Cost::Unknown => None,
            Cost::Estimated(m) | Cost::Real(m) => Some(m),
        }
    }
}

impl UsageRecord {
    /// Tokens de entrada + salida (sin caché): la medida de "total" de las vistas.
    pub fn total(&self) -> u64 {
        self.input + self.output
    }
}

/// Días que Kuntur carga del historial (Uso y Auditoría): los últimos 30 días locales, hoy incluido.
/// Lo más viejo no se lee ni se guarda en memoria (Claude Code tampoco guarda más por defecto).
pub const WINDOW_DAYS: i64 = 30;

/// Período de la vista de uso: hoy / 7 días / 30 días. Todos son días locales completos que
/// terminan hoy ("Hoy" = solo hoy; "7 días" = hoy y los 6 anteriores). Ninguno pasa de
/// [`WINDOW_DAYS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Today,
    Days7,
    Days30,
}

impl Period {
    /// En el orden del selector de la vista.
    pub const ALL: [Period; 3] = [Period::Today, Period::Days7, Period::Days30];

    /// Cantidad de días del período.
    pub fn days(self) -> i64 {
        match self {
            Period::Today => 1,
            Period::Days7 => 7,
            Period::Days30 => WINDOW_DAYS,
        }
    }

    /// Texto del selector.
    pub fn label(self) -> &'static str {
        match self {
            Period::Today => "Hoy",
            Period::Days7 => "7 días",
            Period::Days30 => "30 días",
        }
    }
}
