//! Estado exacto de los agentes (F5b) a partir de los eventos de los hooks (named pipe) y del
//! OSC 7501: trabajando / esperando permiso / esperando respuesta / terminado, la acción actual,
//! los subagentes con la suya y los comandos que sugirió en su último mensaje. Lógica pura: la UI
//! le pasa los eventos y lee el resultado por terminal (`KUNTUR_SESSION`).

use std::collections::HashMap;

use kuntur_hook::{EventKind, HookEvent};

use crate::running::AgentState;
use crate::suggest::suggested_commands;

/// Subagentes que se recuerdan por agente (los terminados más viejos se olvidan primero).
pub const MAX_SUBAGENTS: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExactState {
    Working,
    /// Pide permiso para usar una herramienta.
    Permission,
    /// Espera una respuesta del usuario (pregunta, `idle_prompt`).
    Reply,
    /// Terminó su turno.
    Done,
}

impl ExactState {
    /// Estado grueso para filtros, pestañas y alertas.
    pub fn coarse(self) -> AgentState {
        match self {
            ExactState::Working => AgentState::Working,
            _ => AgentState::Waiting,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ExactState::Working => "trabajando",
            ExactState::Permission => "pide permiso",
            ExactState::Reply => "espera respuesta",
            ExactState::Done => "terminó",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubAgent {
    pub id: String,
    /// Tipo (`Explore`, `Plan`, `general-purpose`…).
    pub kind: String,
    pub action: Option<String>,
    pub working: bool,
    /// Segundos Unix del último cambio.
    pub since: u64,
    last_ts: u64,
}

/// Lo que se sabe del agente de una terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneAgent {
    /// Id del agente en el catálogo (`claude`…).
    pub agent: String,
    pub state: ExactState,
    /// "Bash · npm test" mientras corre una herramienta.
    pub action: Option<String>,
    /// Qué pide permitir ("Bash · rm -rf dist").
    pub permission: Option<String>,
    pub subagents: Vec<SubAgent>,
    /// Comandos de shell sugeridos en su último mensaje.
    pub suggestions: Vec<String>,
    /// Segundos Unix del último cambio de estado.
    pub since: u64,
    last_ts: u64,
}

impl PaneAgent {
    fn new(agent: &str, now: u64) -> Self {
        Self {
            agent: agent.to_string(),
            state: ExactState::Done,
            action: None,
            permission: None,
            subagents: Vec::new(),
            suggestions: Vec::new(),
            since: now,
            last_ts: 0,
        }
    }

    fn set_state(&mut self, state: ExactState, now: u64) {
        if self.state != state {
            self.state = state;
            self.since = now;
        }
    }

    /// Subagentes trabajando / total.
    pub fn subagent_counts(&self) -> (usize, usize) {
        (
            self.subagents.iter().filter(|s| s.working).count(),
            self.subagents.len(),
        )
    }

    fn subagent(&mut self, id: &str, kind: Option<&str>, now: u64) -> &mut SubAgent {
        let pos = match self.subagents.iter().position(|s| s.id == id) {
            Some(p) => p,
            None => {
                if self.subagents.len() >= MAX_SUBAGENTS
                    && let Some(old) = self
                        .subagents
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| !s.working)
                        .min_by_key(|(_, s)| s.since)
                        .map(|(i, _)| i)
                {
                    self.subagents.remove(old);
                }
                self.subagents.push(SubAgent {
                    id: id.to_string(),
                    kind: kind.unwrap_or("subagente").to_string(),
                    action: None,
                    working: true,
                    since: now,
                    last_ts: 0,
                });
                self.subagents.len() - 1
            }
        };
        let sub = &mut self.subagents[pos];
        if let Some(k) = kind {
            sub.kind = k.to_string();
        }
        sub
    }
}

/// "Bash · npm test" o solo "Bash".
pub fn action_text(tool: Option<&str>, detail: Option<&str>) -> Option<String> {
    match (tool, detail) {
        (Some(t), Some(d)) => Some(format!("{t} · {d}")),
        (Some(t), None) => Some(t.to_string()),
        (None, Some(d)) => Some(d.to_string()),
        (None, None) => None,
    }
}

/// Estado exacto de cada terminal de Kuntur.
#[derive(Clone, Debug, Default)]
pub struct ExactTracker {
    panes: HashMap<String, PaneAgent>,
}

impl ExactTracker {
    pub fn get(&self, pane: &str) -> Option<&PaneAgent> {
        self.panes.get(pane)
    }

    pub fn panes(&self) -> impl Iterator<Item = (&String, &PaneAgent)> {
        self.panes.iter()
    }

    /// Olvida la terminal (su agente se cerró).
    pub fn forget(&mut self, pane: &str) {
        self.panes.remove(pane);
    }

    /// Olvida las terminales que ya no existen.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.panes.retain(|k, _| keep(k));
    }

    /// El usuario respondió desde Kuntur (Permitir / Rechazar): vuelve a trabajar hasta el próximo evento.
    pub fn answered(&mut self, pane: &str, now: u64) {
        if let Some(p) = self.panes.get_mut(pane) {
            p.permission = None;
            p.set_state(ExactState::Working, now);
        }
    }

    /// Aplica un evento de un hook. `now`: segundos Unix. Devuelve `true` si algo cambió.
    pub fn apply(&mut self, ev: &HookEvent, now: u64) -> bool {
        if ev.pane.is_empty() || ev.kind == EventKind::StatusLine {
            return false;
        }
        if ev.kind == EventKind::SessionEnd {
            return self.panes.remove(&ev.pane).is_some();
        }
        let agent = ev.agent.as_deref().unwrap_or("claude");
        let p = self
            .panes
            .entry(ev.pane.clone())
            .or_insert_with(|| PaneAgent::new(agent, now));
        p.agent = agent.to_string();
        let action = action_text(ev.tool.as_deref(), ev.detail.as_deref());

        // Eventos de un subagente: actualizan su fila (y el permiso, que se ve en la terminal principal).
        let sub_event = matches!(
            ev.kind,
            EventKind::PreTool
                | EventKind::PostTool
                | EventKind::Permission
                | EventKind::SubagentStart
                | EventKind::SubagentStop
        );
        if let (Some(id), true) = (ev.agent_id.as_deref(), sub_event) {
            let sub = p.subagent(id, ev.agent_type.as_deref(), now);
            // El pipe puede entregar desordenado: un evento más viejo que el último no cuenta.
            if ev.ts != 0 && ev.ts < sub.last_ts {
                return false;
            }
            sub.last_ts = ev.ts;
            sub.since = now;
            match ev.kind {
                EventKind::SubagentStart => {
                    sub.working = true;
                    sub.action = None;
                }
                EventKind::PreTool => {
                    sub.working = true;
                    sub.action = action.clone();
                }
                EventKind::PostTool => sub.action = None,
                EventKind::SubagentStop => {
                    sub.working = false;
                    sub.action = None;
                }
                _ => {}
            }
            let kind = sub.kind.clone();
            match ev.kind {
                EventKind::Permission => {
                    p.permission = action.map(|a| format!("{kind} · {a}"));
                    p.set_state(ExactState::Permission, now);
                }
                EventKind::PreTool | EventKind::SubagentStart if p.state != ExactState::Working => {
                    p.set_state(ExactState::Working, now);
                }
                _ => {}
            }
            return true;
        }

        if ev.ts != 0 && ev.ts < p.last_ts {
            return false;
        }
        p.last_ts = ev.ts;
        match ev.kind {
            EventKind::SessionStart => {
                p.action = None;
                p.permission = None;
                p.set_state(ExactState::Done, now);
            }
            EventKind::Prompt => {
                p.action = None;
                p.permission = None;
                p.suggestions.clear();
                p.subagents.retain(|s| s.working);
                p.set_state(ExactState::Working, now);
            }
            EventKind::PreTool => {
                p.action = action;
                p.permission = None;
                p.set_state(ExactState::Working, now);
            }
            EventKind::PostTool => {
                p.action = None;
                p.permission = None;
                p.set_state(ExactState::Working, now);
            }
            EventKind::Permission => {
                p.permission = action.or_else(|| p.action.clone());
                p.set_state(ExactState::Permission, now);
            }
            EventKind::Notification => match ev.notification.as_deref() {
                Some("permission_prompt") => {
                    if p.permission.is_none() {
                        p.permission = p.action.clone();
                    }
                    p.set_state(ExactState::Permission, now);
                }
                Some("idle_prompt" | "elicitation_dialog" | "elicitation_url_dialog" | "agent_needs_input") => {
                    p.set_state(ExactState::Reply, now);
                }
                _ => return false,
            },
            EventKind::Stop => {
                p.action = None;
                p.permission = None;
                for s in &mut p.subagents {
                    s.working = false;
                    s.action = None;
                }
                p.suggestions = ev.message.as_deref().map(suggested_commands).unwrap_or_default();
                p.set_state(ExactState::Done, now);
            }
            // Sin `agent_id`: se ignoran (no hay a quién atribuirlos).
            EventKind::SubagentStart | EventKind::SubagentStop => return false,
            EventKind::SessionEnd | EventKind::StatusLine => return false,
        }
        true
    }

    /// Aplica un OSC 7501 de la terminal. `state = None` es "el agente se cerró".
    pub fn apply_signal(
        &mut self,
        pane: &str,
        state: Option<ExactState>,
        agent: Option<&str>,
        action: Option<String>,
        now: u64,
    ) {
        let Some(state) = state else {
            self.panes.remove(pane);
            return;
        };
        let p = self
            .panes
            .entry(pane.to_string())
            .or_insert_with(|| PaneAgent::new(agent.unwrap_or("agente"), now));
        if let Some(a) = agent {
            p.agent = a.to_string();
        }
        match state {
            ExactState::Permission => {
                if action.is_some() {
                    p.permission = action;
                }
            }
            ExactState::Working => {
                p.permission = None;
                p.action = action;
            }
            _ => {
                p.permission = None;
                p.action = None;
            }
        }
        p.set_state(state, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: EventKind, ts: u64) -> HookEvent {
        HookEvent {
            pane: "k1".into(),
            kind,
            ts,
            agent: Some("claude".into()),
            ..HookEvent::default()
        }
    }

    fn tool(kind: EventKind, ts: u64, name: &str, detail: &str) -> HookEvent {
        HookEvent {
            tool: Some(name.into()),
            detail: Some(detail.into()),
            ..ev(kind, ts)
        }
    }

    fn sub(mut e: HookEvent, id: &str, kind: &str) -> HookEvent {
        e.agent_id = Some(id.into());
        e.agent_type = Some(kind.into());
        e
    }

    #[test]
    fn ciclo_de_un_turno() {
        let mut t = ExactTracker::default();
        assert!(t.apply(&ev(EventKind::SessionStart, 1), 100));
        assert_eq!(t.get("k1").unwrap().state, ExactState::Done);
        t.apply(&ev(EventKind::Prompt, 2), 101);
        let p = t.get("k1").unwrap();
        assert_eq!((p.state, p.since), (ExactState::Working, 101));
        t.apply(&tool(EventKind::PreTool, 3, "Bash", "npm test"), 102);
        assert_eq!(t.get("k1").unwrap().action.as_deref(), Some("Bash · npm test"));
        // El estado no cambió: el tiempo sigue contando desde que empezó a trabajar.
        assert_eq!(t.get("k1").unwrap().since, 101);
        t.apply(&tool(EventKind::PostTool, 4, "Bash", "npm test"), 103);
        assert_eq!(t.get("k1").unwrap().action, None);
        let mut stop = ev(EventKind::Stop, 5);
        stop.message = Some("Listo. Para probar:\n```bash\nnpm test\n```".into());
        t.apply(&stop, 104);
        let p = t.get("k1").unwrap();
        assert_eq!(p.state, ExactState::Done);
        assert_eq!(p.state.coarse(), AgentState::Waiting);
        assert_eq!(p.suggestions, ["npm test"]);
        // Un mensaje nuevo limpia las sugerencias.
        t.apply(&ev(EventKind::Prompt, 6), 105);
        assert!(t.get("k1").unwrap().suggestions.is_empty());
        assert!(t.apply(&ev(EventKind::SessionEnd, 7), 106));
        assert!(t.get("k1").is_none());
    }

    #[test]
    fn permisos_y_notificaciones() {
        let mut t = ExactTracker::default();
        t.apply(&ev(EventKind::Prompt, 1), 100);
        t.apply(&tool(EventKind::PreTool, 2, "Bash", "rm -rf dist"), 100);
        t.apply(&tool(EventKind::Permission, 3, "Bash", "rm -rf dist"), 101);
        let p = t.get("k1").unwrap();
        assert_eq!(p.state, ExactState::Permission);
        assert_eq!(p.permission.as_deref(), Some("Bash · rm -rf dist"));
        // Responder desde Kuntur: vuelve a trabajar.
        t.answered("k1", 102);
        assert_eq!(t.get("k1").unwrap().state, ExactState::Working);
        assert_eq!(t.get("k1").unwrap().permission, None);
        // Notification permission_prompt sin PermissionRequest: usa la acción en curso.
        let mut n = ev(EventKind::Notification, 4);
        n.notification = Some("permission_prompt".into());
        t.apply(&n, 103);
        assert_eq!(t.get("k1").unwrap().state, ExactState::Permission);
        assert_eq!(t.get("k1").unwrap().permission.as_deref(), Some("Bash · rm -rf dist"));
        n.notification = Some("idle_prompt".into());
        n.ts = 5;
        t.apply(&n, 104);
        assert_eq!(t.get("k1").unwrap().state, ExactState::Reply);
        // Notificaciones que no son de estado no cambian nada.
        n.notification = Some("auth_success".into());
        n.ts = 6;
        assert!(!t.apply(&n, 105));
        assert_eq!(t.get("k1").unwrap().state, ExactState::Reply);
    }

    #[test]
    fn subagentes_con_su_accion() {
        let mut t = ExactTracker::default();
        t.apply(&ev(EventKind::Prompt, 1), 100);
        t.apply(&sub(ev(EventKind::SubagentStart, 2), "a1", "Explore"), 101);
        t.apply(&sub(ev(EventKind::SubagentStart, 3), "a2", "Plan"), 101);
        t.apply(
            &sub(tool(EventKind::PreTool, 4, "Grep", "\"fn main\""), "a1", "Explore"),
            102,
        );
        t.apply(&sub(tool(EventKind::PreTool, 5, "Read", "main.rs"), "a2", "Plan"), 102);
        let p = t.get("k1").unwrap();
        assert_eq!(p.subagent_counts(), (2, 2));
        assert_eq!(p.subagents[0].action.as_deref(), Some("Grep · \"fn main\""));
        assert_eq!(p.subagents[1].kind, "Plan");
        // La acción del principal no cambia con las de los subagentes.
        assert_eq!(p.action, None);
        t.apply(&sub(ev(EventKind::SubagentStop, 6), "a1", "Explore"), 103);
        assert_eq!(t.get("k1").unwrap().subagent_counts(), (1, 2));
        // Un subagente pide permiso: lo muestra el agente principal.
        t.apply(
            &sub(tool(EventKind::Permission, 7, "Bash", "cargo test"), "a2", "Plan"),
            104,
        );
        let p = t.get("k1").unwrap();
        assert_eq!(p.state, ExactState::Permission);
        assert_eq!(p.permission.as_deref(), Some("Plan · Bash · cargo test"));
        // Stop: todos terminan; el siguiente mensaje olvida los terminados.
        t.apply(&ev(EventKind::Stop, 8), 105);
        assert_eq!(t.get("k1").unwrap().subagent_counts(), (0, 2));
        t.apply(&ev(EventKind::Prompt, 9), 106);
        assert!(t.get("k1").unwrap().subagents.is_empty());
    }

    #[test]
    fn un_evento_viejo_no_pisa_uno_nuevo() {
        let mut t = ExactTracker::default();
        // Llegan al revés: Stop (ts 20) antes que el PreTool (ts 10).
        t.apply(&ev(EventKind::Stop, 20), 100);
        assert!(!t.apply(&tool(EventKind::PreTool, 10, "Bash", "x"), 101));
        assert_eq!(t.get("k1").unwrap().state, ExactState::Done);
        // Igual en un subagente.
        t.apply(&sub(ev(EventKind::SubagentStop, 30), "a", "Explore"), 102);
        assert!(!t.apply(&sub(tool(EventKind::PreTool, 25, "Read", "x"), "a", "Explore"), 103));
        assert!(!t.get("k1").unwrap().subagents[0].working);
    }

    #[test]
    fn limite_de_subagentes_olvida_los_terminados_mas_viejos() {
        let mut t = ExactTracker::default();
        for i in 0..MAX_SUBAGENTS as u64 {
            let id = format!("s{i}");
            t.apply(&sub(ev(EventKind::SubagentStart, i * 2 + 1), &id, "Explore"), 100 + i);
            if i < 3 {
                t.apply(&sub(ev(EventKind::SubagentStop, i * 2 + 2), &id, "Explore"), 100 + i);
            }
        }
        t.apply(&sub(ev(EventKind::SubagentStart, 1000), "nuevo", "Plan"), 500);
        let p = t.get("k1").unwrap();
        assert_eq!(p.subagents.len(), MAX_SUBAGENTS);
        assert!(p.subagents.iter().all(|s| s.id != "s0"));
        assert!(p.subagents.iter().any(|s| s.id == "nuevo"));
    }

    #[test]
    fn senales_osc_7501() {
        let mut t = ExactTracker::default();
        t.apply_signal(
            "k9",
            Some(ExactState::Working),
            Some("codex"),
            Some("compilando".into()),
            10,
        );
        let p = t.get("k9").unwrap();
        assert_eq!((p.agent.as_str(), p.state), ("codex", ExactState::Working));
        assert_eq!(p.action.as_deref(), Some("compilando"));
        t.apply_signal("k9", Some(ExactState::Permission), None, Some("rm x".into()), 11);
        assert_eq!(t.get("k9").unwrap().permission.as_deref(), Some("rm x"));
        assert_eq!(t.get("k9").unwrap().agent, "codex");
        t.apply_signal("k9", None, None, None, 12);
        assert!(t.get("k9").is_none());
        // Eventos sin terminal o de la línea de estado no crean nada.
        let mut e = ev(EventKind::Prompt, 1);
        e.pane.clear();
        assert!(!t.apply(&e, 1));
        assert!(!t.apply(&ev(EventKind::StatusLine, 1), 1));
        assert!(t.get("k1").is_none());
    }
}
