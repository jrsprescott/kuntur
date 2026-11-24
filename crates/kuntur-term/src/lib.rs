//! Núcleo de terminal de Kuntur: PTY (ConPTY vía portable-pty) + emulador VT (alacritty_terminal).
//! No depende de la UI: la app dibuja a partir de [`Screen`].

pub mod agent_signal;
pub mod cwd;
pub mod emulator;
pub mod keys;
pub mod mouse;
pub mod osc;
pub mod palette;
pub mod pane;
pub mod paste;
pub mod screen;
pub mod size;

pub use agent_signal::{AgentSignal, SignalState, parse_7501};
pub use cwd::CwdScanner;
pub use emulator::{Emulator, EventSink, SelectionKind};
pub use keys::{Key, Mods, encode_key};
pub use mouse::{MouseAction, MouseButtonKind, MouseMode, MouseReport, MouseReporting, encode_mouse};
pub use osc::OscScanner;
pub use palette::{Palette, Rgb8};
pub use pane::{Pane, PaneOptions};
pub use paste::paste_bytes;
pub use screen::{CellView, Screen};
pub use size::GridSize;

// Tipos de alacritty que la UI necesita, sin depender de ese crate directamente.
pub use alacritty_terminal::index::Side;
pub use alacritty_terminal::term::TermMode;
