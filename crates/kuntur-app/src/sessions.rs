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
