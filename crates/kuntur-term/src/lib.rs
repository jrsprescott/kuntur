//! Núcleo de terminal de Kuntur: PTY (ConPTY vía portable-pty) + emulador VT (alacritty_terminal).
//! No depende de la UI: la app dibuja a partir de [`Screen`].

pub mod size;

pub use size::GridSize;
