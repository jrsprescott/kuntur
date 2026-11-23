// Temporal: la UI (Task 6) todavía no usa la lista; quitar esta línea en Task 6.
#![allow(dead_code)]

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

    pub fn iter(&self) -> impl Iterator<Item = &Entry<T>> {
        self.items.iter()
    }

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
    fn renombrar() {
        let mut s = SessionList::new();
        let a = s.push("uno", ());
        s.rename(a, "web");
        assert_eq!(s.iter().next().unwrap().name, "web");
    }
}
