/// Una sesión de la barra lateral. `T` es lo que la UI guarda (la vista de terminal).
pub struct Entry<T> {
    pub id: u64,
    pub name: String,
    pub value: T,
}

/// Lista ordenada de sesiones con una activa. Sin GPUI: se prueba aparte.
pub struct SessionList<T> {
    items: Vec<Entry<T>>,
    active: usize,
    next_id: u64,
}

impl<T> SessionList<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            active: 0,
            next_id: 1,
        }
    }

    pub fn push(&mut self, name: impl Into<String>, value: T) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Entry {
            id,
            name: name.into(),
            value,
        });
        self.active = self.items.len() - 1;
        id
    }

    /// Quita la sesión; si era la activa, activa la de la izquierda (o la primera).
    pub fn close(&mut self, id: u64) -> Option<T> {
        let idx = self.items.iter().position(|e| e.id == id)?;
        let removed = self.items.remove(idx);
        if self.items.is_empty() {
            self.active = 0;
        } else if idx < self.active || (idx == self.active && idx > 0) {
            self.active -= 1;
        } else if self.active >= self.items.len() {
            self.active = self.items.len() - 1;
        }
        Some(removed.value)
    }

    pub fn activate(&mut self, id: u64) -> bool {
        match self.items.iter().position(|e| e.id == id) {
            Some(i) => {
                self.active = i;
                true
            }
            None => false,
        }
    }

    pub fn activate_index(&mut self, index: usize) -> bool {
        if index < self.items.len() {
            self.active = index;
            true
        } else {
            false
        }
    }

    pub fn next(&mut self) {
        if !self.items.is_empty() {
            self.active = (self.active + 1) % self.items.len();
        }
    }

    pub fn prev(&mut self) {
        if !self.items.is_empty() {
            self.active = (self.active + self.items.len() - 1) % self.items.len();
        }
    }

    pub fn rename(&mut self, id: u64, name: impl Into<String>) {
        if let Some(e) = self.items.iter_mut().find(|e| e.id == id) {
            e.name = name.into();
        }
    }

    pub fn active(&self) -> Option<&Entry<T>> {
        self.items.get(self.active)
    }

    pub fn active_id(&self) -> Option<u64> {
        self.active().map(|e| e.id)
    }

    pub fn get(&self, id: u64) -> Option<&Entry<T>> {
        self.items.iter().find(|e| e.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Entry<T>> {
        self.items.iter_mut().find(|e| e.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Entry<T>> {
        self.items.iter()
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "API de la lista; hoy solo la usan las pruebas")
    )]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T> Default for SessionList<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Nombres de shell que Kuntur pone solo (ajustes v0.2): una pestaña con uno de estos nombres (o
/// "nombre N") no fue renombrada a mano.
const SHELL_LABELS: [&str; 9] = [
    "PowerShell",
    "cmd",
    "bash",
    "zsh",
    "fish",
    "nu",
    "sh",
    "WSL",
    "Terminal",
];

/// Nombre de la pestaña según el shell (`None` = el predeterminado): "PowerShell", "cmd", "bash"…
/// En Linux/Mac el predeterminado sale de `$SHELL`; si no se reconoce, "Terminal".
pub fn shell_label(shell: Option<&str>) -> String {
    let Some(program) = shell.map(str::trim).filter(|s| !s.is_empty()) else {
        if cfg!(windows) {
            return "PowerShell".into();
        }
        return match std::env::var("SHELL") {
            Ok(s) if !s.trim().is_empty() => shell_label(Some(&s)),
            _ => "Terminal".into(),
        };
    };
    let file = program.trim_matches('"').rsplit(['\\', '/']).next().unwrap_or(program);
    let first = file.split_whitespace().next().unwrap_or(file);
    let stem = match first.len().checked_sub(4) {
        Some(i) if first.is_char_boundary(i) && first[i..].eq_ignore_ascii_case(".exe") => &first[..i],
        _ => first,
    };
    match stem.to_ascii_lowercase().as_str() {
        "pwsh" | "powershell" => "PowerShell".into(),
        "wsl" => "WSL".into(),
        "" => "Terminal".into(),
        "cmd" | "bash" | "zsh" | "fish" | "nu" | "sh" => stem.to_ascii_lowercase(),
        _ => stem.to_string(),
    }
}

/// ¿El nombre es uno de los que Kuntur pone solo? ("PowerShell", "bash 2"…). Sirve para los
/// `config.toml` anteriores a v0.2, que no guardaban si el nombre se puso a mano.
pub fn is_default_name(name: &str) -> bool {
    let base = match name.rsplit_once(' ') {
        Some((base, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => base,
        _ => name,
    };
    SHELL_LABELS.contains(&base)
}

/// Título que puso un agente (OSC 0/2) para mostrar en la pestaña: sin los adornos del principio
/// (spinners como ✳ ⠂ ⠐ ·) ni espacios de más, y con un tope de largo. `None` si no queda nada o si
/// es solo la ruta de un ejecutable (lo que pone el shell, no el agente).
pub fn agent_title(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let is_path = (raw.contains('\\') || raw.contains('/')) && raw.to_ascii_lowercase().ends_with(".exe");
    if is_path {
        return None;
    }
    let start = raw
        .char_indices()
        .find(|&(_, c)| c.is_alphanumeric() || "\"'¿¡([«".contains(c))
        .map(|(i, _)| i)?;
    let text: String = raw[start..].split_whitespace().collect::<Vec<_>>().join(" ");
    let text: String = text.chars().take(120).collect();
    (!text.is_empty()).then_some(text)
}

/// Nombre para una sesión nueva: "PowerShell", "PowerShell 2", … (el primero libre).
pub fn next_default_name(base: &str, existing: &[&str]) -> String {
    if !existing.contains(&base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|name| !existing.contains(&name.as_str()))
        .unwrap_or_else(|| base.to_string())
}

/// Título del programa para mostrar: si es la ruta de un ejecutable (lo que pone PowerShell al
/// abrir), solo su nombre sin `.exe`; si no, el texto tal cual.
pub fn short_title(raw: &str) -> String {
    let t = raw.trim();
    let last = t.rsplit(['\\', '/']).next().unwrap_or(t);
    if last.len() > 4 && last[last.len() - 4..].eq_ignore_ascii_case(".exe") && last.len() < t.len() {
        last[..last.len() - 4].to_string()
    } else {
        t.to_string()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombre_de_la_pestana_segun_el_shell() {
        assert_eq!(
            shell_label(Some(r"C:\Program Files\PowerShell\7\pwsh.exe")),
            "PowerShell"
        );
        assert_eq!(shell_label(Some("powershell.exe")), "PowerShell");
        assert_eq!(shell_label(Some("CMD.EXE")), "cmd");
        assert_eq!(shell_label(Some("/bin/zsh")), "zsh");
        assert_eq!(shell_label(Some("/usr/bin/fish -l")), "fish");
        assert_eq!(shell_label(Some("wsl.exe")), "WSL");
        assert_eq!(shell_label(Some("nushell-raro")), "nushell-raro");
        if cfg!(windows) {
            assert_eq!(shell_label(None), "PowerShell");
        }
    }

    #[test]
    fn nombres_puestos_por_kuntur_o_a_mano() {
        assert!(is_default_name("PowerShell"));
        assert!(is_default_name("PowerShell 12"));
        assert!(is_default_name("bash 2"));
        assert!(is_default_name("Terminal"));
        assert!(!is_default_name("web"));
        assert!(!is_default_name("PowerShell de pruebas"));
        assert!(!is_default_name("api 2"));
    }

    #[test]
    fn titulo_del_agente_sin_adornos() {
        assert_eq!(agent_title("✳ Arreglar el login").as_deref(), Some("Arreglar el login"));
        assert_eq!(
            agent_title("⠐ Revisando   pruebas  ").as_deref(),
            Some("Revisando pruebas")
        );
        assert_eq!(agent_title("· «Plan» de la API").as_deref(), Some("«Plan» de la API"));
        assert_eq!(agent_title("✳ Claude Code").as_deref(), Some("Claude Code"));
        assert_eq!(agent_title(r"C:\Program Files\PowerShell\7\pwsh.exe"), None);
        assert_eq!(agent_title("✳ ⠂ "), None);
        assert_eq!(agent_title(""), None);
        assert_eq!(agent_title(&"x".repeat(300)).map(|t| t.chars().count()), Some(120));
    }

    #[test]
    fn agregar_activa_la_nueva() {
        let mut s = SessionList::new();
        let a = s.push("uno", 'a');
        let b = s.push("dos", 'b');
        assert_eq!(s.active_id(), Some(b));
        assert_eq!(s.len(), 2);
        assert!(s.activate(a));
        assert_eq!(s.active().unwrap().value, 'a');
    }

    #[test]
    fn cerrar_la_activa_activa_la_vecina_izquierda() {
        let mut s = SessionList::new();
        let a = s.push("uno", 1);
        let b = s.push("dos", 2);
        let c = s.push("tres", 3);
        s.activate(b);
        assert_eq!(s.close(b), Some(2));
        assert_eq!(s.active_id(), Some(a));
        assert_eq!(s.close(a), Some(1));
        assert_eq!(s.active_id(), Some(c));
        assert_eq!(s.close(c), Some(3));
        assert!(s.is_empty());
        assert_eq!(s.active_id(), None);
    }

    #[test]
    fn siguiente_y_anterior_dan_la_vuelta() {
        let mut s = SessionList::new();
        let a = s.push("uno", ());
        let _b = s.push("dos", ());
        let c = s.push("tres", ());
        s.next();
        assert_eq!(s.active_id(), Some(a));
        s.prev();
        assert_eq!(s.active_id(), Some(c));
        assert!(s.activate_index(1));
        assert!(!s.activate_index(9));
    }

    #[test]
    fn nombres_por_defecto() {
        assert_eq!(next_default_name("PowerShell", &[]), "PowerShell");
        assert_eq!(next_default_name("PowerShell", &["PowerShell"]), "PowerShell 2");
        assert_eq!(
            next_default_name("PowerShell", &["PowerShell", "PowerShell 3"]),
            "PowerShell 2"
        );
        assert_eq!(
            next_default_name("PowerShell", &["PowerShell", "PowerShell 2"]),
            "PowerShell 3"
        );
    }

    #[test]
    fn titulo_corto() {
        assert_eq!(short_title(r"C:\Program Files\PowerShell\7\pwsh.exe"), "pwsh");
        assert_eq!(short_title("C:/tools/nvim.EXE"), "nvim");
        assert_eq!(short_title("  opencode  "), "opencode");
        assert_eq!(short_title("pwsh.exe"), "pwsh.exe");
        assert_eq!(short_title(r"Admin: C:\Windows\cmd.exe"), "cmd");
    }

    #[test]
    fn buscar_y_modificar_por_id() {
        let mut s = SessionList::new();
        let a = s.push("uno", 1);
        let b = s.push("dos", 2);
        assert_eq!(s.get(a).map(|e| e.value), Some(1));
        s.get_mut(a).unwrap().value = 10;
        assert_eq!(s.get(a).unwrap().value, 10);
        assert_eq!(s.get(b).unwrap().value, 2);
        assert!(s.get(99).is_none());
        assert!(s.get_mut(99).is_none());
    }

    #[test]
    fn renombrar() {
        let mut s = SessionList::new();
        let a = s.push("uno", ());
        s.rename(a, "web");
        assert_eq!(s.iter().next().unwrap().name, "web");
    }
}
