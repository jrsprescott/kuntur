//! Barras del gráfico "Tokens por …" de la vista de uso: por hora (Hoy) o por día (7 y 30 días).

use jiff::civil::Date;

use crate::{Period, Summary};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bucket {
    Hour,
    Day,
}

impl Bucket {
    /// Título del gráfico.
    pub fn title(self) -> &'static str {
        match self {
            Bucket::Hour => "Tokens por hora",
            Bucket::Day => "Tokens por día",
        }
    }
}

/// Una barra apilada (entrada abajo, salida arriba).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bar {
    /// Texto corto del eje (la vista puede mostrar solo algunos).
    pub label: String,
    /// Texto largo del tooltip: "lun 23 nov", "08:00–09:00".
    pub title: String,
    pub input: u64,
    pub output: u64,
}

const MONTHS: [&str; 12] = [
    "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
];
const WEEKDAYS: [&str; 7] = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"];

fn month(d: Date) -> &'static str {
    MONTHS[(d.month() - 1) as usize]
}

/// Agrupación que corresponde al período.
pub fn bucket_for(period: Period) -> Bucket {
    match period {
        Period::Today => Bucket::Hour,
        Period::Days7 | Period::Days30 => Bucket::Day,
    }
}

/// Barras del gráfico, en orden cronológico.
pub fn bars(s: &Summary, period: Period) -> (Bucket, Vec<Bar>) {
    let bucket = bucket_for(period);
    let bars = match bucket {
        Bucket::Hour => s
            .by_hour
            .iter()
            .enumerate()
            .map(|(h, &(input, output))| Bar {
                label: format!("{h:02}"),
                title: format!("{h:02}:00–{:02}:00", (h + 1) % 24),
                input,
                output,
            })
            .collect(),
        Bucket::Day => s
            .by_day
            .iter()
            .map(|&(d, input, output)| Bar {
                label: d.day().to_string(),
                title: format!(
                    "{} {} {}",
                    WEEKDAYS[d.weekday().to_monday_zero_offset() as usize],
                    d.day(),
                    month(d)
                ),
                input,
                output,
            })
            .collect(),
    };
    (bucket, bars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentKind, Cost, UsageRecord, summarize};
    use jiff::tz::TimeZone;

    fn ts(s: &str) -> i64 {
        s.parse::<jiff::Timestamp>().unwrap().as_second()
    }

    fn lima() -> TimeZone {
        TimeZone::fixed(jiff::tz::offset(-5))
    }

    /// Lunes 23 de noviembre de 2026, mediodía en UTC-5.
    fn now() -> i64 {
        ts("2026-11-23T12:00:00-05:00")
    }

    fn rec(at: &str, input: u64, output: u64) -> UsageRecord {
        UsageRecord {
            agent: AgentKind::ClaudeCode,
            model: "claude-x".into(),
            ts: ts(at),
            input,
            output,
            cache_read: 0,
            cache_write: 0,
            session_id: "a".into(),
            title: None,
            project: None,
            cost: Cost::Unknown,
            key: None,
        }
    }

    #[test]
    fn agrupacion_segun_el_periodo() {
        assert_eq!(bucket_for(Period::Today), Bucket::Hour);
        assert_eq!(bucket_for(Period::Days7), Bucket::Day);
        assert_eq!(bucket_for(Period::Days30), Bucket::Day);
    }

    #[test]
    fn hoy_va_por_hora() {
        let records = vec![
            rec("2026-11-23T08:15:00-05:00", 10, 1),
            rec("2026-11-23T08:45:00-05:00", 5, 1),
        ];
        let (bucket, bars) = bars(&summarize(&records, Period::Today, now(), &lima()), Period::Today);
        assert_eq!(bucket, Bucket::Hour);
        assert_eq!(bars.len(), 24);
        assert_eq!((bars[8].input, bars[8].output), (15, 2));
        assert_eq!(bars[8].title, "08:00–09:00");
        assert_eq!(bars[23].title, "23:00–00:00");
    }

    #[test]
    fn siete_dias_van_por_dia_con_nombre_del_dia() {
        let records = vec![rec("2026-11-23T08:00:00-05:00", 7, 3)];
        let (_, bars) = bars(&summarize(&records, Period::Days7, now(), &lima()), Period::Days7);
        assert_eq!(bars.len(), 7);
        assert_eq!(bars[6].label, "23");
        assert_eq!(bars[6].title, "lun 23 nov");
        assert_eq!(bars[0].title, "mar 17 nov");
        assert_eq!((bars[6].input, bars[6].output), (7, 3));
    }
}
