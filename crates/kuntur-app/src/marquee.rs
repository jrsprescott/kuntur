//! Marquesina del título de la pestaña activa (ajustes v0.2): si el título no entra, se desplaza de
//! izquierda a derecha y vuelve, con una pausa en cada extremo. Lógica pura (sin GPUI): la vista
//! llama `tick` cada [`TICK_MS`] y dibuja el texto corrido `offset` píxeles a la izquierda.

/// Intervalo entre pasos (≈12 cuadros por segundo: suave y barato).
pub const TICK_MS: u64 = 80;
/// Píxeles por paso.
const STEP: f32 = 1.5;
/// Pasos de pausa en cada extremo (≈1,4 s).
const HOLD: u32 = 18;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    HoldStart,
    Forward,
    HoldEnd,
    Back,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Marquee {
    offset: f32,
    phase: Phase,
    held: u32,
}

impl Marquee {
    /// Cuánto se corre el texto a la izquierda (0 = al principio).
    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// Vuelve al principio (título nuevo, otra pestaña).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Un paso. `overflow` = ancho del texto menos el ancho visible (≤ 0: entra y no se mueve).
    pub fn tick(&mut self, overflow: f32) {
        if overflow <= 0.0 {
            self.reset();
            return;
        }
        match self.phase {
            Phase::HoldStart => {
                self.held += 1;
                if self.held >= HOLD {
                    self.phase = Phase::Forward;
                    self.held = 0;
                }
            }
            Phase::Forward => {
                self.offset = (self.offset + STEP).min(overflow);
                if self.offset >= overflow {
                    self.phase = Phase::HoldEnd;
                }
            }
            Phase::HoldEnd => {
                self.held += 1;
                if self.held >= HOLD {
                    self.phase = Phase::Back;
                    self.held = 0;
                }
            }
            Phase::Back => {
                self.offset = (self.offset - STEP).max(0.0);
                if self.offset <= 0.0 {
                    self.phase = Phase::HoldStart;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HOLD, Marquee, STEP};

    #[test]
    fn pausa_avanza_hasta_el_final_pausa_y_vuelve() {
        let mut m = Marquee::default();
        let overflow = 6.0;
        for _ in 0..HOLD {
            m.tick(overflow);
            assert_eq!(m.offset(), 0.0, "primero se queda quieto");
        }
        m.tick(overflow);
        assert_eq!(m.offset(), STEP);
        for _ in 0..10 {
            m.tick(overflow);
        }
        assert_eq!(m.offset(), overflow, "no pasa del final");
        for _ in 0..HOLD {
            m.tick(overflow);
        }
        m.tick(overflow);
        assert!(m.offset() < overflow, "vuelve");
        for _ in 0..10 {
            m.tick(overflow);
        }
        assert_eq!(m.offset(), 0.0, "vuelve hasta el principio");
    }

    #[test]
    fn si_el_texto_entra_no_se_mueve() {
        let mut m = Marquee::default();
        for _ in 0..100 {
            m.tick(0.0);
        }
        assert_eq!(m.offset(), 0.0);
        // Si el título cambia a uno que entra, vuelve al principio.
        let mut m = Marquee::default();
        for _ in 0..(HOLD + 3) {
            m.tick(50.0);
        }
        assert!(m.offset() > 0.0);
        m.tick(-1.0);
        assert_eq!(m.offset(), 0.0);
    }
}
