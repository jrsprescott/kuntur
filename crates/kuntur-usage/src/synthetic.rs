//! Historial **sintético** para las pruebas de humo y las capturas de la vista de uso
//! (`KUNTUR_SMOKE_USAGE`): nunca se muestran datos reales del usuario en una captura.
//! Determinista: la misma fecha da siempre los mismos registros.

use jiff::ToSpan;
use jiff::tz::TimeZone;

use crate::{AgentKind, UsageRecord};

/// Días hacia atrás que cubre (más de un año, para que "1 año" y "Todo" difieran).
pub const DAYS: i64 = 420;

const TASKS: [&str; 10] = [
    "Refactor del lector de configuración",
    "Pruebas del módulo de pagos",
    "Migrar la API a la versión 2",
    "Corregir el inicio de sesión",
    "Documentar los endpoints",
    "Optimizar consultas lentas",
    "Agregar modo oscuro",
    "Revisar el pull request de facturas",
    "Script de despliegue",
    "Traducir la interfaz",
];
const PROJECTS: [&str; 4] = [
    "C:/proyectos/tienda-web",
    "C:/proyectos/api-pagos",
    "C:/proyectos/app-movil",
    "/srv/demo/backend",
];

/// Generador lineal congruente: suficiente para datos de muestra y sin dependencias.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n.max(1)
    }
}

/// (agente, modelo, ¿usa caché?) según un número 0–99: Claude Code es el más usado.
fn pick_agent(roll: u64) -> (AgentKind, &'static str, bool) {
    match roll {
        0..40 => (AgentKind::ClaudeCode, "claude-sonnet-4-5", true),
        40..52 => (AgentKind::ClaudeCode, "claude-opus-4-1", true),
        52..80 => (AgentKind::Codex, "gpt-5-codex", true),
        80..92 => (AgentKind::OpenCode, "deepseek/deepseek-chat", false),
        _ => (AgentKind::Gemini, "gemini-2.5-pro", false),
    }
}

/// Registros de los últimos `DAYS` días hasta `now` (ninguno posterior), en la zona `tz`.
pub fn records(now: i64, tz: &TimeZone) -> Vec<UsageRecord> {
    let mut rng = Rng(0x6b75_6e74_7572);
    let today = jiff::Timestamp::from_second(now)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .to_zoned_datetime(tz.clone())
        .date();
    let mut out = Vec::new();
    for back in (0..DAYS).rev() {
        let day = today.saturating_sub(back.days());
        let Ok(start) = day.to_zoned_datetime(tz.clone()) else {
            continue;
        };
        let day_start = start.timestamp().as_second();
        let weekend = day.weekday().to_monday_zero_offset() >= 5;
        // Más uso en los días recientes (el uso "crece") y poco el fin de semana.
        let recent = (DAYS - back) as u64 * 3 / DAYS as u64;
        let sessions = if weekend {
            rng.below(2)
        } else {
            1 + rng.below(2 + recent)
        };
        for s in 0..sessions {
            let (agent, model, cache) = pick_agent(rng.below(100));
            let title = TASKS[rng.below(TASKS.len() as u64) as usize];
            let project = PROJECTS[rng.below(PROJECTS.len() as u64) as usize];
            let mut ts = day_start + (8 + rng.below(12)) as i64 * 3600 + rng.below(3600) as i64;
            for _ in 0..(3 + rng.below(20)) {
                if ts > now {
                    break;
                }
                let input = 400 + rng.below(5_000);
                out.push(UsageRecord {
                    agent,
                    model: model.into(),
                    ts,
                    input,
                    output: 150 + rng.below(2_500),
                    cache_read: if cache { input * (4 + rng.below(12)) } else { 0 },
                    cache_write: if cache { rng.below(2_000) } else { 0 },
                    session_id: format!("sintetica-{back}-{s}"),
                    title: Some(title.into()),
                    project: Some(project.into()),
                });
                ts += 20 + rng.below(160) as i64;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Period, summarize};

    fn now() -> i64 {
        "2026-11-23T15:00:00-05:00"
            .parse::<jiff::Timestamp>()
            .unwrap()
            .as_second()
    }

    fn lima() -> TimeZone {
        TimeZone::fixed(jiff::tz::offset(-5))
    }

    #[test]
    fn determinista_y_sin_registros_futuros() {
        let a = records(now(), &lima());
        assert_eq!(a, records(now(), &lima()));
        assert!(a.len() > 1_000);
        assert!(a.iter().all(|r| r.ts <= now()));
    }

    #[test]
    fn llena_todos_los_periodos_y_varios_agentes() {
        let a = records(now(), &lima());
        let year = summarize(&a, Period::Year, now(), &lima());
        let all = summarize(&a, Period::All, now(), &lima());
        assert!(all.totals.turns > year.totals.turns, "Todo abarca más que 1 año");
        assert!(year.agents.len() >= 4);
        assert!(year.models.len() >= 4);
        assert_eq!(year.top_tasks.len(), 10);
        assert!(year.totals.cache_read > 0);
    }
}
