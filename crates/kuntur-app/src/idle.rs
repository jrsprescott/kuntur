//! Reposo (integración F10-F11): el monitor de agentes y RAM solo pide redibujar la ventana si
//! cambió algo que se ve, y con la ventana sin foco lee más espaciado. Lógica pura, con pruebas.
//!
//! Antes, cada lectura (cada 2 s) llamaba a `cx.notify()` aunque nada hubiera cambiado: GPUI
//! volvía a dibujar toda la ventana y la app gastaba 1,4–2,6 % de un núcleo quieta.

use std::time::Duration;

use kuntur_usage::codex::Limits;

use crate::agents_panel::AgentRow;

/// Lectura de procesos y RAM con la ventana enfocada.
pub const TICK_FOCUSED: Duration = Duration::from_secs(2);
/// Con la ventana sin foco: las alertas siguen llegando, solo más espaciadas.
pub const TICK_UNFOCUSED: Duration = Duration::from_secs(5);
/// Lectura lenta (agentes instalados, límites de Codex), con o sin foco.
pub const SLOW_EVERY: Duration = Duration::from_secs(30);

/// Espera hasta la próxima lectura según el foco de la ventana.
pub fn monitor_interval(window_active: bool) -> Duration {
    if window_active { TICK_FOCUSED } else { TICK_UNFOCUSED }
}

/// ¿Toca la lectura lenta? `since_last` = tiempo desde la anterior (`None` = nunca se hizo).
pub fn slow_due(since_last: Option<Duration>) -> bool {
    since_last.is_none_or(|d| d >= SLOW_EVERY)
}

/// MB que muestra la línea de estado ("Kuntur · N MB").
pub fn ram_mb(bytes: Option<u64>) -> Option<u64> {
    bytes.map(|b| (b as f64 / (1024.0 * 1024.0)).round() as u64)
}

/// Lo que se ve de las lecturas del monitor y de lo que cambia sin avisar (carpeta, estado de las
/// conexiones remotas). Si dos lecturas dan el mismo `Visible`, no hace falta redibujar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Visible {
    /// Filas del panel de agentes con el tiempo que se muestra ("45 s", "3 min"…).
    pub rows: Vec<(AgentRow, String)>,
    pub ram_mb: Option<u64>,
    pub installed: Vec<String>,
    pub codex_limits: Option<Limits>,
    /// Insignias de alerta (sesión, cantidad), ordenadas.
    pub badges: Vec<(u64, u32)>,
    /// Textos sueltos que se dibujan y no avisan al cambiar: carpeta de la terminal con el foco,
    /// estado y agentes de cada pestaña remota.
    pub status: Vec<String>,
}

impl Visible {
    /// Ordena lo que viene de un `HashMap` para comparar sin depender del orden.
    pub fn with_badges(mut self, badges: impl IntoIterator<Item = (u64, u32)>) -> Self {
        self.badges = badges.into_iter().filter(|&(_, n)| n > 0).collect();
        self.badges.sort_unstable();
        self
    }
}

/// Guarda lo último que se dibujó y dice si la lectura nueva cambia algo visible.
#[derive(Default)]
pub struct VisibleTracker {
    last: Option<Visible>,
}

impl VisibleTracker {
    /// `true` (y lo recuerda) si `now` es distinto de lo último que se dibujó.
    pub fn changed(&mut self, now: Visible) -> bool {
        if self.last.as_ref() == Some(&now) {
            return false;
        }
        self.last = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{TICK_FOCUSED, TICK_UNFOCUSED, Visible, VisibleTracker, monitor_interval, ram_mb, slow_due};
    use crate::agents_panel::AgentRow;
    use kuntur_agents::AgentState;
    use std::time::Duration;

    fn row(state: AgentState) -> AgentRow {
        AgentRow {
            session_id: 1,
            session_name: "web".into(),
            docked: false,
            agent: "claude".into(),
            name: "Claude Code".into(),
            pid: 10,
            state,
            started_at: 0,
            exact: None,
        }
    }

    fn base() -> Visible {
        Visible {
            rows: vec![(row(AgentState::Waiting), "3 min".into())],
            ram_mb: ram_mb(Some(64 * 1024 * 1024)),
            installed: vec!["claude".into(), "codex".into()],
            codex_limits: None,
            badges: Vec::new(),
            status: vec!["~\\proyectos".into()],
        }
    }

    #[test]
    fn intervalo_segun_el_foco() {
        assert_eq!(monitor_interval(true), TICK_FOCUSED);
        assert_eq!(monitor_interval(false), TICK_UNFOCUSED);
        assert!(TICK_UNFOCUSED > TICK_FOCUSED);
    }

    #[test]
    fn lectura_lenta_cada_30_s_sin_importar_el_intervalo() {
        assert!(slow_due(None));
        assert!(!slow_due(Some(Duration::from_secs(29))));
        assert!(slow_due(Some(Duration::from_secs(30))));
        assert!(slow_due(Some(Duration::from_secs(31))));
    }

    #[test]
    fn ram_redondeada_al_mb_que_se_muestra() {
        let mb = 1024 * 1024;
        assert_eq!(ram_mb(None), None);
        assert_eq!(ram_mb(Some(64 * mb)), Some(64));
        assert_eq!(ram_mb(Some(64 * mb + 100 * 1024)), Some(64));
        assert_eq!(ram_mb(Some(64 * mb + 600 * 1024)), Some(65));
    }

    #[test]
    fn sin_cambios_visibles_no_se_redibuja() {
        let mut t = VisibleTracker::default();
        assert!(t.changed(base()), "la primera lectura siempre se dibuja");
        assert!(!t.changed(base()));
        // Unos KB más de RAM no cambian el número que se muestra.
        let mut v = base();
        v.ram_mb = ram_mb(Some(64 * 1024 * 1024 + 4096));
        assert!(!t.changed(v));
    }

    #[test]
    fn cada_cambio_visible_redibuja() {
        let cambios: [fn(&mut Visible); 7] = [
            |v| v.ram_mb = Some(65),
            |v| v.rows[0].1 = "4 min".into(),
            |v| v.rows[0].0.state = AgentState::Working,
            |v| v.rows.clear(),
            |v| {
                v.installed.pop();
            },
            |v| v.badges = vec![(2, 1)],
            |v| v.status = vec![r"C:\Windows".into()],
        ];
        for (i, cambio) in cambios.iter().enumerate() {
            let mut t = VisibleTracker::default();
            t.changed(base());
            let mut v = base();
            cambio(&mut v);
            assert!(t.changed(v.clone()), "cambio {i} no redibujó");
            assert!(!t.changed(v), "cambio {i} redibujó dos veces");
        }
    }

    #[test]
    fn insignias_sin_depender_del_orden_y_sin_ceros() {
        let a = Visible::default().with_badges([(3, 1), (1, 2), (5, 0)]);
        let b = Visible::default().with_badges([(1, 2), (3, 1)]);
        assert_eq!(a, b);
        assert_eq!(a.badges, vec![(1, 2), (3, 1)]);
    }
}
