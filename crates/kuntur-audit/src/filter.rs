//! Búsqueda y filtros de la vista "Auditoría": texto (sin mayúsculas ni tildes; cada palabra debe
//! aparecer en el comando, la carpeta, la sesión o el agente), agente y período.

use jiff::ToSpan;
use jiff::tz::TimeZone;
use kuntur_usage::{AgentKind, Period};

use crate::model::AuditRecord;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditFilter {
    pub query: String,
    /// `None` = todos los agentes.
    pub agent: Option<AgentKind>,
    pub period: Period,
}

impl Default for AuditFilter {
    fn default() -> Self {
        Self {
            query: String::new(),
            agent: None,
            period: Period::Days30,
        }
    }
}

/// Minúsculas y sin tildes (la puntuación se conserva: en un comando importa).
fn normalize(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// Comienzo (segundos Unix) del período que termina hoy en la zona `tz`, con días locales completos
/// como "Uso de agentes" ("7 días" = hoy y los 6 anteriores). `None` en "Todo".
pub fn period_start(period: Period, now: i64, tz: &TimeZone) -> Option<i64> {
    let days = period.days()?;
    let today = jiff::Timestamp::from_second(now)
        .ok()?
        .to_zoned_datetime(tz.clone())
        .date();
    let first = today.saturating_sub((days - 1).days());
    first
        .to_zoned_datetime(tz.clone())
        .ok()
        .map(|z| z.timestamp().as_second())
}

/// Índices de `records` que pasan el filtro, en el mismo orden.
pub fn filter(records: &[AuditRecord], f: &AuditFilter, now: i64, tz: &TimeZone) -> Vec<usize> {
    let start = period_start(f.period, now, tz);
    let words: Vec<String> = normalize(&f.query).split_whitespace().map(str::to_string).collect();
    records
        .iter()
        .enumerate()
        .filter(|(_, r)| start.is_none_or(|s| r.ts >= s))
        .filter(|(_, r)| f.agent.is_none_or(|a| r.agent == a))
        .filter(|(_, r)| {
            if words.is_empty() {
                return true;
            }
            let hay = normalize(&format!(
                "{} {} {} {}",
                r.command,
                r.folder.as_deref().unwrap_or(""),
                r.session,
                r.agent.name()
            ));
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LOCAL_HOST;

    fn ts(s: &str) -> i64 {
        s.parse::<jiff::Timestamp>().unwrap().as_second()
    }

    fn rec(at: &str, agent: AgentKind, command: &str, folder: &str) -> AuditRecord {
        AuditRecord {
            ts: ts(at),
            agent,
            session: "ses-1".into(),
            host: LOCAL_HOST.into(),
            folder: Some(folder.into()),
            command: command.into(),
            tool: "Bash".into(),
            key: None,
        }
    }

    fn sample() -> Vec<AuditRecord> {
        vec![
            rec(
                "2026-11-23T15:00:00Z",
                AgentKind::ClaudeCode,
                "cargo test --workspace",
                "C:/proyectos/api",
            ),
            rec(
                "2026-11-22T15:00:00Z",
                AgentKind::Codex,
                "git status",
                "C:/proyectos/Producción",
            ),
            rec(
                "2026-11-10T15:00:00Z",
                AgentKind::ClaudeCode,
                "rm -rf target",
                "C:/proyectos/api",
            ),
            rec("2025-01-10T15:00:00Z", AgentKind::Codex, "npm install", "/srv/demo"),
        ]
    }

    fn run(f: &AuditFilter) -> Vec<usize> {
        let tz = TimeZone::fixed(jiff::tz::offset(-5));
        filter(&sample(), f, ts("2026-11-23T20:00:00Z"), &tz)
    }

    #[test]
    fn por_periodo() {
        let mut f = AuditFilter {
            period: Period::Today,
            ..AuditFilter::default()
        };
        assert_eq!(run(&f), [0]);
        f.period = Period::Days7;
        assert_eq!(run(&f), [0, 1]);
        f.period = Period::Days30;
        assert_eq!(run(&f), [0, 1, 2]);
        f.period = Period::All;
        assert_eq!(run(&f), [0, 1, 2, 3]);
    }

    #[test]
    fn por_agente_y_texto() {
        let mut f = AuditFilter {
            period: Period::All,
            agent: Some(AgentKind::Codex),
            ..AuditFilter::default()
        };
        assert_eq!(run(&f), [1, 3]);
        f.agent = None;
        f.query = "RM -rf".into();
        assert_eq!(run(&f), [2]);
        // Sin tildes y en la carpeta.
        f.query = "produccion".into();
        assert_eq!(run(&f), [1]);
        // Cada palabra debe aparecer; también el nombre del agente.
        f.query = "claude api".into();
        assert_eq!(run(&f), [0, 2]);
        f.query = "codex api".into();
        assert!(run(&f).is_empty());
    }

    #[test]
    fn inicio_del_periodo_en_hora_local() {
        let tz = TimeZone::fixed(jiff::tz::offset(-5));
        let now = ts("2026-11-23T20:00:00Z");
        assert_eq!(
            period_start(Period::Today, now, &tz),
            Some(ts("2026-11-23T00:00:00-05:00"))
        );
        assert_eq!(
            period_start(Period::Days7, now, &tz),
            Some(ts("2026-11-17T00:00:00-05:00"))
        );
        assert_eq!(period_start(Period::All, now, &tz), None);
    }
}
