//! Comandos con atajo (`[[comandos]]`): texto que se escribe en la terminal de la sesión o en la
//! acoplada con un atajo configurable, global o solo en una sesión.
//!
//! Reglas de los atajos (para no pisar al shell ni a Kuntur):
//! - **Ctrl+Shift+tecla** (Alt opcional) o una **tecla F** con algún modificador.
//! - Nada de Ctrl+Alt sin Shift: en teclados en español AltGr = Ctrl+Alt (escribiría `@`, `#`…).
//! - No se puede usar un atajo de Kuntur (Ctrl+Shift+T/W/B/E/P/U/A/O/R/M/C/V, Ctrl+Shift+Tab…).

use kuntur_config::ShortcutCommand;

/// Teclas de Ctrl+Shift que ya usa Kuntur (sesiones, paneles, copiar/pegar, MCP…).
pub const RESERVED: &[&str] = &[
    "t", "w", "b", "e", "p", "u", "a", "o", "r", "m", "c", "v", "tab", "=", "+", "-",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// Nombre de la tecla como lo da GPUI, en minúsculas (`t`, `5`, `f5`).
    pub key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShortcutError {
    Empty,
    UnknownPart(String),
    NeedsModifiers,
    Reserved(String),
}

impl std::fmt::Display for ShortcutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("Falta el atajo"),
            Self::UnknownPart(p) => write!(f, "No se reconoce «{p}» en el atajo"),
            Self::NeedsModifiers => f.write_str("Usa Ctrl+Shift+tecla, o una tecla F con Ctrl, Shift o Alt"),
            Self::Reserved(s) => write!(f, "{s} ya lo usa Kuntur"),
        }
    }
}

impl std::error::Error for ShortcutError {}

fn is_fkey(key: &str) -> bool {
    key.strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n))
}

impl Shortcut {
    /// `Ctrl+Shift+1`, `ctrl+alt+shift+k`, `Shift+F5`…
    pub fn parse(text: &str) -> Result<Shortcut, ShortcutError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ShortcutError::Empty);
        }
        let mut s = Shortcut {
            ctrl: false,
            shift: false,
            alt: false,
            key: String::new(),
        };
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        // `Ctrl+Shift++` → la última parte vacía es la tecla `+`.
        let (mods, key) = match parts.as_slice() {
            [rest @ .., "", ""] => (rest, "+"),
            [rest @ .., k] => (rest, *k),
            [] => return Err(ShortcutError::Empty),
        };
        for m in mods {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => s.ctrl = true,
                "shift" | "mayús" | "mayus" => s.shift = true,
                "alt" => s.alt = true,
                _ => return Err(ShortcutError::UnknownPart(m.to_string())),
            }
        }
        let key = key.to_lowercase();
        if key.is_empty() || (key.chars().count() > 1 && !is_fkey(&key)) {
            return Err(ShortcutError::UnknownPart(key));
        }
        s.key = key;
        let fkey = is_fkey(&s.key);
        if !(s.ctrl && s.shift || fkey && (s.ctrl || s.shift || s.alt)) {
            return Err(ShortcutError::NeedsModifiers);
        }
        if s.ctrl && s.shift && !s.alt && RESERVED.contains(&s.key.as_str()) {
            return Err(ShortcutError::Reserved(s.to_string()));
        }
        Ok(s)
    }

    pub fn matches(&self, ctrl: bool, shift: bool, alt: bool, key: &str) -> bool {
        self.ctrl == ctrl && self.shift == shift && self.alt == alt && self.key.eq_ignore_ascii_case(key)
    }
}

impl std::fmt::Display for Shortcut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        parts.push(self.key.to_uppercase());
        f.write_str(&parts.join("+"))
    }
}

/// Comandos que valen en la sesión `session` (los suyos primero, después los globales).
pub fn commands_for<'a>(commands: &'a [ShortcutCommand], session: &str) -> Vec<&'a ShortcutCommand> {
    let own = commands.iter().filter(|c| c.session.as_deref() == Some(session));
    let global = commands.iter().filter(|c| c.session.is_none());
    own.chain(global).collect()
}

/// El comando de esa tecla en la sesión (el de la sesión gana al global).
pub fn find<'a>(
    commands: &'a [ShortcutCommand],
    session: &str,
    ctrl: bool,
    shift: bool,
    alt: bool,
    key: &str,
) -> Option<&'a ShortcutCommand> {
    commands_for(commands, session)
        .into_iter()
        .find(|c| !c.text.is_empty() && Shortcut::parse(&c.shortcut).is_ok_and(|s| s.matches(ctrl, shift, alt, key)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(name: &str, shortcut: &str, session: Option<&str>) -> ShortcutCommand {
        ShortcutCommand {
            name: name.into(),
            text: format!("echo {name}"),
            shortcut: shortcut.into(),
            session: session.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn leer_atajos() {
        let s = Shortcut::parse("Ctrl+Shift+1").unwrap();
        assert!(s.ctrl && s.shift && !s.alt);
        assert_eq!(s.key, "1");
        assert_eq!(s.to_string(), "Ctrl+Shift+1");
        assert_eq!(
            Shortcut::parse("ctrl + alt + shift + K").unwrap().to_string(),
            "Ctrl+Alt+Shift+K"
        );
        assert_eq!(Shortcut::parse("Shift+F5").unwrap().key, "f5");
        assert_eq!(Shortcut::parse(" "), Err(ShortcutError::Empty));
        assert_eq!(
            Shortcut::parse("Hyper+K"),
            Err(ShortcutError::UnknownPart("Hyper".into()))
        );
        assert_eq!(
            Shortcut::parse("Ctrl+Shift+Espacio"),
            Err(ShortcutError::UnknownPart("espacio".into()))
        );
    }

    #[test]
    fn no_pisa_al_shell_ni_a_kuntur() {
        // Ctrl+letra es del shell (Ctrl+C…), Ctrl+Alt es AltGr en teclados en español.
        assert_eq!(Shortcut::parse("Ctrl+K"), Err(ShortcutError::NeedsModifiers));
        assert_eq!(Shortcut::parse("Ctrl+Alt+Q"), Err(ShortcutError::NeedsModifiers));
        assert_eq!(Shortcut::parse("F5"), Err(ShortcutError::NeedsModifiers));
        assert!(matches!(
            Shortcut::parse("Ctrl+Shift+T"),
            Err(ShortcutError::Reserved(_))
        ));
        assert!(matches!(
            Shortcut::parse("Ctrl+Shift+M"),
            Err(ShortcutError::Reserved(_))
        ));
        assert!(matches!(
            Shortcut::parse("Ctrl+Shift+C"),
            Err(ShortcutError::Reserved(_))
        ));
        // Con Alt además ya no es de Kuntur.
        assert!(Shortcut::parse("Ctrl+Alt+Shift+T").is_ok());
        assert!(Shortcut::parse("Ctrl+Shift+K").is_ok());
    }

    #[test]
    fn el_de_la_sesion_gana() {
        let all = vec![
            cmd("global", "Ctrl+Shift+1", None),
            cmd("api", "Ctrl+Shift+1", Some("api")),
            cmd("otro", "Ctrl+Shift+2", Some("web")),
            cmd("roto", "Ctrl+K", None),
        ];
        assert_eq!(find(&all, "api", true, true, false, "1").unwrap().name, "api");
        assert_eq!(find(&all, "web", true, true, false, "1").unwrap().name, "global");
        assert!(find(&all, "api", true, true, false, "2").is_none());
        assert!(find(&all, "api", true, false, false, "k").is_none());
        let names: Vec<&str> = commands_for(&all, "web").iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["otro", "global", "roto"]);
    }
}
