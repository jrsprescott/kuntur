//! Exportación a CSV de la auditoría (botón "Exportar CSV" de la vista): las filas que se ven con la
//! búsqueda y los filtros actuales.

use jiff::tz::TimeZone;

use crate::model::AuditRecord;

const HEADER: &str = "fecha,agente,chat,sesion,host,carpeta,herramienta,comando";

/// CSV (RFC 4180, con BOM UTF-8 para que Excel lea las tildes) de `rows`, en el orden recibido y con
/// la fecha en la zona local `tz`.
pub fn to_csv<'a>(rows: impl IntoIterator<Item = &'a AuditRecord>, tz: &TimeZone) -> String {
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
            field(r.chat.as_deref().unwrap_or("")),
            field(&r.session),
            field(r.host),
            field(r.folder.as_deref().unwrap_or("")),
            field(r.tool),
            field(&r.command),
        ];
        out.push_str(&fields.join(","));
        out.push_str("\r\n");
    }
    out
}

/// Texto como campo seguro: con un apóstrofo delante si empieza como una fórmula (`=`, `+`, `-`,
/// `@`, tabulador o retorno), para que una hoja de cálculo no la ejecute, y entre comillas si lleva
/// separadores. Los comandos suelen empezar con `-` o traer comillas y saltos de línea.
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
    use crate::model::LOCAL_HOST;
    use kuntur_usage::AgentKind;

    fn rec(command: &str) -> AuditRecord {
        AuditRecord {
            ts: "2026-11-23T14:00:00Z".parse::<jiff::Timestamp>().unwrap().as_second(),
            agent: AgentKind::Codex,
            session: "ses-1".into(),
            chat: Some("Demo".into()),
            host: LOCAL_HOST,
            folder: Some("C:/proyectos/demo".into()),
            command: command.into(),
            tool: "shell_command",
            key: None,
        }
    }

    #[test]
    fn filas_con_hora_local() {
        let rows = [rec("cargo test"), rec("echo \"hola, mundo\"\nls")];
        let csv = to_csv(&rows, &TimeZone::fixed(jiff::tz::offset(-5)));
        assert!(csv.starts_with('\u{feff}'));
        let body = csv.trim_start_matches('\u{feff}');
        let lines: Vec<&str> = body.split("\r\n").collect();
        assert_eq!(lines[0], HEADER);
        assert_eq!(
            lines[1],
            "2026-11-23 09:00:00,Codex,Demo,ses-1,local,C:/proyectos/demo,shell_command,cargo test"
        );
        assert!(body.contains("\"echo \"\"hola, mundo\"\"\nls\""));
    }

    #[test]
    fn a_prueba_de_formulas() {
        assert_eq!(field("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(field("-rf"), "'-rf");
        assert_eq!(field("+1"), "'+1");
        assert_eq!(field("@SUMA"), "'@SUMA");
        assert_eq!(field("\tx"), "'\tx");
        assert_eq!(field("ls"), "ls");
    }
}
