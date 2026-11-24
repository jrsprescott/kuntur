//! Puente entre los temas de `kuntur-config` y los colores de GPUI / la paleta de la terminal.

use gpui_kit::component::{Theme as KitTheme, ThemeMode};
// Importes explícitos: el glob de gpui_kit trae su propia macro `test` y rompe `#[test]`.
use gpui_kit::{App, Background, Hsla, Rgba, SharedString, linear_color_stop, linear_gradient, solid_background};
use kuntur_config::{Rgba8, Theme};
use kuntur_term::{Palette, Rgb8};

/// Fuente de la terminal si la del tema no está instalada.
pub const FALLBACK_MONO: &str = "Cascadia Mono";
/// Fuente de la interfaz si la del tema no está instalada.
pub const FALLBACK_UI: &str = "Segoe UI";

/// Color del tema → color de GPUI, respetando el alfa.
pub fn to_hsla(c: Rgba8) -> Hsla {
    Rgba {
        r: c.r as f32 / 255.0,
        g: c.g as f32 / 255.0,
        b: c.b as f32 / 255.0,
        a: c.a as f32 / 255.0,
    }
    .into()
}

fn opaque(c: Rgba8) -> Rgb8 {
    Rgb8 { r: c.r, g: c.g, b: c.b }
}

/// Mezcla `top` (con su alfa) sobre un fondo opaco.
fn over(top: Rgba8, bottom: Rgb8) -> Rgb8 {
    let a = top.a as u32;
    let mix = |t: u8, b: u8| ((t as u32 * a + b as u32 * (255 - a) + 127) / 255) as u8;
    Rgb8 {
        r: mix(top.r, bottom.r),
        g: mix(top.g, bottom.g),
        b: mix(top.b, bottom.b),
    }
}

fn with_alpha(c: Rgba8, a: f32) -> Rgba8 {
    Rgba8 {
        a: (a.clamp(0.0, 1.0) * 255.0).round() as u8,
        ..c
    }
}

/// Paleta de la terminal: 16 ANSI del tema, texto = `ui.text`, fondo = `ui.terminal_bg` ya mezclado
/// sobre el primer color del degradado, cursor = acento y selección = acento al 30 % sobre el fondo.
pub fn palette_from(t: &Theme) -> Palette {
    let base =
        t.ui.window_gradient
            .first()
            .map(|&c| opaque(c))
            .unwrap_or_else(|| opaque(t.ui.surface));
    let bg = over(t.ui.terminal_bg, base);
    let ansi = t.ansi.colors.map(opaque);
    let selection = over(with_alpha(t.ui.accent, 0.30), bg);
    Palette::new(ansi, opaque(t.ui.text), bg, opaque(t.ui.accent), selection)
}

/// Colores de la interfaz ya convertidos (se recalculan al cambiar de tema).
#[derive(Clone)]
pub struct UiColors {
    pub window: Background,
    pub titlebar: Hsla,
    pub surface: Hsla,
    pub surface_2: Hsla,
    pub border: Hsla,
    pub popover: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    /// Estados: listo / esperando / error (también: sube / baja en las tarjetas de uso).
    pub ok: Hsla,
    pub warn: Hsla,
    pub error: Hsla,
    /// Fondo al pasar el mouse (texto al 7 %): funciona igual en temas claros y oscuros.
    pub hover: Hsla,
    /// Fondo del área de terminal (con alfa: deja ver el degradado de la ventana).
    pub terminal: Hsla,
}

impl UiColors {
    pub fn from_theme(t: &Theme) -> Self {
        let ui = &t.ui;
        let window = match ui.window_gradient.as_slice() {
            [first, .., last] => linear_gradient(
                140.0,
                linear_color_stop(to_hsla(*first), 0.0),
                linear_color_stop(to_hsla(*last), 1.0),
            ),
            [only] => solid_background(to_hsla(*only)),
            [] => solid_background(to_hsla(ui.surface)),
        };
        Self {
            window,
            titlebar: to_hsla(ui.titlebar),
            surface: to_hsla(ui.surface),
            surface_2: to_hsla(ui.surface_2),
            border: to_hsla(ui.border),
            popover: to_hsla(ui.popover),
            text: to_hsla(ui.text),
            muted: to_hsla(ui.muted),
            accent: to_hsla(ui.accent),
            ok: to_hsla(ui.ok),
            warn: to_hsla(ui.warn),
            error: to_hsla(ui.error),
            hover: to_hsla(with_alpha(ui.text, 0.07)),
            terminal: to_hsla(ui.terminal_bg),
        }
    }
}

/// Elige `wanted` si está instalada; si no, `fallback`.
pub fn pick_font(installed: &[String], wanted: &str, fallback: &str) -> SharedString {
    let name = installed
        .iter()
        .find(|f| f.eq_ignore_ascii_case(wanted))
        .map_or(fallback, String::as_str);
    SharedString::from(name.to_string())
}

/// Ajusta el tema de gpui-kit (modo claro/oscuro y colores base) para que sus componentes
/// —botones de ventana, tarjetas flotantes— combinen con el tema de Kuntur.
pub fn apply_to_kit(t: &Theme, cx: &mut App) {
    let mode = if t.is_light() {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
    KitTheme::change(mode, None, cx);
    let c = UiColors::from_theme(t);
    let ui = &t.ui;
    KitTheme::update(cx, |k| {
        // Botones de ventana: grises en reposo; toman el color del texto al pasar el mouse.
        k.foreground = c.muted;
        k.muted_foreground = c.muted;
        k.secondary_foreground = c.text;
        k.secondary_hover = to_hsla(with_alpha(ui.text, 0.10));
        k.secondary_active = to_hsla(with_alpha(ui.text, 0.16));
        k.title_bar = c.titlebar;
        k.title_bar_border = c.border;
        k.border = c.border;
        k.popover = c.popover;
        k.popover_foreground = c.text;
        k.accent = c.hover;
        k.accent_foreground = c.text;
    });
}

/// Contraste WCAG de los temas (F11, accesibilidad): cada color de texto contra cada fondo real
/// de la interfaz (las capas con alfa se mezclan sobre cada color del degradado de la ventana).
#[cfg(test)]
mod contraste {
    use super::{opaque, over};
    use kuntur_config::{Rgba8, Theme, ThemeId, builtin};
    use kuntur_term::Rgb8;

    /// Mínimo WCAG 2 AA para texto normal.
    const TEXTO: f32 = 4.5;
    /// Mínimo WCAG 2 AA para texto grande y componentes de la interfaz (texto tenue, estados).
    const TENUE: f32 = 3.0;

    /// Fondos sobre los que se dibuja texto: el degradado y cada capa de la interfaz mezclada sobre
    /// cada color del degradado (`surface_2` también sobre `surface`).
    fn fondos(t: &Theme) -> Vec<(String, Rgb8)> {
        let ui = &t.ui;
        let mut out = Vec::new();
        for (i, &stop) in ui.window_gradient.iter().enumerate() {
            let base = opaque(stop);
            out.push((format!("degradado[{i}]"), base));
            let capas: [(&str, Rgba8); 5] = [
                ("titlebar", ui.titlebar),
                ("surface", ui.surface),
                ("surface_2", ui.surface_2),
                ("popover", ui.popover),
                ("terminal_bg", ui.terminal_bg),
            ];
            for (nombre, capa) in capas {
                out.push((format!("{nombre} sobre degradado[{i}]"), over(capa, base)));
            }
            out.push((
                format!("surface_2 sobre surface sobre degradado[{i}]"),
                over(ui.surface_2, over(ui.surface, base)),
            ));
        }
        out
    }

    /// Pares (color, fondo) por debajo del mínimo, con su contraste.
    fn fallas(id: ThemeId) -> Vec<String> {
        let t = builtin(id);
        let ui = &t.ui;
        let colores: [(&str, Rgba8, f32); 6] = [
            ("text", ui.text, TEXTO),
            ("muted", ui.muted, TENUE),
            ("accent", ui.accent, TENUE),
            ("ok", ui.ok, TENUE),
            ("warn", ui.warn, TENUE),
            ("error", ui.error, TENUE),
        ];
        let mut out = Vec::new();
        for (fondo, bg) in fondos(&t) {
            for (nombre, color, min) in colores {
                let ratio = opaque(color).contrast(bg);
                if ratio < min {
                    out.push(format!("{}: {nombre} sobre {fondo} = {ratio:.2} (< {min})", id.key()));
                }
            }
        }
        out
    }

    #[test]
    fn los_tres_temas_cumplen_el_contraste_wcag() {
        let todas: Vec<String> = ThemeId::ALL.into_iter().flat_map(fallas).collect();
        assert!(todas.is_empty(), "contraste insuficiente:\n{}", todas.join("\n"));
    }

    #[test]
    fn la_prueba_detecta_un_tema_sin_contraste() {
        // Gris claro sobre blanco: 1,5:1.
        let gris = Rgb8::hex(0xcccccc);
        assert!(gris.contrast(Rgb8::hex(0xffffff)) < TENUE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuntur_config::{ThemeId, builtin};

    #[test]
    fn to_hsla_conserva_el_alfa() {
        let c = to_hsla(Rgba8::parse("#ffffff80").unwrap());
        assert!((c.a - 0.5).abs() < 0.01, "alfa = {}", c.a);
        assert!((c.l - 1.0).abs() < 0.01);
    }

    #[test]
    fn paleta_del_tema_dark() {
        let p = palette_from(&builtin(ThemeId::Dark));
        assert_eq!(p.fg, Rgb8::hex(0xd5dcea));
        // terminal_bg es opaco en Dark: no se mezcla.
        assert_eq!(p.bg, Rgb8::hex(0x0b1019));
        assert_eq!(p.cursor, Rgb8::hex(0x7aa2f7));
    }

    #[test]
    fn el_fondo_con_alfa_se_mezcla_sobre_el_degradado() {
        let p = palette_from(&builtin(ThemeId::Blue));
        // #050e34 al 40 % sobre #1a379a
        assert_eq!(
            p.bg,
            Rgb8 {
                r: 0x12,
                g: 0x27,
                b: 0x71
            }
        );
        // La selección queda entre el fondo y el acento.
        assert!(p.selection_bg.g > p.bg.g && p.selection_bg.g < 0xd3);
    }

    #[test]
    fn elige_la_fuente_instalada_o_la_de_respaldo() {
        let installed = vec!["Cascadia Mono".to_string(), "Fira Code".to_string()];
        assert_eq!(pick_font(&installed, "fira code", FALLBACK_MONO).as_ref(), "Fira Code");
        assert_eq!(
            pick_font(&installed, "JetBrains Mono", FALLBACK_MONO).as_ref(),
            FALLBACK_MONO
        );
    }
}
