//! Navegación con teclado de los paneles (F11, accesibilidad): lógica pura, con pruebas.
//!
//! - Uso: ←/→ cambian el período; Ctrl+S exporta el CSV; Esc cierra.
//! - Auditoría: ←/→ período, ↑/↓ agente (Todos → Claude Code → Codex); Ctrl+S CSV; Esc limpia la
//!   búsqueda y, si ya está vacía, cierra.
//! - Conexiones: ↑/↓ eligen, Enter conecta, F2 edita, Supr pide confirmar el borrado (Enter = sí,
//!   Esc = no), Ctrl+N nueva; Esc cierra.

/// Elemento siguiente (`forward`) o anterior de `all`, con vuelta al principio/final. Si `current`
/// no está en la lista devuelve el primero.
pub fn cycle<T: Copy + PartialEq>(all: &[T], current: T, forward: bool) -> T {
    let Some(i) = all.iter().position(|&x| x == current) else {
        return all[0];
    };
    let n = all.len();
    all[if forward { (i + 1) % n } else { (i + n - 1) % n }]
}

/// Fila elegida tras ↑ (`down = false`) o ↓ en una lista de `len` filas, sin dar la vuelta.
pub fn step(selected: usize, len: usize, down: bool) -> usize {
    if len == 0 {
        return 0;
    }
    let selected = selected.min(len - 1);
    if down {
        (selected + 1).min(len - 1)
    } else {
        selected.saturating_sub(1)
    }
}

/// Dirección de ←/→ (o `None` si la tecla es otra). Solo sin modificadores.
pub fn horizontal(key: &str, modified: bool) -> Option<bool> {
    match (key, modified) {
        ("right", false) => Some(true),
        ("left", false) => Some(false),
        _ => None,
    }
}

/// Dirección de ↑/↓ (`true` = abajo) o `None`. Solo sin modificadores.
pub fn vertical(key: &str, modified: bool) -> Option<bool> {
    match (key, modified) {
        ("down", false) => Some(true),
        ("up", false) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{cycle, horizontal, step, vertical};

    #[test]
    fn cycle_da_la_vuelta_en_los_dos_sentidos() {
        let all = [1, 2, 3];
        assert_eq!(cycle(&all, 1, true), 2);
        assert_eq!(cycle(&all, 3, true), 1);
        assert_eq!(cycle(&all, 1, false), 3);
        assert_eq!(cycle(&all, 2, false), 1);
        // Desconocido: el primero.
        assert_eq!(cycle(&all, 9, true), 1);
        let agentes = [None, Some('c'), Some('x')];
        assert_eq!(cycle(&agentes, None, true), Some('c'));
        assert_eq!(cycle(&agentes, None, false), Some('x'));
    }

    #[test]
    fn step_se_queda_en_los_bordes() {
        assert_eq!(step(0, 3, false), 0);
        assert_eq!(step(0, 3, true), 1);
        assert_eq!(step(2, 3, true), 2);
        assert_eq!(step(2, 3, false), 1);
        // La lista se achicó (se borró una conexión): vuelve dentro.
        assert_eq!(step(7, 3, false), 1);
        assert_eq!(step(7, 3, true), 2);
        assert_eq!(step(4, 0, true), 0);
    }

    #[test]
    fn flechas_sin_modificadores() {
        assert_eq!(horizontal("right", false), Some(true));
        assert_eq!(horizontal("left", false), Some(false));
        assert_eq!(horizontal("left", true), None);
        assert_eq!(horizontal("up", false), None);
        assert_eq!(vertical("down", false), Some(true));
        assert_eq!(vertical("up", false), Some(false));
        assert_eq!(vertical("up", true), None);
        assert_eq!(vertical("a", false), None);
    }
}
