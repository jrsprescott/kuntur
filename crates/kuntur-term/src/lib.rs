//! Núcleo de terminal de Kuntur: PTY (ConPTY vía portable-pty) + emulador VT (alacritty_terminal).
//! No depende de la UI: la app dibuja a partir de [`Screen`].

pub mod keys;
pub mod paste;
pub mod size;

pub use keys::{Key, Mods, encode_key};
pub use paste::paste_bytes;
pub use size::GridSize;
