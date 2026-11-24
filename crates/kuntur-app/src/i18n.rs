//! Textos de la interfaz en español e inglés (ajustes v0.2).
//!
//! Cada texto es una variante de [`T`] con su par (español, inglés): agregar uno obliga a escribir
//! las dos traducciones. El idioma en uso es global (lo fija `Workspace` al arrancar y al cambiarlo
//! en Configuración), así el dibujo solo llama `t(T::Algo)`.
//!
//! Por ahora se traducen Configuración, el árbol de archivos y su menú; el resto de la app se pasa
//! por partes.

use std::sync::atomic::{AtomicU8, Ordering};

use kuntur_config::Language;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Fija el idioma de la interfaz.
pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::Es
    }
}

/// Idioma efectivo: el elegido o, en "automático", el del sistema (`system`, por ejemplo "es-PE",
/// "en_US.UTF-8"). Español si empieza con "es"; cualquier otro, inglés.
pub fn resolve(setting: Language, system: Option<&str>) -> Lang {
    match setting {
        Language::Es => Lang::Es,
        Language::En => Lang::En,
        Language::Auto => match system {
            Some(s) if s.trim().to_ascii_lowercase().starts_with("es") => Lang::Es,
            Some(_) => Lang::En,
            // Sin dato del sistema: el idioma original de Kuntur.
            None => Lang::Es,
        },
    }
}

/// Idioma del sistema (Windows, Linux y Mac).
pub fn system_locale() -> Option<String> {
    sys_locale::get_locale()
}

/// Texto en el idioma en uso.
pub fn t(key: T) -> &'static str {
    tr(current(), key)
}

/// Texto en un idioma dado.
pub fn tr(lang: Lang, key: T) -> &'static str {
    let (es, en) = key.pair();
    match lang {
        Lang::Es => es,
        Lang::En => en,
    }
}

macro_rules! texts {
    ($($name:ident => $es:literal, $en:literal;)*) => {
        /// Claves de los textos traducidos.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum T {
            $($name,)*
        }

        impl T {
            #[cfg(test)]
            pub const ALL: &'static [T] = &[$(T::$name,)*];

            fn pair(self) -> (&'static str, &'static str) {
                match self {
                    $(T::$name => ($es, $en),)*
                }
            }
        }
    };
}

texts! {
    // ── Configuración ──
    Settings => "Configuración", "Settings";
    SettingsTip => "Configuración (Ctrl+,)", "Settings (Ctrl+,)";
    ThemeTip => "Tema y apariencia", "Theme and appearance";
    Appearance => "Apariencia", "Appearance";
    Theme => "Tema", "Theme";
    FontSize => "Tamaño de letra", "Font size";
    FontSmaller => "Más chica", "Smaller";
    FontBigger => "Más grande", "Bigger";
    SidePanel => "Panel lateral", "Side panel";
    SidePanelFull => "Completo", "Full";
    SidePanelIcons => "Íconos", "Icons";
    Language => "Idioma", "Language";
    LanguageAuto => "Automático", "Automatic";
    Notifications => "Notificaciones", "Notifications";
    NotifyEnabled => "Mostrar notificaciones", "Show notifications";
    NotifyEnabledHint => "La insignia de la pestaña se muestra siempre", "The tab badge is always shown";
    NotifyWaiting => "Cuando espera tu respuesta", "When it waits for your reply";
    NotifyPermission => "Cuando pide permiso", "When it asks for permission";
    NotifyFinished => "Cuando termina", "When it finishes";
    NotifyInterval => "Tiempo mínimo entre avisos", "Minimum time between alerts";
    NotifyIntervalHint => "Por sesión; lo demás solo suma a la insignia", "Per session; the rest only adds to the badge";
    IntervalAlways => "Sin límite", "No limit";
    NotifyBackground => "Solo si Kuntur no está al frente", "Only when Kuntur is in the background";
    SettingsKeys => "↑↓ elegir · ←→ cambiar · Enter alternar · Esc cerrar", "↑↓ select · ←→ change · Enter toggle · Esc close";
    // ── Notificaciones ──
    ToastWaiting => "espera tu respuesta", "is waiting for your reply";
    ToastPermission => "pide permiso", "asks for permission";
    ToastFinished => "terminó", "finished";
    ToastSession => "Sesión", "Session";
}

#[cfg(test)]
mod tests {
    use super::{Lang, T, resolve, tr};
    use kuntur_config::Language;

    #[test]
    fn todos_los_textos_tienen_las_dos_traducciones() {
        for &key in T::ALL {
            assert!(!tr(Lang::Es, key).trim().is_empty(), "{key:?} sin español");
            assert!(!tr(Lang::En, key).trim().is_empty(), "{key:?} sin inglés");
        }
    }

    #[test]
    fn idioma_elegido_o_del_sistema() {
        assert_eq!(resolve(Language::Es, Some("en-US")), Lang::Es);
        assert_eq!(resolve(Language::En, Some("es-PE")), Lang::En);
        assert_eq!(resolve(Language::Auto, Some("es-PE")), Lang::Es);
        assert_eq!(resolve(Language::Auto, Some("ES_es.UTF-8")), Lang::Es);
        assert_eq!(resolve(Language::Auto, Some("en-GB")), Lang::En);
        assert_eq!(resolve(Language::Auto, Some("pt-BR")), Lang::En);
        assert_eq!(resolve(Language::Auto, None), Lang::Es);
    }

    #[test]
    fn el_menu_se_traduce() {
        assert_eq!(tr(Lang::Es, T::Settings), "Configuración");
        assert_eq!(tr(Lang::En, T::Settings), "Settings");
    }
}
