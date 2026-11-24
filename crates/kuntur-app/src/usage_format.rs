//! Textos y niveles de la vista "Uso de agentes" (sin GPUI): números compactos, duraciones,
//! % de cambio, rango de fechas y niveles del mapa de calor. El `Workspace` los dibuja.

use jiff::tz::TimeZone;
use kuntur_usage::{ModelRow, Period, Summary, Totals};

const MONTHS: [&str; 12] = [
    "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
];

/// Número con separador de miles español: 1234567 → "1.234.567".
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Tokens en corto: "950", "12,3 k", "1,2 M", "3.400 M" (coma decimal, como en español).
pub fn compact(n: u64) -> String {
    let one_decimal = |v: f64, unit: &str| {
        let s = format!("{v:.1}");
        let s = s.strip_suffix(".0").unwrap_or(&s).replace('.', ",");
        format!("{s} {unit}")
    };
    match n {
        0..1_000 => n.to_string(),
        1_000..999_950 => one_decimal(n as f64 / 1e3, "k"),
        999_950..999_950_000 => one_decimal(n as f64 / 1e6, "M"),
        _ => format!("{} M", thousands((n as f64 / 1e6).round() as u64)),
    }
}

/// Dinero desde micro-dólares: "US$ 1.234,56"; entre 0 y 1 centavo, "< US$ 0,01".
pub fn usd(micros: u64) -> String {
    let cents = (micros + 5_000) / 10_000;
    if micros > 0 && cents == 0 {
        return "< US$ 0,01".into();
    }
    format!("US$ {},{:02}", thousands(cents / 100), cents % 100)
}

/// Detalle de la tarjeta de costo (tooltip): qué significa y de dónde sale.
pub fn cost_detail(t: &Totals) -> String {
    let mut parts =
        vec!["Precio de API: lo que costaría este uso por API, no lo que cobra una suscripción".to_string()];
    if t.real_cost_micros > 0 {
        parts.push(format!("{} informado por OpenCode", usd(t.real_cost_micros)));
    }
    if t.unpriced_turns > 0 {
        parts.push(format!(
            "{} turnos de modelos sin precio no suman (agrégalos en pricing.toml)",
            thousands(t.unpriced_turns)
        ));
    }
    parts.join(" · ")
}

/// Tiempo de agente: "0 min", "45 min", "2 h 05 min", "152 h".
pub fn duration(secs: u64) -> String {
    let mins = secs / 60;
    match mins {
        0..60 => format!("{mins} min"),
        60..600 => format!("{} h {:02} min", mins / 60, mins % 60),
        _ => format!("{} h", mins / 60),
    }
}

/// Cambio porcentual para la tarjeta: ("+12 %", sube) o ("−8 %", baja); `None` sin base.
pub fn change(pct: Option<f64>) -> Option<(String, bool)> {
    let p = pct.filter(|p| p.is_finite())?;
    let rounded = p.round() as i64;
    let text = match rounded {
        0 => "0 %".to_string(),
        r if r > 0 => format!("+{} %", thousands(r as u64)),
        r => format!("−{} %", thousands(r.unsigned_abs())),
    };
    Some((text, p >= 0.0))
}

/// Rango de fechas del período, en tenue bajo el título: "23 nov 2026" (Hoy),
/// "17 – 23 nov 2026", "24 nov 2025 – 23 nov 2026".
pub fn range(s: &Summary, tz: &TimeZone) -> String {
    let date = |ts: i64| {
        jiff::Timestamp::from_second(ts)
            .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
            .to_zoned_datetime(tz.clone())
            .date()
    };
    let first = date(s.start);
    // `end` es exclusivo (inicio de mañana): el último día es el anterior.
    let last = date(s.end - 1);
    let fmt = |d: jiff::civil::Date| format!("{} {} {}", d.day(), MONTHS[(d.month() - 1) as usize], d.year());
    if first == last {
        fmt(first)
    } else if first.year() == last.year() && first.month() == last.month() {
        format!("{} – {}", first.day(), fmt(last))
    } else {
        format!("{} – {}", fmt(first), fmt(last))
    }
}

/// Texto que compara con el período anterior ("vs. ayer", "vs. 7 días anteriores"…).
pub fn versus(period: Period) -> &'static str {
    match period {
        Period::Today => "vs. ayer",
        Period::Days7 => "vs. 7 días anteriores",
        Period::Days30 => "vs. 30 días anteriores",
        Period::Year => "vs. año anterior",
        Period::All => "",
    }
}

/// Texto de la tarjeta cuando no hay % de cambio: en "Todo" no hay período anterior; sin
/// historial que lo cubra, no se puede comparar; con historial pero sin uso, se dice eso.
pub fn no_change_note(period: Period, has_previous: bool) -> &'static str {
    match (period, has_previous) {
        (Period::All, _) => "Todo el historial",
        (_, false) => "sin datos para comparar",
        (_, true) => "sin uso en el período anterior",
    }
}

/// Intensidad 0–4 de cada celda del mapa de calor: 0 = sin turnos; 1–4 por cuartiles de las
/// celdas con turnos (así un pico aislado no apaga el resto).
pub fn heat_levels(heatmap: &[[u64; 24]; 7]) -> [[u8; 24]; 7] {
    let mut values: Vec<u64> = heatmap.iter().flatten().copied().filter(|&v| v > 0).collect();
    values.sort_unstable();
    let mut out = [[0u8; 24]; 7];
    if values.is_empty() {
        return out;
    }
    let q = |f: f64| values[((values.len() - 1) as f64 * f).round() as usize];
    let (q1, q2, q3) = (q(0.25), q(0.5), q(0.75));
    for (d, row) in heatmap.iter().enumerate() {
        for (h, &v) in row.iter().enumerate() {
            out[d][h] = match v {
                0 => 0,
                v if v <= q1 => 1,
                v if v <= q2 => 2,
                v if v <= q3 => 3,
                _ => 4,
            };
        }
    }
    out
}

/// Porcentaje entero de `part` sobre `total` (0 si no hay total).
pub fn share(part: u64, total: u64) -> u64 {
    if total == 0 {
        0
    } else {
        ((part as f64 / total as f64) * 100.0).round() as u64
    }
}

/// Entrada + salida por proveedor, de mayor a menor (barra de participación de "Modelos").
pub fn providers(models: &[ModelRow]) -> Vec<(&'static str, u64)> {
    let mut out: Vec<(&'static str, u64)> = Vec::new();
    for m in models {
        let total = m.input + m.output;
        match out.iter_mut().find(|(p, _)| *p == m.provider) {
            Some((_, sum)) => *sum += total,
            None => out.push((m.provider, total)),
        }
    }
    out.retain(|(_, total)| *total > 0);
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    out
}

/// Último segmento de una carpeta ("C:/proyectos/demo" → "demo"), con `/` o `\`.
pub fn folder_name(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuntur_usage::summarize;

    #[test]
    fn numeros_compactos() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(1_234_567), "1.234.567");
        assert_eq!(compact(950), "950");
        assert_eq!(compact(1_000), "1 k");
        assert_eq!(compact(12_340), "12,3 k");
        assert_eq!(compact(999_949), "999,9 k");
        assert_eq!(compact(999_950), "1 M");
        assert_eq!(compact(1_260_000), "1,3 M");
        assert_eq!(compact(3_400_000_000), "3.400 M");
    }

    #[test]
    fn dinero() {
        assert_eq!(usd(0), "US$ 0,00");
        assert_eq!(usd(4_000), "< US$ 0,01");
        assert_eq!(usd(5_000), "US$ 0,01");
        assert_eq!(usd(12_345_678), "US$ 12,35");
        assert_eq!(usd(1_234_560_000), "US$ 1.234,56");
    }

    #[test]
    fn detalle_del_costo() {
        let mut t = Totals::default();
        assert!(cost_detail(&t).starts_with("Precio de API"));
        t.real_cost_micros = 2_500_000;
        t.unpriced_turns = 3;
        let d = cost_detail(&t);
        assert!(d.contains("US$ 2,50 informado por OpenCode"));
        assert!(d.contains("3 turnos de modelos sin precio"));
    }

    #[test]
    fn duraciones() {
        assert_eq!(duration(30), "0 min");
        assert_eq!(duration(45 * 60), "45 min");
        assert_eq!(duration(2 * 3600 + 5 * 60), "2 h 05 min");
        assert_eq!(duration(152 * 3600 + 59 * 60), "152 h");
    }

    #[test]
    fn cambio_porcentual() {
        assert_eq!(change(Some(12.4)), Some(("+12 %".into(), true)));
        assert_eq!(change(Some(-7.6)), Some(("−8 %".into(), false)));
        assert_eq!(change(Some(0.2)), Some(("0 %".into(), true)));
        assert_eq!(change(Some(1520.0)), Some(("+1.520 %".into(), true)));
        assert_eq!(change(None), None);
        assert_eq!(change(Some(f64::NAN)), None);
    }

    fn summary(period: Period) -> Summary {
        let now = "2026-11-23T12:00:00-05:00"
            .parse::<jiff::Timestamp>()
            .unwrap()
            .as_second();
        summarize(&[], period, now, &lima())
    }

    fn lima() -> TimeZone {
        TimeZone::fixed(jiff::tz::offset(-5))
    }

    #[test]
    fn rangos_de_fechas() {
        assert_eq!(range(&summary(Period::Today), &lima()), "23 nov 2026");
        assert_eq!(range(&summary(Period::Days7), &lima()), "17 – 23 nov 2026");
        assert_eq!(range(&summary(Period::Days30), &lima()), "25 oct 2026 – 23 nov 2026");
        assert_eq!(range(&summary(Period::Year), &lima()), "24 nov 2025 – 23 nov 2026");
    }

    #[test]
    fn niveles_del_mapa_por_cuartiles() {
        let mut h = [[0u64; 24]; 7];
        h[0][9] = 1;
        h[0][10] = 2;
        h[0][11] = 3;
        h[0][12] = 4;
        h[4][15] = 100;
        let l = heat_levels(&h);
        assert_eq!(l[0][0], 0);
        assert_eq!(l[0][9], 1);
        assert_eq!(l[0][11], 2);
        assert_eq!(l[0][12], 3);
        assert_eq!(l[4][15], 4);
        assert_eq!(heat_levels(&[[0; 24]; 7]), [[0; 24]; 7]);
    }

    #[test]
    fn texto_sin_cambio() {
        assert_eq!(no_change_note(Period::All, false), "Todo el historial");
        assert_eq!(no_change_note(Period::Days7, false), "sin datos para comparar");
        assert_eq!(no_change_note(Period::Today, true), "sin uso en el período anterior");
        assert_eq!(versus(Period::Today), "vs. ayer");
        assert_eq!(versus(Period::All), "");
    }

    #[test]
    fn suma_por_proveedor() {
        let row = |model: &str, provider: &'static str, input: u64| ModelRow {
            model: model.into(),
            provider,
            input,
            output: 0,
            turns: 1,
            cost_micros: None,
        };
        let models = vec![
            row("claude-a", "Anthropic", 10),
            row("gpt-5", "OpenAI", 25),
            row("claude-b", "Anthropic", 20),
            row("vacio", "Otros", 0),
        ];
        assert_eq!(providers(&models), vec![("Anthropic", 30), ("OpenAI", 25)]);
    }

    #[test]
    fn porcentajes_y_carpetas() {
        assert_eq!(share(1, 3), 33);
        assert_eq!(share(5, 0), 0);
        assert_eq!(folder_name("C:/proyectos/demo"), "demo");
        assert_eq!(folder_name("C:\\src\\kuntur\\"), "kuntur");
        assert_eq!(folder_name("/srv/app"), "app");
        assert_eq!(folder_name("solo"), "solo");
    }
}
