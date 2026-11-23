//! Paleta de comandos (Ctrl+Shift+P): lista de acciones, filtro y estado. Sin GPUI: se prueba
//! aparte; el `Workspace` la dibuja y ejecuta la acción elegida.

use kuntur_config::ThemeId;

/// Largo máximo del nombre de una sesión.
pub const MAX_NAME: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteAction {
    NewSession,
    RenameSession,
    CloseSession,
    NextSession,
    PrevSession,
    Theme(ThemeId),
    RailFull,
    RailIcons,
    FontBigger,
    FontSmaller,
    FontReset,
}

#[derive(Clone, Copy, Debug)]
pub struct PaletteItem {
    pub action: PaletteAction,
    pub label: &'static str,
    pub shortcut: Option<&'static str>,
}

const fn item(action: PaletteAction, label: &'static str, shortcut: Option<&'static str>) -> PaletteItem {
    PaletteItem {
        action,
        label,
        shortcut,
    }
}

/// Todas las acciones, en el orden en que se muestran.
pub const ITEMS: [PaletteItem; 13] = [
    item(PaletteAction::NewSession, "Nueva sesión", Some("Ctrl+Shift+T")),
    item(PaletteAction::RenameSession, "Renombrar sesión", None),
    item(PaletteAction::CloseSession, "Cerrar sesión", Some("Ctrl+Shift+W")),
    item(PaletteAction::NextSession, "Siguiente sesión", Some("Ctrl+Tab")),
    item(PaletteAction::PrevSession, "Sesión anterior", Some("Ctrl+Shift+Tab")),
    item(PaletteAction::Theme(ThemeId::Blue), "Tema: Blue", None),
    item(PaletteAction::Theme(ThemeId::Dark), "Tema: Dark", None),
    item(PaletteAction::Theme(ThemeId::White), "Tema: White", None),
    item(PaletteAction::RailFull, "Riel: completo", Some("Ctrl+Shift+B")),
    item(PaletteAction::RailIcons, "Riel: solo íconos", Some("Ctrl+Shift+B")),
    item(PaletteAction::FontBigger, "Fuente: más grande", Some("Ctrl+=")),
    item(PaletteAction::FontSmaller, "Fuente: más chica", Some("Ctrl+-")),
    item(PaletteAction::FontReset, "Fuente: restablecer", Some("Ctrl+0")),
];

/// Minúsculas y sin tildes; la puntuación cuenta como espacio.
fn normalize(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            c if c.is_alphanumeric() || c == '+' || c == '-' || c == '=' => c,
            _ => ' ',
        })
        .collect()
}

/// ¿`label` coincide con lo escrito? Sin distinguir mayúsculas ni tildes; cada palabra de la
/// consulta debe aparecer en la etiqueta ("tema blu" → "Tema: Blue").
pub fn matches(query: &str, label: &str) -> bool {
    let label = normalize(label);
    normalize(query).split_whitespace().all(|w| label.contains(w))
}

/// Qué hace el campo de texto: filtrar acciones o escribir el nombre nuevo de una sesión.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteMode {
    Search,
    Rename { id: u64, current: String },
}

/// Estado de la paleta abierta.
#[derive(Clone, Debug)]
pub struct PaletteState {
    pub mode: PaletteMode,
    pub query: String,
    pub selected: usize,
    /// El texto es el nombre actual "seleccionado": lo primero que se escribe lo reemplaza.
    pub replace_on_type: bool,
}

impl Default for PaletteState {
    fn default() -> Self {
        Self::new()
    }
}

impl PaletteState {
    pub fn new() -> Self {
        Self {
            mode: PaletteMode::Search,
            query: String::new(),
            selected: 0,
            replace_on_type: false,
        }
    }

    /// Acciones que coinciden con lo escrito (vacío en modo renombrar).
    pub fn filtered(&self) -> Vec<&'static PaletteItem> {
        match self.mode {
            PaletteMode::Search => ITEMS.iter().filter(|i| matches(&self.query, i.label)).collect(),
            PaletteMode::Rename { .. } => Vec::new(),
        }
    }

    pub fn selected_action(&self) -> Option<PaletteAction> {
        self.filtered().get(self.selected).map(|i| i.action)
    }

    /// Agrega texto escrito o pegado (solo la primera línea, sin caracteres de control).
    pub fn insert(&mut self, text: &str) {
        if std::mem::take(&mut self.replace_on_type) {
            self.query.clear();
        }
        let line = text.lines().next().unwrap_or("");
        for c in line.chars().filter(|c| !c.is_control()) {
            if matches!(self.mode, PaletteMode::Rename { .. }) && self.query.chars().count() >= MAX_NAME {
                break;
            }
            self.query.push(c);
        }
        self.selected = 0;
    }

    pub fn backspace(&mut self) {
        if std::mem::take(&mut self.replace_on_type) {
            self.query.clear();
        } else {
            self.query.pop();
        }
        self.selected = 0;
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.replace_on_type = false;
        self.selected = 0;
    }

    pub fn select_next(&mut self) {
        let n = self.filtered().len();
        if n > 0 {
            self.selected = (self.selected + 1) % n;
        }
    }

    pub fn select_prev(&mut self) {
        let n = self.filtered().len();
        if n > 0 {
            self.selected = (self.selected + n - 1) % n;
        }
    }

    /// Pasa al modo renombrar: el campo arranca con el nombre actual, listo para reemplazarlo.
    pub fn start_rename(&mut self, id: u64, current: &str) {
        self.mode = PaletteMode::Rename {
            id,
            current: current.to_string(),
        };
        self.query = current.to_string();
        self.replace_on_type = true;
        self.selected = 0;
    }

    /// Nombre listo para guardar: sin espacios sobrantes; `None` si quedó vacío.
    pub fn rename_value(&self) -> Option<String> {
        let name = self.query.trim();
        (!name.is_empty()).then(|| name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consulta_vacia_coincide_con_todo() {
        assert!(matches("", "Nueva sesión"));
        assert!(matches("   ", "Tema: Blue"));
    }

    #[test]
    fn sin_mayusculas_ni_tildes() {
        assert!(matches("fuente", "Fuente: más grande"));
        assert!(matches("MAS GRANDE", "Fuente: más grande"));
        assert!(matches("sesion", "Nueva sesión"));
        assert!(matches("íconos", "Riel: solo íconos"));
        assert!(matches("iconos", "Riel: solo íconos"));
    }

    #[test]
    fn cada_palabra_debe_aparecer() {
        assert!(matches("tema blu", "Tema: Blue"));
        assert!(matches("blu tema", "Tema: Blue"));
        assert!(!matches("tema white", "Tema: Blue"));
        assert!(!matches("zzz", "Nueva sesión"));
    }

    #[test]
    fn filtra_y_navega_dando_la_vuelta() {
        let mut p = PaletteState::new();
        assert_eq!(p.filtered().len(), ITEMS.len());
        p.insert("tema");
        assert_eq!(p.filtered().len(), 3);
        assert_eq!(p.selected_action(), Some(PaletteAction::Theme(ThemeId::Blue)));
        p.select_prev();
        assert_eq!(p.selected_action(), Some(PaletteAction::Theme(ThemeId::White)));
        p.select_next();
        assert_eq!(p.selected_action(), Some(PaletteAction::Theme(ThemeId::Blue)));
        p.insert(" whi");
        assert_eq!(p.selected_action(), Some(PaletteAction::Theme(ThemeId::White)));
        p.insert("zzz");
        assert_eq!(p.selected_action(), None);
        p.select_next();
        assert_eq!(p.selected, 0);
        p.clear();
        assert_eq!(p.filtered().len(), ITEMS.len());
    }

    #[test]
    fn renombrar_en_el_mismo_campo() {
        let mut p = PaletteState::new();
        p.insert("renom");
        assert_eq!(p.selected_action(), Some(PaletteAction::RenameSession));
        p.start_rename(7, "PowerShell");
        assert_eq!(p.query, "PowerShell");
        assert_eq!(p.rename_value().as_deref(), Some("PowerShell"));
        assert!(p.filtered().is_empty());
        // Lo primero que se escribe reemplaza el nombre actual.
        p.insert("  web\nsegunda línea");
        assert_eq!(p.rename_value().as_deref(), Some("web"));
        p.insert("-api");
        assert_eq!(p.rename_value().as_deref(), Some("web-api"));
        p.insert(&"x".repeat(100));
        assert_eq!(p.query.chars().count(), MAX_NAME);
    }

    #[test]
    fn borrar_el_nombre_propuesto_lo_vacia_de_una_vez() {
        let mut p = PaletteState::new();
        p.start_rename(1, "PowerShell 2");
        p.backspace();
        assert_eq!(p.rename_value(), None);
        p.insert("web");
        p.backspace();
        assert_eq!(p.query, "we");
    }
}
