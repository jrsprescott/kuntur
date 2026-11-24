//! Lógica del panel de agentes (sin GPUI): filas, filtro Todos / Esperando / Trabajando, búsqueda,
//! tiempos y textos del anillo de límites. El `Workspace` la dibuja.

use kuntur_agents::{AgentKind, AgentState};
use kuntur_usage::codex::Limits;

use crate::command_palette::matches;

/// Un agente corriendo en una sesión, listo para mostrar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRow {
    pub session_id: u64,
    pub session_name: String,
    pub kind: AgentKind,
    pub pid: u32,
    pub state: AgentState,
    /// Segundos Unix en que arrancó.
    pub started_at: u64,
}

impl AgentRow {
    pub fn name(&self) -> &'static str {
        kuntur_agents::info(self.kind).map_or("Agente", |i| i.name)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentFilter {
    #[default]
    All,
    Waiting,
    Working,
}

impl AgentFilter {
    pub const ALL: [AgentFilter; 3] = [AgentFilter::All, AgentFilter::Waiting, AgentFilter::Working];

    pub fn label(self) -> &'static str {
        match self {
            AgentFilter::All => "Todos",
            AgentFilter::Waiting => "Esperando",
            AgentFilter::Working => "Trabajando",
        }
    }

    pub fn accepts(self, state: AgentState) -> bool {
        match self {
            AgentFilter::All => true,
            AgentFilter::Waiting => state == AgentState::Waiting,
            AgentFilter::Working => state == AgentState::Working,
        }
    }
}

/// Filas que pasan el filtro y la búsqueda (por nombre del agente o de la sesión; sin mayúsculas
/// ni tildes; cada palabra debe aparecer).
pub fn filter_rows<'a>(rows: &'a [AgentRow], filter: AgentFilter, query: &str) -> Vec<&'a AgentRow> {
    rows.iter()
        .filter(|r| filter.accepts(r.state))
        .filter(|r| matches(query, &format!("{} {}", r.name(), r.session_name)))
        .collect()
}

/// Cuántas filas hay en cada filtro (sin búsqueda): `[todos, esperando, trabajando]`.
pub fn counts(rows: &[AgentRow]) -> [usize; 3] {
    AgentFilter::ALL.map(|f| rows.iter().filter(|r| f.accepts(r.state)).count())
}

/// Tiempo corriendo, corto: "ahora", "45 s", "12 min", "1 h 05", "3 d".
pub fn format_elapsed(secs: u64) -> String {
    match secs {
        0..5 => "ahora".into(),
        5..60 => format!("{secs} s"),
        60..3600 => format!("{} min", secs / 60),
        3600..86_400 => format!("{} h {:02}", secs / 3600, secs % 3600 / 60),
        _ => format!("{} d", secs / 86_400),
    }
}

/// Nombre de una ventana de límite: 300 min → "5 h", 10 080 → "semanal", 1 440 → "diario".
pub fn window_label(mins: u64) -> String {
    match mins {
        10_080 => "semanal".into(),
        1_440 => "diario".into(),
        m if m % 1_440 == 0 => format!("{} d", m / 1_440),
        m if m % 60 == 0 => format!("{} h", m / 60),
        m => format!("{m} min"),
    }
}

/// Porcentaje para el anillo (0–100) si hay dato real: el de la ventana corta (primaria).
pub fn ring_pct(limits: Option<&Limits>) -> Option<f32> {
    limits
        .and_then(|l| l.primary_pct)
        .filter(|p| p.is_finite())
        .map(|p| p.clamp(0.0, 100.0) as f32)
}

/// Nivel del anillo según el porcentaje usado: decide su color (acento / aviso / error).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingLevel {
    Normal,
    /// Desde 75 %.
    High,
    /// Desde 90 %.
    Critical,
}

impl RingLevel {
    pub fn from_pct(pct: f32) -> Self {
        if pct >= 90.0 {
            RingLevel::Critical
        } else if pct >= 75.0 {
            RingLevel::High
        } else {
            RingLevel::Normal
        }
    }
}

/// Texto del tooltip de un agente instalado: "Codex · 5 h: 12 % · semanal: 44 %" o, sin datos,
/// "Codex · sin datos de límite".
pub fn ring_tooltip(name: &str, limits: Option<&Limits>) -> String {
    let mut parts = vec![name.to_string()];
    if let Some(l) = limits {
        let mut push = |pct: Option<f64>, mins: Option<u64>, fallback: &str| {
            if let Some(p) = pct.filter(|p| p.is_finite()) {
                let label = mins.map_or_else(|| fallback.to_string(), window_label);
                parts.push(format!("{label}: {} %", p.round() as i64));
            }
        };
        push(l.primary_pct, l.primary_window_mins, "5 h");
        push(l.secondary_pct, l.secondary_window_mins, "semanal");
    }
    if parts.len() == 1 {
        parts.push("sin datos de límite".into());
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(session: &str, kind: AgentKind, state: AgentState) -> AgentRow {
        AgentRow {
            session_id: 1,
            session_name: session.into(),
            kind,
            pid: 10,
            state,
            started_at: 0,
        }
    }

    fn rows() -> Vec<AgentRow> {
        vec![
            row("web", AgentKind::ClaudeCode, AgentState::Working),
            row("api", AgentKind::Codex, AgentState::Waiting),
            row("Documentación", AgentKind::OpenCode, AgentState::Waiting),
        ]
    }

    #[test]
    fn filtra_por_estado() {
        let r = rows();
        assert_eq!(filter_rows(&r, AgentFilter::All, "").len(), 3);
        let waiting = filter_rows(&r, AgentFilter::Waiting, "");
        assert_eq!(waiting.len(), 2);
        assert!(waiting.iter().all(|r| r.state == AgentState::Waiting));
        let working = filter_rows(&r, AgentFilter::Working, "");
        assert_eq!(working.len(), 1);
        assert_eq!(working[0].kind, AgentKind::ClaudeCode);
        assert_eq!(counts(&r), [3, 2, 1]);
    }

    #[test]
    fn busca_por_agente_o_sesion_sin_tildes() {
        let r = rows();
        assert_eq!(filter_rows(&r, AgentFilter::All, "codex")[0].session_name, "api");
        assert_eq!(filter_rows(&r, AgentFilter::All, "CLAUDE web").len(), 1);
        assert_eq!(
            filter_rows(&r, AgentFilter::All, "documentacion")[0].kind,
            AgentKind::OpenCode
        );
        assert!(filter_rows(&r, AgentFilter::All, "claude api").is_empty());
        // La búsqueda se combina con el filtro.
        assert!(filter_rows(&r, AgentFilter::Working, "codex").is_empty());
    }

    #[test]
    fn tiempos_cortos() {
        assert_eq!(format_elapsed(0), "ahora");
        assert_eq!(format_elapsed(45), "45 s");
        assert_eq!(format_elapsed(60), "1 min");
        assert_eq!(format_elapsed(59 * 60 + 59), "59 min");
        assert_eq!(format_elapsed(3600 + 5 * 60), "1 h 05");
        assert_eq!(format_elapsed(3 * 86_400 + 10), "3 d");
    }

    fn limits(primary: Option<f64>, secondary: Option<f64>) -> Limits {
        Limits {
            primary_pct: primary,
            primary_window_mins: Some(300),
            primary_resets_at: None,
            secondary_pct: secondary,
            secondary_window_mins: Some(10_080),
            secondary_resets_at: None,
            ts: 0,
        }
    }

    #[test]
    fn anillo_solo_con_dato_real() {
        assert_eq!(ring_pct(None), None);
        assert_eq!(ring_pct(Some(&limits(None, Some(40.0)))), None);
        assert_eq!(ring_pct(Some(&limits(Some(12.4), None))), Some(12.4));
        assert_eq!(ring_pct(Some(&limits(Some(130.0), None))), Some(100.0));
        assert_eq!(ring_pct(Some(&limits(Some(f64::NAN), None))), None);
    }

    #[test]
    fn color_del_anillo_por_umbral() {
        assert_eq!(RingLevel::from_pct(0.0), RingLevel::Normal);
        assert_eq!(RingLevel::from_pct(74.9), RingLevel::Normal);
        assert_eq!(RingLevel::from_pct(75.0), RingLevel::High);
        assert_eq!(RingLevel::from_pct(89.9), RingLevel::High);
        assert_eq!(RingLevel::from_pct(90.0), RingLevel::Critical);
        assert_eq!(RingLevel::from_pct(100.0), RingLevel::Critical);
    }

    #[test]
    fn tooltip_del_anillo() {
        assert_eq!(
            ring_tooltip("Codex", Some(&limits(Some(12.4), Some(44.0)))),
            "Codex · 5 h: 12 % · semanal: 44 %"
        );
        assert_eq!(
            ring_tooltip("Codex", Some(&limits(None, None))),
            "Codex · sin datos de límite"
        );
        assert_eq!(ring_tooltip("Claude Code", None), "Claude Code · sin datos de límite");
        assert_eq!(window_label(300), "5 h");
        assert_eq!(window_label(1_440), "diario");
        assert_eq!(window_label(10_080), "semanal");
        assert_eq!(window_label(2_880), "2 d");
        assert_eq!(window_label(45), "45 min");
    }
}
