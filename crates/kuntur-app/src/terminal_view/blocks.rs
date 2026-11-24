//! Bloques (OSC 133): cada comando enviado desde el prompt, con su salida, se marca con una raya
//! fina en el margen izquierdo. Tenue si terminó bien; roja si terminó con error (color solo para
//! estado). No ocupa columnas de la terminal: se dibuja en el relleno izquierdo.

use gpui_kit::*;

use super::Metrics;

/// Ancho de la raya y separación desde el texto.
const BAR_W: f32 = 2.0;
const BAR_GAP: f32 = 7.0;

pub struct BlockColors {
    pub ok: Hsla,
    pub error: Hsla,
}

/// `blocks`: (fila inicial, fila final, código de salida) ya recortados a la pantalla.
pub(super) fn paint(
    blocks: &[(usize, usize, Option<i32>)],
    bounds: Bounds<Pixels>,
    m: Metrics,
    colors: &BlockColors,
    window: &mut Window,
) {
    for &(top, bottom, exit) in blocks {
        let color = match exit {
            Some(0) | None => colors.ok,
            Some(_) => colors.error,
        };
        let origin = point(
            bounds.origin.x - px(BAR_GAP),
            bounds.origin.y + m.line_h * top as f32 + px(2.),
        );
        let height = m.line_h * (bottom - top + 1) as f32 - px(4.);
        window.paint_quad(fill(Bounds::new(origin, size(px(BAR_W), height)), color));
    }
}
