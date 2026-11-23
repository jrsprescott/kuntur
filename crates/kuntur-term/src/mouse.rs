use crate::keys::Mods;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButtonKind {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Move,
}

/// Evento de mouse en coordenadas de celda (0-based) para enviar al programa.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseReport {
    pub button: MouseButtonKind,
    pub action: MouseAction,
    pub col: usize,
    pub row: usize,
    pub mods: Mods,
}

/// Qué pidió el programa (DECSET 1000/1002/1003) y si usa el formato SGR (1006).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseReporting {
    Off,
    Click,
    Drag,
    Motion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseMode {
    pub report: MouseReporting,
    pub sgr: bool,
}

/// Codifica el evento como xterm. `sgr = true` → `ESC[<b;x;yM|m`; si no, X10 (`ESC[M` + 3 bytes).
pub fn encode_mouse(ev: &MouseReport, sgr: bool) -> Option<Vec<u8>> {
    let mut code: u32 = match ev.button {
        MouseButtonKind::Left => 0,
        MouseButtonKind::Middle => 1,
        MouseButtonKind::Right => 2,
        MouseButtonKind::WheelUp => 64,
        MouseButtonKind::WheelDown => 65,
    };
    if ev.action == MouseAction::Move {
        code += 32;
    }
    if ev.mods.shift {
        code += 4;
    }
    if ev.mods.alt {
        code += 8;
    }
    if ev.mods.ctrl {
        code += 16;
    }
    let (x, y) = (ev.col + 1, ev.row + 1);
    if sgr {
        let end = if ev.action == MouseAction::Release { 'm' } else { 'M' };
        return Some(format!("\x1b[<{code};{x};{y}{end}").into_bytes());
    }
    if x > 223 || y > 223 {
        return None;
    }
    if ev.action == MouseAction::Release {
        code = (code & !0b11) | 3;
    }
    Some(vec![0x1b, b'[', b'M', 32 + code as u8, 32 + x as u8, 32 + y as u8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Mods;

    fn ev(button: MouseButtonKind, action: MouseAction, col: usize, row: usize) -> MouseReport {
        MouseReport {
            button,
            action,
            col,
            row,
            mods: Mods::NONE,
        }
    }

    #[test]
    fn sgr_clic_y_soltar() {
        assert_eq!(
            encode_mouse(&ev(MouseButtonKind::Left, MouseAction::Press, 0, 0), true),
            Some(b"\x1b[<0;1;1M".to_vec())
        );
        assert_eq!(
            encode_mouse(&ev(MouseButtonKind::Left, MouseAction::Release, 9, 4), true),
            Some(b"\x1b[<0;10;5m".to_vec())
        );
        assert_eq!(
            encode_mouse(&ev(MouseButtonKind::Right, MouseAction::Press, 2, 3), true),
            Some(b"\x1b[<2;3;4M".to_vec())
        );
    }

    #[test]
    fn sgr_rueda_arrastre_y_modificadores() {
        assert_eq!(
            encode_mouse(&ev(MouseButtonKind::WheelUp, MouseAction::Press, 0, 0), true),
            Some(b"\x1b[<64;1;1M".to_vec())
        );
        assert_eq!(
            encode_mouse(&ev(MouseButtonKind::WheelDown, MouseAction::Press, 0, 0), true),
            Some(b"\x1b[<65;1;1M".to_vec())
        );
        assert_eq!(
            encode_mouse(&ev(MouseButtonKind::Left, MouseAction::Move, 1, 1), true),
            Some(b"\x1b[<32;2;2M".to_vec())
        );
        let mut e = ev(MouseButtonKind::Left, MouseAction::Press, 0, 0);
        e.mods = Mods {
            ctrl: true,
            alt: false,
            shift: true,
        };
        assert_eq!(encode_mouse(&e, true), Some(b"\x1b[<20;1;1M".to_vec()));
    }

    #[test]
    fn x10_clasico_y_su_limite() {
        assert_eq!(
            encode_mouse(&ev(MouseButtonKind::Left, MouseAction::Press, 0, 0), false),
            Some(vec![0x1b, b'[', b'M', 32, 33, 33])
        );
        assert_eq!(
            encode_mouse(&ev(MouseButtonKind::Left, MouseAction::Release, 0, 0), false),
            Some(vec![0x1b, b'[', b'M', 35, 33, 33])
        );
        assert_eq!(
            encode_mouse(&ev(MouseButtonKind::Left, MouseAction::Press, 300, 0), false),
            None
        );
    }
}
