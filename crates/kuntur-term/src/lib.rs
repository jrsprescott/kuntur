//! Núcleo de terminal de Kuntur: PTY (ConPTY vía portable-pty) + emulador VT (alacritty_terminal).
//! No depende de la UI: la app dibuja a partir de [`Screen`].

pub mod paste;
pub mod size;

pub use paste::paste_bytes;
pub use size::GridSize;
