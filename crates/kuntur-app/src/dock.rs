//! Terminal acoplada (F4): estado por sesión, foco, límites del separador y carpeta inicial. Sin
//! GPUI: se prueba aparte; el `Workspace` crea la vista y dibuja.

use std::path::{Path, PathBuf};

/// Proporción inicial del panel acoplado (alto del acoplado / alto del área de terminales).
pub const DEFAULT_RATIO: f32 = 0.35;
/// Límites de la proporción: ni el acoplado ni la principal desaparecen.
pub const MIN_RATIO: f32 = 0.15;
pub const MAX_RATIO: f32 = 0.85;
/// Alto mínimo del acoplado (como la simulación) y de la terminal principal, en píxeles.
pub const MIN_DOCK_PX: f32 = 110.0;
pub const MIN_MAIN_PX: f32 = 140.0;

/// Panel de la sesión que tiene (o tuvo por última vez) el teclado.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Slot {
    #[default]
    Main,
    Dock,
}

/// Qué hacer después de Ctrl+`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    /// Estaba oculto: mostrarlo (creándolo si no existe) y enfocarlo.
    Show,
    /// Estaba visible sin el teclado: enfocarlo.
    Focus,
    /// Estaba visible con el teclado: ocultarlo (sin destruirlo) y volver a la principal.
    Hide,
}

/// Estado del panel acoplado de una sesión.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockState {
    pub open: bool,
    ratio: f32,
    focus: Slot,
}

impl Default for DockState {
    fn default() -> Self {
        Self {
            open: false,
            ratio: DEFAULT_RATIO,
            focus: Slot::Main,
        }
    }
}

impl DockState {
    /// Estado con la proporción guardada (corregida si viene rota del archivo).
    pub fn with_ratio(ratio: Option<f32>) -> Self {
        Self {
            ratio: ratio.map_or(DEFAULT_RATIO, clamp_ratio),
            ..Self::default()
        }
    }

    pub fn ratio(&self) -> f32 {
        self.ratio
    }

    pub fn set_ratio(&mut self, ratio: f32) {
        self.ratio = clamp_ratio(ratio);
    }

    /// Proporción a guardar en la configuración: `None` si es la de siempre (archivo limpio).
    pub fn saved_ratio(&self) -> Option<f32> {
        ((self.ratio - DEFAULT_RATIO).abs() > 0.001).then_some((self.ratio * 1000.0).round() / 1000.0)
    }

    /// Panel que debe recibir el teclado: con el acoplado oculto, siempre la principal.
    pub fn focus(&self) -> Slot {
        if self.open { self.focus } else { Slot::Main }
    }

    /// Clic (u otro motivo) que lleva el teclado a un panel.
    pub fn set_focus(&mut self, slot: Slot) {
        self.focus = if self.open { slot } else { Slot::Main };
    }

    /// Ctrl+`, como en VS Code. `focused` es el panel que tiene el teclado ahora mismo.
    pub fn toggle(&mut self, focused: Slot) -> Toggle {
        let outcome = if !self.open {
            Toggle::Show
        } else if focused == Slot::Dock {
            Toggle::Hide
        } else {
            Toggle::Focus
        };
        match outcome {
            Toggle::Show | Toggle::Focus => {
                self.open = true;
                self.focus = Slot::Dock;
            }
            Toggle::Hide => self.hide(),
        }
        outcome
    }

    /// Oculta el acoplado (sigue vivo) y el teclado vuelve a la principal.
    pub fn hide(&mut self) {
        self.open = false;
        self.focus = Slot::Main;
    }

    /// El shell del acoplado terminó: se cierra; la proporción se conserva para la próxima vez.
    pub fn dock_exited(&mut self) {
        self.hide();
    }
}

/// Corrige una proporción: NaN o infinita → la de siempre; si no, dentro de los límites.
pub fn clamp_ratio(ratio: f32) -> f32 {
    if ratio.is_finite() {
        ratio.clamp(MIN_RATIO, MAX_RATIO)
    } else {
        DEFAULT_RATIO
    }
}

/// Proporción al arrastrar el separador: `top` y `height` son el área de las dos terminales y
/// `pointer_y` la posición del mouse (mismas coordenadas). El acoplado queda debajo del puntero,
/// con al menos `MIN_DOCK_PX` y dejando `MIN_MAIN_PX` a la principal.
pub fn ratio_for_pointer(top: f32, height: f32, pointer_y: f32) -> f32 {
    if !(height.is_finite() && height > 0.0 && pointer_y.is_finite()) {
        return DEFAULT_RATIO;
    }
    if height < MIN_DOCK_PX + MIN_MAIN_PX {
        return 0.5;
    }
    let dock_px = (top + height - pointer_y).clamp(MIN_DOCK_PX, height - MIN_MAIN_PX);
    clamp_ratio(dock_px / height)
}

/// Carpeta donde arranca el acoplado: la actual de la sesión si existe; si no, la personal.
pub fn start_dir(session_cwd: Option<PathBuf>, home: Option<&Path>) -> Option<PathBuf> {
    session_cwd
        .filter(|p| p.is_dir())
        .or_else(|| home.filter(|h| h.is_dir()).map(Path::to_path_buf))
}

/// ¿La tecla es Ctrl+`? En las distribuciones en español esa misma tecla física es la Ñ (igual
/// que en VS Code), así que también vale Ctrl+Ñ. Con Shift, Alt (o AltGr) no.
pub fn is_toggle_key(key: &str, control: bool, shift: bool, alt: bool) -> bool {
    control && !shift && !alt && matches!(key, "`" | "ñ" | "Ñ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arranca_oculto_con_el_foco_en_la_principal() {
        let d = DockState::default();
        assert!(!d.open);
        assert_eq!(d.focus(), Slot::Main);
        assert_eq!(d.ratio(), DEFAULT_RATIO);
        assert_eq!(d.saved_ratio(), None);
    }

    #[test]
    fn ctrl_acento_muestra_enfoca_y_oculta_como_vs_code() {
        let mut d = DockState::default();
        assert_eq!(d.toggle(Slot::Main), Toggle::Show);
        assert!(d.open);
        assert_eq!(d.focus(), Slot::Dock);
        // Clic en la principal: el acoplado sigue visible, sin teclado → Ctrl+` lo enfoca.
        d.set_focus(Slot::Main);
        assert_eq!(d.toggle(Slot::Main), Toggle::Focus);
        assert_eq!(d.focus(), Slot::Dock);
        // Con el teclado en el acoplado → se oculta y vuelve a la principal.
        assert_eq!(d.toggle(Slot::Dock), Toggle::Hide);
        assert!(!d.open);
        assert_eq!(d.focus(), Slot::Main);
        // Y vuelve a mostrarse.
        assert_eq!(d.toggle(Slot::Main), Toggle::Show);
        assert!(d.open);
    }

    #[test]
    fn oculto_no_puede_tener_el_foco() {
        let mut d = DockState::default();
        d.set_focus(Slot::Dock);
        assert_eq!(d.focus(), Slot::Main);
    }

    #[test]
    fn al_terminar_su_shell_se_cierra_y_conserva_el_alto() {
        let mut d = DockState::with_ratio(Some(0.5));
        d.toggle(Slot::Main);
        d.dock_exited();
        assert!(!d.open);
        assert_eq!(d.focus(), Slot::Main);
        assert_eq!(d.ratio(), 0.5);
    }

    #[test]
    fn proporcion_guardada_y_valores_rotos() {
        assert_eq!(DockState::with_ratio(None).ratio(), DEFAULT_RATIO);
        assert_eq!(DockState::with_ratio(Some(0.42)).ratio(), 0.42);
        assert_eq!(DockState::with_ratio(Some(f32::NAN)).ratio(), DEFAULT_RATIO);
        assert_eq!(DockState::with_ratio(Some(5.0)).ratio(), MAX_RATIO);
        assert_eq!(DockState::with_ratio(Some(-1.0)).ratio(), MIN_RATIO);
        let mut d = DockState::default();
        d.set_ratio(0.123_456);
        assert_eq!(d.ratio(), MIN_RATIO);
        d.set_ratio(0.456_789);
        assert_eq!(d.saved_ratio(), Some(0.457));
    }

    #[test]
    fn arrastrar_el_separador_respeta_los_minimos() {
        // Área de 600 px que empieza en y = 40.
        let (top, h) = (40.0, 600.0);
        // Separador a 400 px del borde superior: acoplado de 240 px → 0,4.
        assert!((ratio_for_pointer(top, h, top + 360.0) - 0.4).abs() < 1e-4);
        // Muy abajo: el acoplado no baja de 110 px (0,183…).
        assert!((ratio_for_pointer(top, h, top + 590.0) - 110.0 / 600.0).abs() < 1e-4);
        // Muy arriba: la principal conserva 140 px; el tope es 0,85 si eso da más.
        assert!((ratio_for_pointer(top, h, top) - 460.0 / 600.0).abs() < 1e-4);
        let big = 2000.0;
        assert!((ratio_for_pointer(0.0, big, 100.0) - MAX_RATIO).abs() < 1e-4);
        assert!((ratio_for_pointer(0.0, 700.0, 0.0) - (700.0 - 140.0) / 700.0).abs() < 1e-4);
    }

    #[test]
    fn arrastrar_en_un_area_chica_o_rota() {
        assert_eq!(ratio_for_pointer(0.0, 200.0, 50.0), 0.5);
        assert_eq!(ratio_for_pointer(0.0, 0.0, 50.0), DEFAULT_RATIO);
        assert_eq!(ratio_for_pointer(0.0, 600.0, f32::NAN), DEFAULT_RATIO);
    }

    #[test]
    fn carpeta_inicial_la_de_la_sesion_o_la_personal() {
        let tmp = std::env::temp_dir();
        let missing = tmp.join("kuntur-no-existe-f4-xyz");
        assert_eq!(start_dir(Some(tmp.clone()), None), Some(tmp.clone()));
        assert_eq!(start_dir(Some(missing.clone()), Some(&tmp)), Some(tmp.clone()));
        assert_eq!(start_dir(None, Some(&tmp)), Some(tmp.clone()));
        assert_eq!(start_dir(None, Some(&missing)), None);
        assert_eq!(start_dir(None, None), None);
    }

    #[test]
    fn tecla_ctrl_acento_y_ctrl_enie() {
        assert!(is_toggle_key("`", true, false, false));
        assert!(is_toggle_key("ñ", true, false, false));
        assert!(!is_toggle_key("`", false, false, false));
        assert!(!is_toggle_key("`", true, true, false));
        assert!(!is_toggle_key("ñ", true, false, true), "AltGr llega como Ctrl+Alt");
        assert!(!is_toggle_key("n", true, false, false));
    }
}
