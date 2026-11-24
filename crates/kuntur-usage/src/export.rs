//! Exportación a CSV de los registros de un período (botón "Exportar CSV" de la vista de uso).

use jiff::tz::TimeZone;

use crate::UsageRecord;

const HEADER: &str = "fecha,agente,modelo,entrada,salida,cache_lectura,cache_escritura,sesion,titulo,carpeta";

/// CSV (RFC 4180, con BOM UTF-8 para que Excel lea las tildes) de los registros con
/// `start <= ts < end`, en orden cronológico y con la fecha en la zona local `tz`.
pub fn to_csv(records: &[UsageRecord], start: i64, end: i64, tz: &TimeZone) -> String {
    let mut rows: Vec<&UsageRecord> = records.iter().filter(|r| r.ts >= start && r.ts < end).collect();
    rows.sort_by_key(|r| r.ts);
    let mut out = String::from("\u{feff}");
    out.push_str(HEADER);
    out.push_str("\r\n");
    for r in rows {
        let date = jiff::Timestamp::from_second(r.ts)
            .map(|t| {
                t.to_zoned_datetime(tz.clone())
                    .strftime("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default();
        let fields = [
            date,
            r.agent.name().to_string(),
            field(&r.model),
            r.input.to_string(),
            r.output.to_string(),
            r.cache_read.to_string(),
            r.cache_write.to_string(),
            field(&r.session_id),
            field(r.title.as_deref().unwrap_or("")),
            field(r.project.as_deref().unwrap_or("")),
        ];
        out.push_str(&fields.join(","));
        out.push_str("\r\n");
    }
    out
}

/// Texto de los historiales como campo seguro: entre comillas si lleva separadores, y con un
/// apóstrofo delante si empieza como una fórmula (`=`, `+`, `-`, `@`), para que una hoja de
/// cálculo no la ejecute.
fn field(text: &str) -> String {
    let text = if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{text}")
    } else {
        text.to_string()
    };
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentKind;

    fn ts(s: &str) -> i64 {
        s.parse::<jiff::Timestamp>().unwrap().as_second()
    }

    fn rec(at: &str, title: Option<&str>) -> UsageRecord {
        UsageRecord {
            agent: AgentKind::Codex,
            model: "gpt-5".into(),
            ts: ts(at),
            input: 10,
            output: 2,
            cache_read: 3,
            cache_write: 0,
            session_id: "s1".into(),
            title: title.map(str::to_string),
            project: Some("C:/proyectos/demo".into()),
        }
    }

    #[test]
    fn filtra_ordena_y_usa_la_hora_local() {
        let records = vec![
            rec("2026-11-23T15:00:00Z", Some("Segundo")),
            rec("2026-11-23T14:00:00Z", Some("Primero")),
            rec("2026-11-20T14:00:00Z", Some("Fuera")),
        ];
        let tz = TimeZone::fixed(jiff::tz::offset(-5));
        let csv = to_csv(
            &records,
            ts("2026-11-23T00:00:00-05:00"),
            ts("2026-11-24T00:00:00-05:00"),
            &tz,
        );
        let lines: Vec<&str> = csv.trim_start_matches('\u{feff}').split("\r\n").collect();
        assert_eq!(lines[0], HEADER);
        assert_eq!(
            lines[1],
            "2026-11-23 09:00:00,Codex,gpt-5,10,2,3,0,s1,Primero,C:/proyectos/demo"
        );
        assert!(lines[2].contains("Segundo"));
        assert_eq!(lines.len(), 4, "dos filas y la línea vacía final");
        assert!(csv.starts_with('\u{feff}'));
    }

    #[test]
    fn campos_con_comas_comillas_y_formulas() {
        assert_eq!(field("simple"), "simple");
        assert_eq!(field("uno, dos"), "\"uno, dos\"");
        assert_eq!(field("dice \"hola\""), "\"dice \"\"hola\"\"\"");
        assert_eq!(field("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(field("+1"), "'+1");
        assert_eq!(field("@SUMA"), "'@SUMA");
    }
}
