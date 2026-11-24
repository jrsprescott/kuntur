//! Paleta de comandos (Ctrl+Shift+P): lista de acciones, filtro y estado. Sin GPUI: se prueba
//! aparte; el `Workspace` la dibuja y ejecuta la acción elegida.

use std::borrow::Cow;

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
    ToggleDock,
    Theme(ThemeId),
    RailFull,
    RailIcons,
    PanelAgents,
    PanelFiles,
    FontBigger,
    FontSmaller,
    FontReset,
    Usage,
    ExportUsage,
    EditPrices,
    /// Vuelve a leer los agentes extra de `config.toml` y busca los instalados.
    RefreshAgents,
    Audit,
    ExportAudit,
    /// Abre el agente n de `PaletteState::agents` en la sesión activa.
    LaunchAgent(usize),
    /// Abre el agente n en un `git worktree` nuevo, en una sesión nueva (F5b).
    LaunchInWorktree(usize),
    /// Pega la selección de la terminal principal en la acoplada, sin Enter (F5b).
    SendSelectionToDock,
    /// Pega el último comando que sugirió el agente de la sesión en la acoplada, sin Enter (F5b).
    SendSuggestion,
    /// Muestra qué se agregará a `~/.claude/settings.json` y lo instala si el usuario confirma (F5b).
    InstallClaudeHooks,
    Connections,
    NewConnection,
    Reconnect,
    /// Gestor de MCP de la sesión (F10).
    Mcp,
    /// Panel "Configuración" (ajustes v0.2).
    Settings,
}

#[derive(Clone, Debug)]
pub struct PaletteItem {
    pub action: PaletteAction,
    pub label: Cow<'static, str>,
    pub shortcut: Option<&'static str>,
}

const fn item(action: PaletteAction, label: &'static str, shortcut: Option<&'static str>) -> PaletteItem {
    PaletteItem {
        action,
        label: Cow::Borrowed(label),
        shortcut,
    }
}

/// Las acciones fijas, en el orden en que se muestran (después van las de abrir cada agente instalado).
pub static ITEMS: [PaletteItem; 30] = [
    item(PaletteAction::NewSession, "Nueva sesión", Some("Ctrl+Shift+T")),
    item(PaletteAction::Settings, "Configuración", Some("Ctrl+,")),
    item(PaletteAction::RenameSession, "Renombrar sesión", None),
    item(PaletteAction::CloseSession, "Cerrar sesión", Some("Ctrl+Shift+W")),
    item(PaletteAction::NextSession, "Siguiente sesión", Some("Ctrl+Tab")),
    item(PaletteAction::PrevSession, "Sesión anterior", Some("Ctrl+Shift+Tab")),
    item(
        PaletteAction::ToggleDock,
        "Terminal acoplada: mostrar/ocultar",
        Some("Ctrl+`"),
    ),
    item(PaletteAction::Theme(ThemeId::Blue), "Tema: Blue", None),
    item(PaletteAction::Theme(ThemeId::Dark), "Tema: Dark", None),
    item(PaletteAction::Theme(ThemeId::White), "Tema: White", None),
    item(PaletteAction::PanelAgents, "Panel: agentes", None),
    item(PaletteAction::PanelFiles, "Panel: archivos", Some("Ctrl+Shift+E")),
    item(PaletteAction::RailFull, "Panel: completo", Some("Ctrl+Shift+B")),
    item(PaletteAction::RailIcons, "Panel: compacto", Some("Ctrl+Shift+B")),
    item(PaletteAction::FontBigger, "Fuente: más grande", Some("Ctrl+=")),
    item(PaletteAction::FontSmaller, "Fuente: más chica", Some("Ctrl+-")),
    item(PaletteAction::FontReset, "Fuente: restablecer", Some("Ctrl+0")),
    item(PaletteAction::Usage, "Uso de agentes", Some("Ctrl+Shift+U")),
    item(PaletteAction::ExportUsage, "Uso de agentes: exportar CSV", None),
    item(PaletteAction::EditPrices, "Uso de agentes: editar precios", None),
    item(PaletteAction::Audit, "Auditoría de comandos", Some("Ctrl+Shift+A")),
    item(PaletteAction::ExportAudit, "Auditoría: exportar CSV", None),
    item(PaletteAction::RefreshAgents, "Agentes: actualizar instalados", None),
    item(
        PaletteAction::SendSelectionToDock,
        "Terminal: enviar selección a la acoplada",
        None,
    ),
    item(
        PaletteAction::SendSuggestion,
        "Terminal: enviar el comando sugerido a la acoplada",
        None,
    ),
    item(
        PaletteAction::InstallClaudeHooks,
        "Claude Code: instalar ganchos de Kuntur",
        None,
    ),
    item(PaletteAction::Connections, "Conexiones SSH", Some("Ctrl+Shift+O")),
    item(PaletteAction::NewConnection, "Conexiones SSH: nueva", None),
    item(
        PaletteAction::Reconnect,
        "Sesión remota: reconectar",
        Some("Ctrl+Shift+R"),
    ),
    item(
        PaletteAction::Mcp,
        "MCP: servidores, perfiles y comandos de la sesión",
        Some("Ctrl+Shift+M"),
    ),
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
    /// Ids de los agentes instalados cuando se abrió la paleta ("Agentes: abrir …").
    pub agents: Vec<String>,
    /// Acciones de abrir cada agente.
    extra: Vec<PaletteItem>,
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
            agents: Vec::new(),
            extra: Vec::new(),
        }
    }

    /// Paleta con "Agentes: abrir <nombre>" (y "… en un worktree nuevo") para cada agente instalado
    /// (`(id, nombre)`).
    pub fn with_agents(agents: Vec<(String, String)>) -> Self {
        let mut p = Self::new();
        let launch = agents.iter().enumerate().map(|(i, (_, name))| PaletteItem {
            action: PaletteAction::LaunchAgent(i),
            label: Cow::Owned(format!("Agentes: abrir {name}")),
            shortcut: None,
        });
        let worktree = agents.iter().enumerate().map(|(i, (_, name))| PaletteItem {
            action: PaletteAction::LaunchInWorktree(i),
            label: Cow::Owned(format!("Agentes: abrir {name} en un worktree nuevo")),
            shortcut: None,
        });
        p.extra = launch.chain(worktree).collect();
        p.agents = agents.into_iter().map(|(id, _)| id).collect();
        p
    }

    /// Acciones que coinciden con lo escrito (vacío en modo renombrar).
    pub fn filtered(&self) -> Vec<&PaletteItem> {
        match self.mode {
            PaletteMode::Search => ITEMS
                .iter()
                .chain(&self.extra)
                .filter(|i| matches(&self.query, &i.label))
                .collect(),
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
    fn la_terminal_acoplada_se_encuentra() {
        let mut p = PaletteState::new();
        p.insert("acoplada");
        assert_eq!(p.selected_action(), Some(PaletteAction::ToggleDock));
        p.clear();
        p.insert("terminal ocultar");
        assert_eq!(p.selected_action(), Some(PaletteAction::ToggleDock));
    }

    #[test]
    fn abrir_agentes_instalados() {
        let mut p = PaletteState::with_agents(vec![
            ("claude".into(), "Claude Code".into()),
            ("extra:aider".into(), "Aider".into()),
        ]);
        assert_eq!(p.filtered().len(), ITEMS.len() + 4);
        p.insert("abrir aider");
        assert_eq!(p.selected_action(), Some(PaletteAction::LaunchAgent(1)));
        p.clear();
        p.insert("worktree claude");
        assert_eq!(p.selected_action(), Some(PaletteAction::LaunchInWorktree(0)));
        p.clear();
        p.insert("ganchos");
        assert_eq!(p.selected_action(), Some(PaletteAction::InstallClaudeHooks));
        p.clear();
        p.insert("enviar seleccion");
        assert_eq!(p.selected_action(), Some(PaletteAction::SendSelectionToDock));
        assert_eq!(p.agents[1], "extra:aider");
        p.clear();
        p.insert("agentes actualizar");
        assert_eq!(p.selected_action(), Some(PaletteAction::RefreshAgents));
        p.clear();
        p.insert("auditoria");
        assert_eq!(p.selected_action(), Some(PaletteAction::Audit));
    }

    #[test]
    fn conexiones_y_reconectar_se_encuentran() {
        let mut p = PaletteState::new();
        p.insert("ssh");
        assert_eq!(p.selected_action(), Some(PaletteAction::Connections));
        p.clear();
        p.insert("nueva conexion");
        assert_eq!(p.selected_action(), Some(PaletteAction::NewConnection));
        p.clear();
        p.insert("reconectar");
        assert_eq!(p.selected_action(), Some(PaletteAction::Reconnect));
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
