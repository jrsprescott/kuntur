//! Núcleo de terminal de Kuntur: PTY (ConPTY vía portable-pty) + emulador VT (alacritty_terminal).
//! No depende de la UI: la app dibuja a partir de [`Screen`].

pub mod agent_signal;
pub mod cwd;
pub mod emulator;
pub mod keys;
pub mod links;
pub mod mouse;
pub mod osc;
pub mod palette;
pub mod pane;
pub mod paste;
pub mod screen;
pub mod shell;
pub mod size;
pub mod transport;

pub use agent_signal::{AgentSignal, SignalState, parse_7501};
pub use cwd::{CwdScanner, OscEvent};
pub use emulator::{Emulator, EventSink, InputLine, SelectionKind};
pub use keys::{Key, Mods, encode_key};
pub use links::{Link, LinkKind, LinkSpan, link_at, links_in_row};
pub use mouse::{MouseAction, MouseButtonKind, MouseMode, MouseReport, MouseReporting, encode_mouse};
pub use osc::OscScanner;
pub use palette::{Palette, Rgb8};
pub use pane::{Pane, PaneOptions};
pub use paste::paste_bytes;
pub use screen::{CellView, Screen};
pub use shell::{AbsPoint, Block, ShellMark, ShellPhase, ShellState};
pub use size::GridSize;
pub use transport::{LocalPty, Transport};

// Tipos de alacritty que la UI necesita, sin depender de ese crate directamente.
pub use alacritty_terminal::index::Side;
pub use alacritty_terminal::term::TermMode;
