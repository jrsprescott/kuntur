//! Valores TOML escritos a mano para las sobrescrituras `-c clave=valor` de Codex.
//!
//! Se prefieren las cadenas literales (`'…'`): no llevan comillas dobles, que Windows PowerShell 5.1
//! pierde al pasar argumentos a programas nativos.

use std::collections::BTreeMap;

/// Cadena TOML: literal si se puede; si no, básica con escapes.
pub fn string(s: &str) -> String {
    if !s.contains('\'') && !s.chars().any(char::is_control) {
        return format!("'{s}'");
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Clave TOML: sin comillas si es "simple" (`A-Za-z0-9_-`).
pub fn key(k: &str) -> String {
    if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        k.to_string()
    } else {
        string(k)
    }
}

pub fn array(items: &[String]) -> String {
    let parts: Vec<String> = items.iter().map(|s| string(s)).collect();
    format!("[{}]", parts.join(","))
}

pub fn inline_table(map: &BTreeMap<String, String>) -> String {
    let parts: Vec<String> = map.iter().map(|(k, v)| format!("{}={}", key(k), string(v))).collect();
    format!("{{{}}}", parts.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literales_sin_comillas_dobles() {
        assert_eq!(string(r"C:\Herramientas\srv.exe"), r"'C:\Herramientas\srv.exe'");
        assert_eq!(string("it's"), "\"it's\"");
        assert_eq!(string("a\"b\\c'"), r#""a\"b\\c'""#);
        assert_eq!(key("mi_servidor-1"), "mi_servidor-1");
        assert_eq!(key("con espacio"), "'con espacio'");
        assert_eq!(array(&["-y".into(), "pkg".into()]), "['-y','pkg']");
        let m: BTreeMap<String, String> = [("LOG".into(), "info".into())].into();
        assert_eq!(inline_table(&m), "{LOG='info'}");
    }

    #[test]
    fn lo_que_se_escribe_se_lee_igual() {
        // Los valores generados son TOML válido con el mismo contenido.
        for s in ["simple", r"C:\a b\c", "it's", "x\"y", "tab\tz"] {
            let doc = format!("v = {}", string(s));
            let t: toml::Table = toml::from_str(&doc).unwrap();
            assert_eq!(t["v"].as_str(), Some(s), "{doc}");
        }
        let m: BTreeMap<String, String> = [("A B".into(), "x'y".into()), ("C".into(), "1".into())].into();
        let doc = format!("t = {}\na = {}", inline_table(&m), array(&["it's".into(), "z".into()]));
        let t: toml::Table = toml::from_str(&doc).unwrap();
        assert_eq!(t["t"]["A B"].as_str(), Some("x'y"));
        assert_eq!(t["a"][0].as_str(), Some("it's"));
    }
}
