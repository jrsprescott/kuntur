//! Integración de shell (OSC 133, "FinalTerm"): el shell marca dónde empieza el prompt (`A`), dónde
//! empieza lo que escribe el usuario (`B`), cuándo empieza un comando (`C`) y cuándo termina con su
//! código de salida (`D;n`). Con eso Kuntur sabe si está en el prompt y qué lleva escrito.
//!
//! PowerShell no tiene un gancho "antes de ejecutar" que no toque PSReadLine, así que `C` lo pone
//! Kuntur al enviar Enter estando en el prompt ([`ShellState::command_started`]).

/// Una marca OSC 133.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellMark {
    /// `A`: empieza el prompt.
    PromptStart,
    /// `B`: termina el prompt; desde aquí escribe el usuario.
    InputStart,
    /// `C`: empieza la salida del comando.
    CommandStart,
    /// `D[;código]`: terminó el comando.
    CommandEnd(Option<i32>),
}

impl ShellMark {
    /// Contenido después de `133;` (`A`, `B`, `C`, `D;0`, `A;cl=m`…).
    pub fn parse(payload: &str) -> Option<ShellMark> {
        let mut parts = payload.split(';');
        let mark = match parts.next()? {
            "A" => ShellMark::PromptStart,
            "B" => ShellMark::InputStart,
            "C" => ShellMark::CommandStart,
            "D" => ShellMark::CommandEnd(parts.next().and_then(|c| c.trim().parse().ok())),
            _ => return None,
        };
        Some(mark)
    }
}

/// En qué está el shell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShellPhase {
    /// El shell no informa marcas (cmd, bash sin integración…): no hay autocompletado.
    #[default]
    Unknown,
    /// Dibujando el prompt.
    Prompt,
    /// En el prompt, esperando lo que escribe el usuario.
    Input,
    /// Corriendo un comando (o un programa como `claude`/`vim`).
    Running,
}

/// Punto de la cuadrícula en coordenadas absolutas: la línea cuenta desde el inicio del historial,
/// así no cambia cuando la pantalla se desplaza.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AbsPoint {
    pub line: i64,
    pub col: usize,
}

/// Un comando terminado: desde la línea de su prompt hasta la última de su salida.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub start: i64,
    pub end: i64,
    pub exit: Option<i32>,
}

/// Bloques recordados como máximo (los más viejos se olvidan).
pub const MAX_BLOCKS: usize = 500;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellState {
    pub phase: ShellPhase,
    /// Dónde empieza la entrada (marca `B`) mientras se está en el prompt.
    pub input_start: Option<AbsPoint>,
    /// Dónde empezó la salida del último comando (marca `C`, o Enter en el prompt).
    pub output_start: Option<AbsPoint>,
    /// Código de salida del último comando (marca `D`).
    pub last_exit: Option<i32>,
    /// Comandos terminados (cambia con cada `D`).
    pub finished: u64,
    /// Bloques (comando + salida + código), del más viejo al más nuevo.
    pub blocks: Vec<Block>,
    /// Línea del último prompt (marca `A`).
    prompt_line: Option<i64>,
    /// Línea del prompt del comando que está corriendo.
    running_from: Option<i64>,
}

impl ShellState {
    /// Aplica una marca; `at` es la posición del cursor justo después de ella.
    pub fn apply(&mut self, mark: ShellMark, at: AbsPoint) {
        match mark {
            ShellMark::PromptStart => {
                // Un prompt más arriba que el anterior: se limpió la pantalla (`cls`) y las
                // líneas guardadas ya no apuntan a nada.
                if self.prompt_line.is_some_and(|l| at.line < l) {
                    self.blocks.clear();
                    self.running_from = None;
                }
                self.phase = ShellPhase::Prompt;
                self.input_start = None;
                self.prompt_line = Some(at.line);
            }
            ShellMark::InputStart => {
                self.phase = ShellPhase::Input;
                self.input_start = Some(at);
            }
            ShellMark::CommandStart => self.command_started(at),
            ShellMark::CommandEnd(code) => {
                self.last_exit = code;
                self.finished += 1;
                self.phase = ShellPhase::Prompt;
                self.input_start = None;
                if let Some(start) = self.running_from.take() {
                    // `D` llega al empezar el prompt siguiente: el bloque termina en la línea anterior.
                    let end = if at.col == 0 { at.line - 1 } else { at.line };
                    if end >= start {
                        self.blocks.push(Block { start, end, exit: code });
                        if self.blocks.len() > MAX_BLOCKS {
                            self.blocks.remove(0);
                        }
                    }
                }
            }
        }
    }

    /// El usuario envió la línea (Enter en el prompt) o el shell marcó `C`.
    pub fn command_started(&mut self, at: AbsPoint) {
        if self.phase == ShellPhase::Unknown {
            return;
        }
        if self.phase == ShellPhase::Input {
            self.running_from = self.prompt_line;
        }
        self.phase = ShellPhase::Running;
        self.input_start = None;
        self.output_start = Some(at);
    }

    /// ¿Está en el prompt esperando lo que escribe el usuario?
    pub fn at_input(&self) -> Option<AbsPoint> {
        (self.phase == ShellPhase::Input).then_some(self.input_start).flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(line: i64, col: usize) -> AbsPoint {
        AbsPoint { line, col }
    }

    #[test]
    fn lee_las_marcas() {
        assert_eq!(ShellMark::parse("A"), Some(ShellMark::PromptStart));
        assert_eq!(ShellMark::parse("A;cl=m;aid=1"), Some(ShellMark::PromptStart));
        assert_eq!(ShellMark::parse("B"), Some(ShellMark::InputStart));
        assert_eq!(ShellMark::parse("C"), Some(ShellMark::CommandStart));
        assert_eq!(ShellMark::parse("D"), Some(ShellMark::CommandEnd(None)));
        assert_eq!(ShellMark::parse("D;0"), Some(ShellMark::CommandEnd(Some(0))));
        assert_eq!(ShellMark::parse("D;-1"), Some(ShellMark::CommandEnd(Some(-1))));
        assert_eq!(ShellMark::parse("Z"), None);
    }

    #[test]
    fn ciclo_de_un_comando() {
        let mut s = ShellState::default();
        assert_eq!(s.at_input(), None);
        // Sin marcas, Enter no inventa fases (cmd.exe, bash sin integración).
        s.command_started(p(0, 0));
        assert_eq!(s.phase, ShellPhase::Unknown);

        s.apply(ShellMark::PromptStart, p(0, 0));
        assert_eq!(s.phase, ShellPhase::Prompt);
        s.apply(ShellMark::InputStart, p(0, 7));
        assert_eq!(s.at_input(), Some(p(0, 7)));
        s.command_started(p(1, 0));
        assert_eq!(s.phase, ShellPhase::Running);
        assert_eq!(s.at_input(), None);
        assert_eq!(s.output_start, Some(p(1, 0)));
        s.apply(ShellMark::CommandEnd(Some(3)), p(5, 0));
        assert_eq!((s.last_exit, s.finished), (Some(3), 1));
        s.apply(ShellMark::PromptStart, p(5, 0));
        s.apply(ShellMark::InputStart, p(5, 7));
        assert_eq!(s.at_input(), Some(p(5, 7)));
        // Bloque: del prompt (línea 0) a la última línea de salida (4).
        assert_eq!(
            s.blocks,
            vec![Block {
                start: 0,
                end: 4,
                exit: Some(3)
            }]
        );
        // Enter en una línea vacía (sin `D` del shell) no deja bloque; `cls` los borra.
        s.apply(ShellMark::PromptStart, p(0, 0));
        assert!(s.blocks.is_empty());
    }
}
