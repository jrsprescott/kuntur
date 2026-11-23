use serde::Deserialize;

/// Color con alfa. Acepta "#rrggbb" o "#rrggbbaa".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    pub fn parse(s: &str) -> Result<Self, String> {
        let hex = s.strip_prefix('#').ok_or_else(|| format!("color sin '#': {s}"))?;
        // Solo ASCII: así el corte por bytes de abajo nunca cae dentro de un carácter multibyte.
        if !hex.is_ascii() {
            return Err(format!("color inválido: {s}"));
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| format!("color inválido: {s}"));
        match hex.len() {
            6 => Ok(Self {
                r: byte(0)?,
                g: byte(2)?,
                b: byte(4)?,
                a: 0xff,
            }),
            8 => Ok(Self {
                r: byte(0)?,
                g: byte(2)?,
                b: byte(4)?,
                a: byte(6)?,
            }),
            _ => Err(format!("color inválido: {s}")),
        }
    }
}

impl TryFrom<String> for Rgba8 {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        Self::parse(&s)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Fonts {
    pub ui: String,
    pub mono: String,
    pub size: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Ui {
    pub window_gradient: Vec<Rgba8>,
    pub titlebar: Rgba8,
    pub surface: Rgba8,
    pub surface_2: Rgba8,
    pub border: Rgba8,
    pub popover: Rgba8,
    pub text: Rgba8,
    pub muted: Rgba8,
    pub accent: Rgba8,
    pub accent_2: Rgba8,
    pub ok: Rgba8,
    pub warn: Rgba8,
    pub error: Rgba8,
    pub terminal_bg: Rgba8,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Ansi {
    pub colors: [Rgba8; 16],
}

/// Un tema completo (UI + 16 colores ANSI + fuentes). La sección [syntax] del TOML se ignora aquí.
#[derive(Clone, Debug, Deserialize)]
pub struct Theme {
    pub name: String,
    pub fonts: Fonts,
    pub ui: Ui,
    pub ansi: Ansi,
}

impl Theme {
    pub fn from_toml(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// ¿Es un tema claro? (para elegir el modo claro/oscuro de los componentes de gpui-kit)
    pub fn is_light(&self) -> bool {
        let c = self.ui.text;
        (c.r as u32 + c.g as u32 + c.b as u32) < 3 * 128
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeId {
    Blue,
    Dark,
    White,
}

impl ThemeId {
    pub const ALL: [ThemeId; 3] = [ThemeId::Blue, ThemeId::Dark, ThemeId::White];

    pub fn key(self) -> &'static str {
        match self {
            ThemeId::Blue => "blue",
            ThemeId::Dark => "dark",
            ThemeId::White => "white",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.key() == key)
    }
}

/// Temas incorporados en el binario (no dependen de archivos externos).
pub fn builtin(id: ThemeId) -> Theme {
    let text = match id {
        ThemeId::Blue => include_str!("../../../themes/blue.toml"),
        ThemeId::Dark => include_str!("../../../themes/dark.toml"),
        ThemeId::White => include_str!("../../../themes/white.toml"),
    };
    Theme::from_toml(text).expect("tema incorporado inválido")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_hex_con_y_sin_alfa() {
        assert_eq!(
            Rgba8::parse("#0b1019").unwrap(),
            Rgba8 {
                r: 0x0b,
                g: 0x10,
                b: 0x19,
                a: 0xff
            }
        );
        assert_eq!(
            Rgba8::parse("#09144299").unwrap(),
            Rgba8 {
                r: 0x09,
                g: 0x14,
                b: 0x42,
                a: 0x99
            }
        );
        assert!(Rgba8::parse("0b1019").is_err());
        assert!(Rgba8::parse("#xyz").is_err());
    }

    #[test]
    fn los_tres_temas_incorporados_cargan() {
        for id in ThemeId::ALL {
            let t = builtin(id);
            assert!(!t.name.is_empty());
            assert!(t.fonts.size > 8.0);
            assert_eq!(t.ansi.colors.len(), 16);
        }
        assert_eq!(builtin(ThemeId::Blue).ui.window_gradient.len(), 3);
        assert_eq!(builtin(ThemeId::Dark).ui.text, Rgba8::parse("#d5dcea").unwrap());
    }

    #[test]
    fn id_de_tema_ida_y_vuelta() {
        for id in ThemeId::ALL {
            assert_eq!(ThemeId::from_key(id.key()), Some(id));
        }
        assert_eq!(ThemeId::from_key("rosa"), None);
    }
}
