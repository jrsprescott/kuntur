//! Agregaciones de la vista "Uso de agentes".

use std::collections::HashMap;

use jiff::civil::Date;
use jiff::tz::TimeZone;

use crate::model::{AgentKind, Period, UsageRecord};

/// Totales de un período.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// Registros (turnos).
    pub turns: u64,
    /// Sesiones distintas (por agente + id).
    pub sessions: u64,
    pub agent_time_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRow {
    pub model: String,
    pub provider: &'static str,
    pub input: u64,
    pub output: u64,
    pub turns: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskRow {
    pub title: String,
    pub project: Option<String>,
    pub model: String,
    pub agent: AgentKind,
    pub total: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    /// Inicio del período (segundos Unix); en `All`, el primer registro (o `now` si no hay).
    pub start: i64,
    /// Fin exclusivo del período: comienzo de mañana en la zona local.
    pub end: i64,
    pub totals: Totals,
    /// Mismo largo inmediatamente antes (para el % de cambio). `None` en `All`.
    pub previous: Option<Totals>,
    /// Un elemento por día local del período, con los días vacíos en cero: (día, entrada, salida).
    pub by_day: Vec<(Date, u64, u64)>,
    /// Turnos por día de la semana (lunes = 0) y hora local.
    pub heatmap: [[u64; 24]; 7],
    /// Ordenado por entrada + salida, de mayor a menor.
    pub models: Vec<ModelRow>,
    /// Entrada + salida por agente, de mayor a menor.
    pub agents: Vec<(AgentKind, u64)>,
    /// Las 10 sesiones con más tokens.
    pub top_tasks: Vec<TaskRow>,
}

/// Pausa máxima entre dos turnos de una sesión que todavía cuenta como tiempo de agente.
const MAX_GAP_SECS: i64 = 5 * 60;
/// Cantidad de filas de "Tareas que más tokens usaron".
const TOP_TASKS: usize = 10;

/// Proveedor según el prefijo del modelo. Si el nombre no lo dice, mira un prefijo `proveedor/`
/// (como los de OpenCode: `anthropic/claude-…`).
pub fn provider_of(model: &str) -> &'static str {
    let lower = model.to_ascii_lowercase();
    let (vendor, name) = match lower.rsplit_once('/') {
        Some((vendor, name)) => (vendor, name),
        None => ("", lower.as_str()),
    };
    match provider_by_name(name) {
        "Otros" => match vendor.rsplit('/').next().unwrap_or("") {
            "anthropic" => "Anthropic",
            "openai" => "OpenAI",
            "google" => "Google",
            other => provider_by_name(other),
        },
        found => found,
    }
}

fn provider_by_name(name: &str) -> &'static str {
    let mut chars = name.chars();
    let o_digit = chars.next() == Some('o') && chars.next().is_some_and(|c| c.is_ascii_digit());
    if name.starts_with("claude") {
        "Anthropic"
    } else if name.starts_with("gpt") || name.starts_with("codex") || o_digit {
        "OpenAI"
    } else if name.starts_with("gemini") {
        "Google"
    } else if name.starts_with("deepseek") {
        "DeepSeek"
    } else if name.starts_with("qwen") {
        "Qwen"
    } else {
        "Otros"
    }
}

/// Porcentaje de cambio de `current` respecto de `previous` (`None` si no hay base).
pub fn pct_change(current: u64, previous: u64) -> Option<f64> {
    (previous > 0).then(|| (current as f64 - previous as f64) / previous as f64 * 100.0)
}

fn zoned(ts: i64, tz: &TimeZone) -> jiff::ZonedDateTime {
    jiff::Timestamp::from_second(ts)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .to_zoned_datetime(tz.clone())
}

/// Comienzo (segundos Unix) del día local `date`.
fn day_start(date: Date, tz: &TimeZone) -> i64 {
    date.to_zoned_datetime(tz.clone())
        .map(|z| z.timestamp().as_second())
        .unwrap_or(0)
}

fn next_day(date: Date) -> Date {
    date.tomorrow().unwrap_or(date)
}

/// Resume `records` en el período que termina hoy (zona `tz`). Los períodos son días locales
/// completos: "7 días" = hoy y los 6 días anteriores.
pub fn summarize(records: &[UsageRecord], period: Period, now: i64, tz: &TimeZone) -> Summary {
    use jiff::ToSpan;

    let today = zoned(now, tz).date();
    let end = day_start(next_day(today), tz);
    let (first_day, previous_range) = match period.days() {
        Some(n) => {
            let first = today.saturating_sub((n - 1).days());
            let prev_first = first.saturating_sub(n.days());
            (first, Some((day_start(prev_first, tz), day_start(first, tz))))
        }
        None => {
            let oldest = records.iter().map(|r| r.ts).filter(|&t| t < end).min().unwrap_or(now);
            (zoned(oldest, tz).date(), None)
        }
    };
    let start = day_start(first_day, tz);

    let current: Vec<&UsageRecord> = records.iter().filter(|r| r.ts >= start && r.ts < end).collect();
    let previous = previous_range.map(|(from, to)| {
        let prev: Vec<&UsageRecord> = records.iter().filter(|r| r.ts >= from && r.ts < to).collect();
        totals(&prev)
    });

    // Por día y mapa de calor (zona local).
    let mut per_day: HashMap<Date, (u64, u64)> = HashMap::new();
    let mut heatmap = [[0u64; 24]; 7];
    for r in &current {
        let z = zoned(r.ts, tz);
        let day = per_day.entry(z.date()).or_default();
        day.0 += r.input;
        day.1 += r.output;
        heatmap[z.weekday().to_monday_zero_offset() as usize][z.hour() as usize] += 1;
    }
    let mut by_day = Vec::new();
    let mut day = first_day;
    while day <= today {
        let (input, output) = per_day.get(&day).copied().unwrap_or_default();
        by_day.push((day, input, output));
        if day == next_day(day) {
            break;
        }
        day = next_day(day);
    }

    Summary {
        start,
        end,
        totals: totals(&current),
        previous,
        by_day,
        heatmap,
        models: models(&current),
        agents: agents(&current),
        top_tasks: top_tasks(&current),
    }
}

fn totals(records: &[&UsageRecord]) -> Totals {
    let mut t = Totals::default();
    let mut sessions: HashMap<(AgentKind, &str), Vec<i64>> = HashMap::new();
    for r in records {
        t.input += r.input;
        t.output += r.output;
        t.cache_read += r.cache_read;
        t.cache_write += r.cache_write;
        t.turns += 1;
        sessions.entry((r.agent, r.session_id.as_str())).or_default().push(r.ts);
    }
    t.sessions = sessions.len() as u64;
    t.agent_time_secs = sessions
        .into_values()
        .map(|mut times| {
            times.sort_unstable();
            times
                .windows(2)
                .map(|w| w[1] - w[0])
                .filter(|&gap| gap <= MAX_GAP_SECS)
                .sum::<i64>() as u64
        })
        .sum();
    t
}

fn models(records: &[&UsageRecord]) -> Vec<ModelRow> {
    let mut map: HashMap<&str, ModelRow> = HashMap::new();
    for r in records {
        let row = map.entry(r.model.as_str()).or_insert_with(|| ModelRow {
            model: r.model.clone(),
            provider: provider_of(&r.model),
            input: 0,
            output: 0,
            turns: 0,
        });
        row.input += r.input;
        row.output += r.output;
        row.turns += 1;
    }
    let mut rows: Vec<ModelRow> = map.into_values().collect();
    rows.sort_by(|a, b| {
        (b.input + b.output)
            .cmp(&(a.input + a.output))
            .then_with(|| a.model.cmp(&b.model))
    });
    rows
}

fn agents(records: &[&UsageRecord]) -> Vec<(AgentKind, u64)> {
    let mut map: HashMap<AgentKind, u64> = HashMap::new();
    for r in records {
        *map.entry(r.agent).or_default() += r.total();
    }
    let mut rows: Vec<_> = map.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    rows
}

fn top_tasks(records: &[&UsageRecord]) -> Vec<TaskRow> {
    #[derive(Default)]
    struct Acc<'a> {
        title: Option<&'a str>,
        project: Option<&'a str>,
        models: HashMap<&'a str, u64>,
        total: u64,
    }
    let mut map: HashMap<(AgentKind, &str), Acc> = HashMap::new();
    for r in records {
        let acc = map.entry((r.agent, r.session_id.as_str())).or_default();
        acc.title = acc.title.or(r.title.as_deref());
        acc.project = acc.project.or(r.project.as_deref());
        *acc.models.entry(r.model.as_str()).or_default() += r.total();
        acc.total += r.total();
    }
    let mut rows: Vec<TaskRow> = map
        .into_iter()
        .map(|((agent, _), acc)| {
            let model = acc
                .models
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                .map(|(m, _)| m.to_string())
                .unwrap_or_default();
            TaskRow {
                title: acc.title.unwrap_or("Sin título").to_string(),
                project: acc.project.map(str::to_string),
                model,
                agent,
                total: acc.total,
            }
        })
        .collect();
    rows.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.title.cmp(&b.title)));
    rows.truncate(TOP_TASKS);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn rec(session: &str, model: &str, at: &str, input: u64, output: u64) -> UsageRecord {
        UsageRecord {
            agent: AgentKind::ClaudeCode,
            model: model.into(),
            ts: ts(at),
            input,
            output,
            cache_read: 0,
            cache_write: 0,
            session_id: session.into(),
            title: None,
            project: None,
        }
    }

    fn date(s: &str) -> Date {
        s.parse().unwrap()
    }

    #[test]
    fn el_total_del_periodo_excluye_lo_de_fuera() {
        let records = vec![
            rec("a", "claude-x", "2026-11-23T10:00:00-05:00", 100, 10),
            rec("a", "claude-x", "2026-11-17T00:30:00-05:00", 20, 2),
            // Fuera: un minuto antes del primer día local del período de 7 días.
            rec("b", "claude-x", "2026-11-16T23:59:00-05:00", 1000, 1000),
            rec("c", "claude-x", "2026-11-01T09:00:00-05:00", 5000, 5000),
        ];
        let s = summarize(&records, Period::Days7, now(), &lima());
        assert_eq!(s.totals.input, 120);
        assert_eq!(s.totals.output, 12);
        assert_eq!(s.totals.turns, 2);
        assert_eq!(s.totals.sessions, 1);
        assert_eq!(s.start, ts("2026-11-17T00:00:00-05:00"));
        assert_eq!(s.end, ts("2026-11-24T00:00:00-05:00"));

        let all = summarize(&records, Period::All, now(), &lima());
        assert_eq!(all.totals.turns, 4);
        assert_eq!(all.totals.sessions, 3);
    }

    #[test]
    fn rellena_los_dias_vacios() {
        let records = vec![
            rec("a", "claude-x", "2026-11-20T08:00:00-05:00", 5, 1),
            rec("a", "claude-x", "2026-11-20T09:00:00-05:00", 5, 1),
            rec("a", "claude-x", "2026-11-23T08:00:00-05:00", 7, 3),
        ];
        let s = summarize(&records, Period::Days7, now(), &lima());
        assert_eq!(s.by_day.len(), 7);
        assert_eq!(s.by_day[0], (date("2026-11-17"), 0, 0));
        assert_eq!(s.by_day[1], (date("2026-11-18"), 0, 0));
        assert_eq!(s.by_day[3], (date("2026-11-20"), 10, 2));
        assert_eq!(s.by_day[6], (date("2026-11-23"), 7, 3));

        let s30 = summarize(&records, Period::Days30, now(), &lima());
        assert_eq!(s30.by_day.len(), 30);

        // En "Todo" va desde el día del primer registro hasta hoy.
        let all = summarize(&records, Period::All, now(), &lima());
        assert_eq!(all.by_day.len(), 4);
        assert_eq!(all.by_day[0].0, date("2026-11-20"));

        let vacio = summarize(&[], Period::All, now(), &lima());
        assert_eq!(vacio.by_day, vec![(date("2026-11-23"), 0, 0)]);
        assert_eq!(vacio.totals, Totals::default());
    }

    #[test]
    fn el_mapa_de_calor_usa_la_hora_local() {
        // 03:30 UTC del lunes = domingo 22:30 en UTC-5.
        let records = vec![rec("a", "claude-x", "2026-11-23T03:30:00Z", 1, 1)];
        let s = summarize(&records, Period::Days7, now(), &lima());
        assert_eq!(s.heatmap[6][22], 1);
        assert_eq!(s.heatmap[0][3], 0);
        assert_eq!(s.heatmap.iter().flatten().sum::<u64>(), 1);
    }

    #[test]
    fn proveedor_por_prefijo_del_modelo() {
        assert_eq!(provider_of("claude-opus-4-7"), "Anthropic");
        assert_eq!(provider_of("anthropic/claude-sonnet-4"), "Anthropic");
        assert_eq!(provider_of("gpt-5.1-codex"), "OpenAI");
        assert_eq!(provider_of("o3"), "OpenAI");
        assert_eq!(provider_of("o4-mini"), "OpenAI");
        assert_eq!(provider_of("codex-mini-latest"), "OpenAI");
        assert_eq!(provider_of("gemini-2.5-pro"), "Google");
        assert_eq!(provider_of("deepseek-chat"), "DeepSeek");
        assert_eq!(provider_of("Qwen3-Coder"), "Qwen");
        assert_eq!(provider_of("ollama-llama3"), "Otros");
        assert_eq!(provider_of(""), "Otros");
    }

    #[test]
    fn modelos_y_agentes_ordenados_por_total() {
        let mut records = vec![
            rec("a", "claude-x", "2026-11-23T08:00:00-05:00", 10, 10),
            rec("a", "claude-x", "2026-11-23T08:01:00-05:00", 10, 10),
            rec("b", "gpt-5", "2026-11-23T08:00:00-05:00", 100, 0),
        ];
        records[2].agent = AgentKind::Codex;
        let s = summarize(&records, Period::Days7, now(), &lima());
        assert_eq!(
            s.models,
            vec![
                ModelRow {
                    model: "gpt-5".into(),
                    provider: "OpenAI",
                    input: 100,
                    output: 0,
                    turns: 1
                },
                ModelRow {
                    model: "claude-x".into(),
                    provider: "Anthropic",
                    input: 20,
                    output: 20,
                    turns: 2
                },
            ]
        );
        assert_eq!(s.agents, vec![(AgentKind::Codex, 100), (AgentKind::ClaudeCode, 40)]);
    }

    #[test]
    fn top_tareas_agrupa_por_sesion_y_ordena() {
        let mut records = vec![
            rec("a", "claude-x", "2026-11-23T08:00:00-05:00", 100, 0),
            rec("a", "claude-x", "2026-11-23T08:01:00-05:00", 0, 50),
            rec("b", "claude-y", "2026-11-23T09:00:00-05:00", 200, 100),
            rec("c", "claude-x", "2026-11-23T10:00:00-05:00", 1, 1),
        ];
        records[1].title = Some("Arreglar el lector".into());
        records[1].project = Some("C:/src/proyecto".into());
        let s = summarize(&records, Period::Days7, now(), &lima());
        assert_eq!(s.top_tasks.len(), 3);
        assert_eq!(s.top_tasks[0].total, 300);
        assert_eq!(s.top_tasks[0].title, "Sin título");
        assert_eq!(s.top_tasks[0].model, "claude-y");
        assert_eq!(s.top_tasks[1].total, 150);
        assert_eq!(s.top_tasks[1].title, "Arreglar el lector");
        assert_eq!(s.top_tasks[1].project.as_deref(), Some("C:/src/proyecto"));
        assert_eq!(s.top_tasks[1].agent, AgentKind::ClaudeCode);

        let muchas: Vec<_> = (0..12)
            .map(|i| rec(&format!("s{i}"), "claude-x", "2026-11-23T08:00:00-05:00", i, 0))
            .collect();
        let s = summarize(&muchas, Period::Days7, now(), &lima());
        assert_eq!(s.top_tasks.len(), 10);
        assert_eq!(s.top_tasks[0].total, 11);
    }

    #[test]
    fn el_tiempo_de_agente_ignora_pausas_largas() {
        let records = vec![
            rec("a", "claude-x", "2026-11-23T08:00:00-05:00", 1, 1),
            rec("a", "claude-x", "2026-11-23T08:01:00-05:00", 1, 1),
            rec("a", "claude-x", "2026-11-23T08:02:00-05:00", 1, 1),
            // Pausa de 14 min 40 s: no cuenta.
            rec("a", "claude-x", "2026-11-23T08:16:40-05:00", 1, 1),
            rec("a", "claude-x", "2026-11-23T08:18:20-05:00", 1, 1),
            // Otra sesión intercalada no se mezcla con la anterior.
            rec("b", "claude-x", "2026-11-23T08:00:30-05:00", 1, 1),
            rec("b", "claude-x", "2026-11-23T08:05:30-05:00", 1, 1),
        ];
        let s = summarize(&records, Period::Days7, now(), &lima());
        assert_eq!(s.totals.agent_time_secs, 60 + 60 + 100 + 300);
    }

    #[test]
    fn cambio_porcentual_contra_el_periodo_anterior() {
        let records = vec![
            rec("a", "claude-x", "2026-11-22T08:00:00-05:00", 150, 30),
            // Período anterior de 7 días: 10 al 16 de noviembre.
            rec("b", "claude-x", "2026-11-10T00:00:00-05:00", 100, 40),
            rec("c", "claude-x", "2026-11-09T23:00:00-05:00", 999, 999),
        ];
        let s = summarize(&records, Period::Days7, now(), &lima());
        let prev = s.previous.clone().expect("hay período anterior");
        assert_eq!(prev.input, 100);
        assert_eq!(prev.output, 40);
        assert_eq!(prev.turns, 1);
        assert_eq!(pct_change(s.totals.input, prev.input), Some(50.0));
        assert_eq!(pct_change(s.totals.output, prev.output), Some(-25.0));
        assert_eq!(pct_change(5, 0), None);

        assert_eq!(summarize(&records, Period::All, now(), &lima()).previous, None);
    }
}
