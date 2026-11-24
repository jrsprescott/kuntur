//! Alertas (§12): cuándo avisar que un agente de una pestaña no visible quedó esperando o terminó.
//! Lógica pura: recibe cada lectura del monitor y devuelve las alertas nuevas; la UI pone la
//! insignia en la pestaña y muestra la notificación de Windows.

use std::collections::HashMap;

use crate::exact::ExactState;
use crate::running::AgentState;

/// Lecturas seguidas en "esperando" (cada 2 s) antes de avisar: una pausa corta del agente mientras
/// trabaja no debe disparar la alerta.
pub const WAITING_CONFIRM: u8 = 2;

/// Un agente en una terminal concreta.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AgentKey {
    pub session: u64,
    /// En la terminal acoplada de la sesión.
    pub docked: bool,
    pub pid: u32,
}

/// Lo que se vio de un agente en una lectura.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub key: AgentKey,
    /// Id del agente en el catálogo.
    pub agent: String,
    pub state: AgentState,
    /// Estado exacto (hooks / OSC 7501), si se conoce: entonces "esperando" es seguro y se avisa en
    /// la primera lectura, sin esperar la confirmación de la heurística.
    pub exact: Option<ExactState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertKind {
    /// Pasó de trabajando a esperando (posible pedido de confirmación).
    Waiting,
    /// Pide permiso para usar una herramienta (estado exacto).
    Permission,
    /// Su proceso terminó.
    Finished,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub key: AgentKey,
    pub agent: String,
    pub kind: AlertKind,
}

#[derive(Clone, Debug)]
struct Tracked {
    agent: String,
    /// Trabajó desde el último aviso (o desde que se lo vio).
    armed: bool,
    waiting_streak: u8,
}

/// Recuerda el estado de cada agente entre lecturas.
#[derive(Clone, Debug, Default)]
pub struct AlertTracker {
    agents: HashMap<AgentKey, Tracked>,
}

impl AlertTracker {
    /// Procesa una lectura completa. `visible(sesión)`: la pestaña se está viendo; `open(sesión)`:
    /// la sesión sigue abierta (un agente que desaparece porque se cerró su sesión no avisa).
    pub fn update(
        &mut self,
        seen: &[Observation],
        visible: impl Fn(u64) -> bool,
        open: impl Fn(u64) -> bool,
    ) -> Vec<Alert> {
        let mut alerts = Vec::new();
        let mut next: HashMap<AgentKey, Tracked> = HashMap::with_capacity(seen.len());
        for obs in seen {
            let entry = match self.agents.remove(&obs.key) {
                // Primera vez que se lo ve: nunca avisa (por ejemplo, al abrir Kuntur).
                None => Tracked {
                    agent: obs.agent.clone(),
                    armed: obs.state == AgentState::Working,
                    waiting_streak: 0,
                },
                Some(mut t) => {
                    match obs.state {
                        AgentState::Working => {
                            t.armed = true;
                            t.waiting_streak = 0;
                        }
                        AgentState::Waiting => {
                            t.waiting_streak = t.waiting_streak.saturating_add(1);
                            let confirm = if obs.exact.is_some() { 1 } else { WAITING_CONFIRM };
                            if t.armed && t.waiting_streak >= confirm {
                                // Visto o no, el cambio ya pasó: no se avisa después al cambiar de pestaña.
                                t.armed = false;
                                if !visible(obs.key.session) {
                                    alerts.push(Alert {
                                        key: obs.key,
                                        agent: obs.agent.clone(),
                                        kind: if obs.exact == Some(ExactState::Permission) {
                                            AlertKind::Permission
                                        } else {
                                            AlertKind::Waiting
                                        },
                                    });
                                }
                            }
                        }
                    }
                    t
                }
            };
            next.insert(obs.key, entry);
        }
        // Los que ya no están: terminaron.
        let mut gone: Vec<(AgentKey, Tracked)> = self.agents.drain().collect();
        gone.sort_by_key(|(k, _)| (k.session, k.docked, k.pid));
        for (key, t) in gone {
            if open(key.session) && !visible(key.session) {
                alerts.push(Alert {
                    key,
                    agent: t.agent,
                    kind: AlertKind::Finished,
                });
            }
        }
        self.agents = next;
        alerts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use AgentState::{Waiting, Working};

    fn obs(session: u64, pid: u32, state: AgentState) -> Observation {
        Observation {
            key: AgentKey {
                session,
                docked: false,
                pid,
            },
            agent: "claude".into(),
            state,
            exact: None,
        }
    }

    fn kinds(alerts: &[Alert]) -> Vec<(u64, AlertKind)> {
        alerts.iter().map(|a| (a.key.session, a.kind)).collect()
    }

    /// La sesión 1 es la visible; todas siguen abiertas.
    fn step(t: &mut AlertTracker, seen: &[Observation]) -> Vec<(u64, AlertKind)> {
        kinds(&t.update(seen, |s| s == 1, |_| true))
    }

    #[test]
    fn trabajando_a_esperando_en_pestana_oculta_avisa_una_vez() {
        let mut t = AlertTracker::default();
        assert!(step(&mut t, &[obs(2, 10, Working)]).is_empty());
        assert!(step(&mut t, &[obs(2, 10, Working)]).is_empty());
        // Una lectura en espera todavía no basta (pausa corta).
        assert!(step(&mut t, &[obs(2, 10, Waiting)]).is_empty());
        assert_eq!(step(&mut t, &[obs(2, 10, Waiting)]), [(2, AlertKind::Waiting)]);
        // Sigue esperando: no se repite.
        assert!(step(&mut t, &[obs(2, 10, Waiting)]).is_empty());
        assert!(step(&mut t, &[obs(2, 10, Waiting)]).is_empty());
        // Vuelve a trabajar y a esperar: avisa de nuevo.
        assert!(step(&mut t, &[obs(2, 10, Working)]).is_empty());
        assert!(step(&mut t, &[obs(2, 10, Waiting)]).is_empty());
        assert_eq!(step(&mut t, &[obs(2, 10, Waiting)]), [(2, AlertKind::Waiting)]);
    }

    #[test]
    fn pausa_corta_no_avisa() {
        let mut t = AlertTracker::default();
        step(&mut t, &[obs(2, 10, Working)]);
        assert!(step(&mut t, &[obs(2, 10, Waiting)]).is_empty());
        assert!(step(&mut t, &[obs(2, 10, Working)]).is_empty());
        assert!(step(&mut t, &[obs(2, 10, Waiting)]).is_empty());
    }

    #[test]
    fn la_pestana_visible_no_avisa_ni_despues() {
        let mut t = AlertTracker::default();
        step(&mut t, &[obs(1, 10, Working)]);
        step(&mut t, &[obs(1, 10, Waiting)]);
        assert!(step(&mut t, &[obs(1, 10, Waiting)]).is_empty());
        // El usuario se va a otra pestaña: el cambio ya ocurrió mientras la veía.
        let later = t.update(&[obs(1, 10, Waiting)], |_| false, |_| true);
        assert!(later.is_empty());
    }

    #[test]
    fn primera_lectura_en_espera_no_avisa() {
        let mut t = AlertTracker::default();
        for _ in 0..4 {
            assert!(step(&mut t, &[obs(2, 10, Waiting)]).is_empty());
        }
    }

    #[test]
    fn agente_que_termina() {
        let mut t = AlertTracker::default();
        step(&mut t, &[obs(1, 10, Working), obs(2, 20, Working), obs(3, 30, Waiting)]);
        // Terminan los tres: el de la pestaña visible no avisa; el de la 3 sí (aunque esperaba).
        assert_eq!(step(&mut t, &[]), [(2, AlertKind::Finished), (3, AlertKind::Finished)]);
        // Ya no se recuerdan.
        assert!(step(&mut t, &[]).is_empty());
    }

    #[test]
    fn cerrar_la_sesion_no_avisa() {
        let mut t = AlertTracker::default();
        t.update(&[obs(2, 20, Working)], |_| false, |_| true);
        assert!(t.update(&[], |_| false, |s| s != 2).is_empty());
    }

    #[test]
    fn la_acoplada_es_otro_agente() {
        let mut t = AlertTracker::default();
        let mut docked = obs(2, 10, Working);
        docked.key.docked = true;
        let main = obs(2, 10, Working);
        step(&mut t, &[main.clone(), docked.clone()]);
        // Solo termina el de la acoplada.
        let alerts = t.update(&[main], |_| false, |_| true);
        assert_eq!(alerts.len(), 1);
        assert!(alerts[0].key.docked);
        assert_eq!(alerts[0].kind, AlertKind::Finished);
        assert_eq!(alerts[0].agent, "claude");
    }

    #[test]
    fn con_estado_exacto_avisa_en_la_primera_lectura() {
        let exact = |state, e| Observation {
            exact: Some(e),
            ..obs(2, 10, state)
        };
        let mut t = AlertTracker::default();
        step(&mut t, &[exact(Working, ExactState::Working)]);
        assert_eq!(
            step(&mut t, &[exact(Waiting, ExactState::Permission)]),
            [(2, AlertKind::Permission)]
        );
        step(&mut t, &[exact(Working, ExactState::Working)]);
        assert_eq!(
            step(&mut t, &[exact(Waiting, ExactState::Done)]),
            [(2, AlertKind::Waiting)]
        );
        // Sin volver a trabajar, pasar de terminado a esperando respuesta no repite.
        assert!(step(&mut t, &[exact(Waiting, ExactState::Reply)]).is_empty());
    }
}
