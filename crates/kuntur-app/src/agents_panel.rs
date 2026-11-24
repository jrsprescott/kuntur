//! Lógica del panel de agentes (sin GPUI): filas, filtro Todos / Esperando / Trabajando, búsqueda,
//! tiempos y textos del anillo de límites. El `Workspace` la dibuja.

use kuntur_agents::{AgentState, ExactState, PaneAgent};
use kuntur_usage::codex::Limits;

use crate::command_palette::matches;

/// Un agente corriendo en una terminal (principal o acoplada) de una sesión, listo para mostrar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRow {
    pub session_id: u64,
    pub session_name: String,
    /// Corre en la terminal acoplada de la sesión.
    pub docked: bool,
    /// Id del agente en el catálogo.
    pub agent: String,
    /// Nombre para mostrar ("Agente" si ya no está en el catálogo).
    pub name: String,
    pub pid: u32,
    pub state: AgentState,
    /// Segundos Unix en que arrancó.
    pub started_at: u64,
    /// Estado exacto (hooks / OSC 7501), acción, subagentes y sugerencias, si llegan eventos. Sin
    /// eventos, `state` sale de la heurística por actividad.
    pub exact: Option<PaneAgent>,
}

impl AgentRow {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// "web" o "web · acoplada".
    pub fn place(&self) -> String {
        place(&self.session_name, self.docked)
    }

    /// Clave estable de la fila: el PID del agente, o (sin proceso conocido, solo por eventos) la
    /// terminal.
    pub fn key(&self) -> u64 {
        if self.pid != 0 {
            self.pid as u64
        } else {
            (1 << 40) | (self.session_id << 1) | self.docked as u64
        }
    }

    pub fn exact_state(&self) -> Option<ExactState> {
        self.exact.as_ref().map(|e| e.state)
    }

    /// Texto del estado: el exacto si se conoce ("pide permiso", "espera respuesta", "terminó"…).
    pub fn state_label(&self) -> &'static str {
        match (self.exact_state(), self.state) {
            (Some(e), _) => e.label(),
            (None, AgentState::Working) => "trabajando",
            (None, AgentState::Waiting) => "esperando",
        }
    }

    /// Lo que se ve en la segunda línea después de la sesión: la acción actual mientras trabaja
    /// ("Bash · npm test") o el estado.
    pub fn detail(&self) -> String {
        match &self.exact {
            Some(e) if e.state == ExactState::Working => {
                e.action.clone().unwrap_or_else(|| e.state.label().to_string())
            }
            _ => self.state_label().to_string(),
        }
    }

    /// ¿El estado pide atención del usuario (permiso o respuesta)? Se muestra en ámbar.
    pub fn needs_user(&self) -> bool {
        match self.exact_state() {
            Some(e) => matches!(e, ExactState::Permission | ExactState::Reply),
            None => self.state == AgentState::Waiting,
        }
    }
}

/// Dónde corre un agente: el nombre de la sesión, y "· acoplada" si es la terminal acoplada.
pub fn place(session_name: &str, docked: bool) -> String {
    if docked {
        format!("{session_name} · acoplada")
    } else {
        session_name.to_string()
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

/// Filas que pasan el filtro y la búsqueda (por nombre del agente o de la sesión, o "acoplada"; sin
/// mayúsculas ni tildes; cada palabra debe aparecer).
pub fn filter_rows<'a>(rows: &'a [AgentRow], filter: AgentFilter, query: &str) -> Vec<&'a AgentRow> {
    rows.iter()
        .filter(|r| filter.accepts(r.state))
        .filter(|r| matches(query, &format!("{} {}", r.name(), r.place())))
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

    fn row(session: &str, agent: &str, state: AgentState) -> AgentRow {
        AgentRow {
            session_id: 1,
            session_name: session.into(),
            docked: false,
            agent: agent.into(),
            name: kuntur_agents::Catalog::builtin().get(agent).unwrap().name.clone(),
            pid: 10,
            state,
            started_at: 0,
            exact: None,
        }
    }

    fn rows() -> Vec<AgentRow> {
        vec![
            row("web", "claude", AgentState::Working),
            row("api", "codex", AgentState::Waiting),
            row("Documentación", "opencode", AgentState::Waiting),
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
        assert_eq!(working[0].agent, "claude");
        assert_eq!(counts(&r), [3, 2, 1]);
    }

    #[test]
    fn busca_por_agente_o_sesion_sin_tildes() {
        let r = rows();
        assert_eq!(filter_rows(&r, AgentFilter::All, "codex")[0].session_name, "api");
        assert_eq!(filter_rows(&r, AgentFilter::All, "CLAUDE web").len(), 1);
        assert_eq!(filter_rows(&r, AgentFilter::All, "documentacion")[0].agent, "opencode");
        assert!(filter_rows(&r, AgentFilter::All, "claude api").is_empty());
        // La búsqueda se combina con el filtro.
        assert!(filter_rows(&r, AgentFilter::Working, "codex").is_empty());
    }

    #[test]
    fn agentes_de_la_acoplada() {
        let mut r = rows();
        r[1].docked = true;
        assert_eq!(r[1].place(), "api · acoplada");
        assert_eq!(r[0].place(), "web");
        let found = filter_rows(&r, AgentFilter::All, "acoplada");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].agent, "codex");
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

    #[test]
    fn textos_con_estado_exacto() {
        use kuntur_agents::ExactTracker;
        use kuntur_hook::{EventKind, HookEvent};
        let mut r = row("web", "claude", AgentState::Working);
        assert_eq!((r.state_label(), r.detail().as_str()), ("trabajando", "trabajando"));
        let mut t = ExactTracker::default();
        let ev = |kind, tool: Option<&str>, detail: Option<&str>| HookEvent {
            pane: "k1".into(),
            kind,
            tool: tool.map(String::from),
            detail: detail.map(String::from),
            ..HookEvent::default()
        };
        t.apply(&ev(EventKind::PreTool, Some("Bash"), Some("npm test")), 1);
        r.exact = t.get("k1").cloned();
        assert_eq!(r.detail(), "Bash · npm test");
        assert!(!r.needs_user());
        t.apply(&ev(EventKind::Permission, Some("Bash"), Some("rm -rf x")), 2);
        r.exact = t.get("k1").cloned();
        assert_eq!(r.detail(), "pide permiso");
        assert!(r.needs_user());
        t.apply(&ev(EventKind::Stop, None, None), 3);
        r.exact = t.get("k1").cloned();
        assert_eq!(r.state_label(), "terminó");
        assert!(!r.needs_user());
        // Clave: el PID, o la terminal si el agente solo se conoce por eventos.
        assert_eq!(r.key(), 10);
        r.pid = 0;
        r.docked = true;
        assert_ne!(r.key(), row("web", "claude", AgentState::Working).key());
    }
}
