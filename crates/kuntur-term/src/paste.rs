/// Prepara texto pegado para enviarlo al programa.
/// - Normaliza saltos de línea a `\r` (lo que envía la tecla Enter).
/// - Con bracketed paste (DEC 2004) lo envuelve en `ESC[200~ … ESC[201~`, para que
///   shells y agentes (Claude Code) lo reciban como un solo bloque y no lo ejecuten línea a línea.
/// - Quita cualquier `ESC[201~` dentro del texto: evita que un texto malicioso "cierre" el pegado.
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        let safe = normalized.replace("\x1b[201~", "");
        format!("\x1b[200~{safe}\x1b[201~").into_bytes()
    } else {
        normalized.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normaliza_saltos() {
        assert_eq!(paste_bytes("a\r\nb\nc", false), b"a\rb\rc".to_vec());
    }

    #[test]
    fn envuelve_en_bracketed_paste() {
        assert_eq!(paste_bytes("ls\n", true), b"\x1b[200~ls\r\x1b[201~".to_vec());
    }

    #[test]
    fn no_permite_cerrar_el_pegado_desde_el_texto() {
        assert_eq!(
            paste_bytes("x\x1b[201~rm -rf /", true),
            b"\x1b[200~xrm -rf /\x1b[201~".to_vec()
        );
    }
}
