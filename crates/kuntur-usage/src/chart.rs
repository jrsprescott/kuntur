//! Barras del gráfico "Tokens por …" de la vista de uso: por hora (Hoy), por día (7 y 30 días, y
//! Todo si es corto), por semana (1 año) o por mes (Todo con más de `MONTHLY_AFTER_DAYS` días).

use jiff::ToSpan;
use jiff::civil::Date;

use crate::{Period, Summary};

/// En "Todo", desde cuántos días se agrupa por mes.
pub const MONTHLY_AFTER_DAYS: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bucket {
    Hour,
    Day,
    Week,
    Month,
}

impl Bucket {
    /// Título del gráfico.
    pub fn title(self) -> &'static str {
        match self {
            Bucket::Hour => "Tokens por hora",
            Bucket::Day => "Tokens por día",
            Bucket::Week => "Tokens por semana",
            Bucket::Month => "Tokens por mes",
        }
    }
}

/// Una barra apilada (entrada abajo, salida arriba).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bar {
    /// Texto corto del eje (la vista puede mostrar solo algunos).
    pub label: String,
    /// Texto largo del tooltip: "lun 23 nov", "Semana del 17 nov", "nov 2026", "08:00–09:00".
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

/// Agrupación que corresponde a un período de `days` días.
pub fn bucket_for(period: Period, days: usize) -> Bucket {
    match period {
        Period::Today => Bucket::Hour,
        Period::Year => Bucket::Week,
        Period::All if days > MONTHLY_AFTER_DAYS => Bucket::Month,
        _ => Bucket::Day,
    }
}

/// Barras del gráfico, en orden cronológico.
pub fn bars(s: &Summary, period: Period) -> (Bucket, Vec<Bar>) {
    let bucket = bucket_for(period, s.by_day.len());
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
        Bucket::Week => group(&s.by_day, week_start, |w| {
            (
                format!("{} {}", w.day(), month(w)),
                format!("Semana del {} {}", w.day(), month(w)),
            )
        }),
        Bucket::Month => group(&s.by_day, Date::first_of_month, |m| {
            (month(m).to_string(), format!("{} {}", month(m), m.year()))
        }),
    };
    (bucket, bars)
}

/// Lunes de la semana de `d`.
fn week_start(d: Date) -> Date {
    d.saturating_sub(i64::from(d.weekday().to_monday_zero_offset()).days())
}

/// Suma días consecutivos con la misma clave (semana o mes) en una barra.
fn group(days: &[(Date, u64, u64)], key: impl Fn(Date) -> Date, names: impl Fn(Date) -> (String, String)) -> Vec<Bar> {
    let mut out: Vec<(Date, Bar)> = Vec::new();
    for &(d, input, output) in days {
        let k = key(d);
        match out.last_mut() {
            Some((last, bar)) if *last == k => {
                bar.input += input;
                bar.output += output;
            }
            _ => {
                let (label, title) = names(k);
                out.push((
                    k,
                    Bar {
                        label,
                        title,
                        input,
                        output,
                    },
                ));
            }
        }
    }
    out.into_iter().map(|(_, b)| b).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentKind, UsageRecord, summarize};
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
        }
    }

    #[test]
    fn agrupacion_segun_el_periodo() {
        assert_eq!(bucket_for(Period::Today, 1), Bucket::Hour);
        assert_eq!(bucket_for(Period::Days7, 7), Bucket::Day);
        assert_eq!(bucket_for(Period::Days30, 30), Bucket::Day);
        assert_eq!(bucket_for(Period::Year, 365), Bucket::Week);
        assert_eq!(bucket_for(Period::All, 120), Bucket::Day);
        assert_eq!(bucket_for(Period::All, 121), Bucket::Month);
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

    #[test]
    fn un_ano_va_por_semanas_de_lunes_a_domingo() {
        let records = vec![
            // Primer día del año móvil (lunes 24 nov 2025) y el domingo de esa semana.
            rec("2025-11-24T10:00:00-05:00", 1, 0),
            rec("2025-11-30T10:00:00-05:00", 2, 0),
            // Lunes siguiente: otra barra.
            rec("2025-12-01T10:00:00-05:00", 4, 0),
            rec("2026-11-23T10:00:00-05:00", 8, 0),
        ];
        let (bucket, bars) = bars(&summarize(&records, Period::Year, now(), &lima()), Period::Year);
        assert_eq!(bucket, Bucket::Week);
        // 52 semanas completas + el lunes de hoy.
        assert_eq!(bars.len(), 53);
        assert_eq!(bars[0].title, "Semana del 24 nov");
        assert_eq!(bars[0].input, 3);
        assert_eq!(bars[1].input, 4);
        assert_eq!(bars[52].input, 8);
        let total: u64 = bars.iter().map(|b| b.input).sum();
        assert_eq!(total, 15);
    }

    #[test]
    fn todo_largo_va_por_mes() {
        let records = vec![
            rec("2026-01-15T10:00:00-05:00", 1, 1),
            rec("2026-01-31T10:00:00-05:00", 1, 1),
            rec("2026-11-02T10:00:00-05:00", 5, 0),
        ];
        let (bucket, bars) = bars(&summarize(&records, Period::All, now(), &lima()), Period::All);
        assert_eq!(bucket, Bucket::Month);
        // Enero a noviembre de 2026.
        assert_eq!(bars.len(), 11);
        assert_eq!(bars[0].label, "ene");
        assert_eq!(bars[0].title, "ene 2026");
        assert_eq!((bars[0].input, bars[0].output), (2, 2));
        assert_eq!(bars[10].input, 5);
    }
}
