//! La "consola" de una terminal remota: por dónde Kuntur escribe sus mensajes (conectando, errores,
//! preguntas) en la misma pantalla que el servidor, y hacia dónde van las teclas según el momento
//! (al servidor, a una pregunta, o a "Enter reconecta").

use std::io::Read;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};

use crate::prompt::{LineEditor, LineEvent};

/// Órdenes para la tarea que atiende el canal SSH.
#[derive(Debug)]
pub(crate) enum ChannelCmd {
    Write(Vec<u8>),
    Resize(u32, u32),
    Close,
}

/// Adónde van las teclas.
pub(crate) enum Input {
    /// Conectando: se ignoran.
    Idle,
    /// Kuntur está preguntando algo (huella, passphrase, contraseña).
    Prompt {
        editor: LineEditor,
        reply: Option<oneshot::Sender<Option<String>>>,
    },
    /// Conectado: al servidor.
    Channel(mpsc::UnboundedSender<ChannelCmd>),
    /// Sin conexión: Enter reconecta.
    Disconnected,
}

/// Qué hacer con lo que el usuario escribió.
pub(crate) enum Routed {
    Nothing,
    Reconnect,
}

#[derive(Clone)]
pub(crate) struct Console {
    inner: Arc<Inner>,
}

struct Inner {
    out: Mutex<Option<Sender<Vec<u8>>>>,
    input: Mutex<Input>,
}

pub(crate) const RED: &str = "\x1b[31m";
pub(crate) const YELLOW: &str = "\x1b[33m";
pub(crate) const DIM: &str = "\x1b[2m";
pub(crate) const BOLD: &str = "\x1b[1m";
pub(crate) const RESET: &str = "\x1b[0m";

impl Console {
    /// Consola nueva y el lector que alimenta al emulador.
    pub(crate) fn new() -> (Console, OutputReader) {
        let (tx, rx) = std::sync::mpsc::channel();
        let console = Console {
            inner: Arc::new(Inner {
                out: Mutex::new(Some(tx)),
                input: Mutex::new(Input::Idle),
            }),
        };
        (console, OutputReader::new(rx))
    }

    /// Bytes del servidor (tal cual).
    pub(crate) fn output(&self, bytes: Vec<u8>) {
        if let Some(tx) = self.inner.out.lock().unwrap().as_ref() {
            let _ = tx.send(bytes);
        }
    }

    /// Texto de Kuntur: los `\n` pasan a `\r\n`.
    pub(crate) fn print(&self, text: &str) {
        self.output(text.replace("\r\n", "\n").replace('\n', "\r\n").into_bytes());
    }

    /// Fin de la salida: el emulador ve el fin del flujo y la pestaña se cierra.
    pub(crate) fn close_output(&self) {
        self.inner.out.lock().unwrap().take();
    }

    pub(crate) fn set_input(&self, input: Input) {
        let old = std::mem::replace(&mut *self.inner.input.lock().unwrap(), input);
        // Una pregunta pendiente que se reemplaza queda sin respuesta.
        if let Input::Prompt { reply: Some(tx), .. } = old {
            let _ = tx.send(None);
        }
    }

    pub(crate) fn is_disconnected(&self) -> bool {
        matches!(*self.inner.input.lock().unwrap(), Input::Disconnected)
    }

    pub(crate) fn is_connected(&self) -> bool {
        matches!(*self.inner.input.lock().unwrap(), Input::Channel(_))
    }

    pub(crate) fn is_prompting(&self) -> bool {
        matches!(*self.inner.input.lock().unwrap(), Input::Prompt { .. })
    }

    /// Envía una orden al canal si está conectado.
    pub(crate) fn send(&self, cmd: ChannelCmd) -> bool {
        match &*self.inner.input.lock().unwrap() {
            Input::Channel(tx) => tx.send(cmd).is_ok(),
            _ => false,
        }
    }

    /// Pregunta en la terminal y espera la respuesta (`None` = cancelada o la terminal se cerró).
    pub(crate) async fn ask(&self, question: &str, secret: bool) -> Option<String> {
        let (tx, rx) = oneshot::channel();
        self.set_input(Input::Prompt {
            editor: LineEditor::new(secret),
            reply: Some(tx),
        });
        self.print(question);
        let answer = rx.await.ok().flatten();
        self.set_input(Input::Idle);
        answer
    }

    /// Teclas del usuario (o respuestas del emulador).
    pub(crate) fn route(&self, bytes: &[u8]) -> Routed {
        let mut echo = Vec::new();
        let routed = {
            let mut input = self.inner.input.lock().unwrap();
            match &mut *input {
                Input::Idle => Routed::Nothing,
                Input::Channel(tx) => {
                    let _ = tx.send(ChannelCmd::Write(bytes.to_vec()));
                    Routed::Nothing
                }
                Input::Prompt { editor, reply } => {
                    let (e, event) = editor.feed(bytes);
                    echo = e;
                    if let Some(event) = event
                        && let Some(tx) = reply.take()
                    {
                        let _ = tx.send(match event {
                            LineEvent::Submit(s) => Some(s),
                            LineEvent::Cancel => None,
                        });
                    }
                    Routed::Nothing
                }
                Input::Disconnected => {
                    // Solo una tecla Enter real (no respuestas del emulador que empiezan con ESC).
                    if bytes.first() != Some(&0x1b) && bytes.contains(&b'\r') {
                        *input = Input::Idle;
                        Routed::Reconnect
                    } else {
                        Routed::Nothing
                    }
                }
            }
        };
        if !echo.is_empty() {
            self.output(echo);
        }
        routed
    }
}

/// Lado de lectura para el emulador (`Pane::with_transport`): fin = la sesión terminó.
pub struct OutputReader {
    rx: Receiver<Vec<u8>>,
    pending: Vec<u8>,
    pos: usize,
}

impl OutputReader {
    fn new(rx: Receiver<Vec<u8>>) -> Self {
        Self {
            rx,
            pending: Vec::new(),
            pos: 0,
        }
    }
}

impl Read for OutputReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.pending.len() {
            match self.rx.recv() {
                Ok(chunk) => {
                    self.pending = chunk;
                    self.pos = 0;
                }
                Err(_) => return Ok(0),
            }
            // Juntar lo que ya llegó: menos vueltas del emulador.
            while self.pending.len() < buf.len() {
                match self.rx.try_recv() {
                    Ok(more) => self.pending.extend_from_slice(&more),
                    Err(_) => break,
                }
            }
        }
        let n = (self.pending.len() - self.pos).min(buf.len());
        buf[..n].copy_from_slice(&self.pending[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_all(r: &mut OutputReader) -> Vec<u8> {
        let mut out = Vec::new();
        r.read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn imprime_con_cr_y_termina_al_cerrar() {
        let (c, mut r) = Console::new();
        c.print("hola\nmundo\r\n");
        c.output(b"crudo".to_vec());
        c.close_output();
        assert_eq!(read_all(&mut r), b"hola\r\nmundo\r\ncrudo");
    }

    #[test]
    fn preguntar_y_responder_por_teclas() {
        let (c, _r) = Console::new();
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let c2 = c.clone();
        let answer = rt.block_on(async move {
            let ask = tokio::spawn(async move { c2.ask("¿Confiar? ", false).await });
            // Esperar a que la pregunta esté activa.
            while !c.is_prompting() {
                tokio::task::yield_now().await;
            }
            c.route(b"s\r");
            ask.await.unwrap()
        });
        assert_eq!(answer.as_deref(), Some("s"));
    }

    #[test]
    fn enter_desconectado_pide_reconectar() {
        let (c, _r) = Console::new();
        c.set_input(Input::Disconnected);
        assert!(matches!(c.route(b"x"), Routed::Nothing));
        assert!(
            matches!(c.route(b"\x1b[1;1R"), Routed::Nothing),
            "respuesta del emulador"
        );
        assert!(c.is_disconnected());
        assert!(matches!(c.route(b"\r"), Routed::Reconnect));
        assert!(!c.is_disconnected());
    }

    #[test]
    fn conectado_manda_al_canal() {
        let (c, _r) = Console::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        c.set_input(Input::Channel(tx));
        assert!(c.is_connected());
        c.route(b"ls\r");
        assert!(matches!(rx.try_recv(), Ok(ChannelCmd::Write(b)) if b == b"ls\r"));
        assert!(c.send(ChannelCmd::Resize(80, 24)));
    }
}
