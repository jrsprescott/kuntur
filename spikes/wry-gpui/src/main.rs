//! Prueba técnica de F8: WebView2 (wry) como ventana hija de una ventana GPUI.
//!
//! - Izquierda: panel GPUI con botones (menú, ocultar, cerrar/abrir el visor web).
//! - Derecha: un área GPUI cuyo rectángulo se copia al WebView2 en cada cuadro (posición y tamaño).
//! - El "menú" es un panel GPUI que se superpone al área del visor: muestra el problema de z-order.
//!
//! Variables (para medir sin tocar el teclado del sistema):
//! - `SPIKE_DISPLAY=n`: abre en la pantalla n (de izquierda a derecha) y sin tomar el foco.
//! - `SPIKE_FILE=ruta`: archivo a mostrar (HTML o PDF); por defecto `datos/ejemplo.html`.
//! - `SPIKE_STEP=menu|menu-hide|resize|close`: acción automática a los 4 s.
// Sin consola extra (la medición busca la ventana principal del proceso).
#![windows_subsystem = "windows"]
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::*;
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{Rect, WebView, WebViewBuilder};

struct Spike {
    web: Rc<RefCell<Option<WebView>>>,
    area: Rc<Cell<Bounds<Pixels>>>,
    menu_open: bool,
    /// Ocultar el visor mientras hay un menú abierto (la solución al z-order).
    hide_with_menu: bool,
    file: PathBuf,
}

/// Sin consola: los mensajes van a `spike.log` junto al ejecutable.
fn log(msg: &str) {
    use std::io::Write;
    let path = std::env::current_exe().unwrap_or_default().with_file_name("spike.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{msg}");
    }
}

fn file_url(path: &std::path::Path) -> String {
    let p = path.to_string_lossy().replace('\\', "/");
    format!("file:///{p}")
}

impl Spike {
    fn open_web(&mut self, window: &mut Window) {
        if self.web.borrow().is_some() {
            return;
        }
        let b = self.area.get();
        let started = std::time::Instant::now();
        let built = WebViewBuilder::new()
            .with_url(file_url(&self.file))
            .with_bounds(rect(b))
            .build_as_child(window);
        match built {
            Ok(w) => {
                log(&format!("WebView2 creado en {:?}", started.elapsed()));
                *self.web.borrow_mut() = Some(w);
            }
            Err(e) => log(&format!("no se pudo crear el WebView2: {e}")),
        }
    }

    fn close_web(&mut self) {
        // Soltar el WebView destruye la ventana hija y (al rato) sus procesos.
        self.web.borrow_mut().take();
    }

    fn set_menu(&mut self, open: bool, cx: &mut Context<Self>) {
        self.menu_open = open;
        if self.hide_with_menu {
            if let Some(w) = self.web.borrow().as_ref() {
                let _ = w.set_visible(!open);
            }
        }
        cx.notify();
    }
}

fn rect(b: Bounds<Pixels>) -> Rect {
    Rect {
        position: LogicalPosition::new(f32::from(b.origin.x) as f64, f32::from(b.origin.y) as f64).into(),
        size: LogicalSize::new(
            f32::from(b.size.width).max(1.0) as f64,
            f32::from(b.size.height).max(1.0) as f64,
        )
        .into(),
    }
}

fn button(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(10.))
        .py(px(6.))
        .rounded(px(6.))
        .bg(rgb(0x2b3340))
        .text_color(rgb(0xe6e6e6))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x3b4558)))
        .child(label)
}

impl Render for Spike {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (web, area) = (self.web.clone(), self.area.clone());
        let left = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(16.))
            .w(px(360.))
            .h_full()
            .bg(rgb(0x161a21))
            .text_color(rgb(0xd0d0d0))
            .child("Spike wry · WebView2 hijo de GPUI")
            .child(
                button("menu", "Abrir menú (z-order)")
                    .on_click(cx.listener(|this, _, _, cx| this.set_menu(!this.menu_open, cx))),
            )
            .child(button("hide", "Ocultar con menú: sí/no").on_click(cx.listener(|this, _, _, cx| {
                this.hide_with_menu = !this.hide_with_menu;
                cx.notify();
            })))
            .child(button("close", "Cerrar visor web").on_click(cx.listener(|this, _, _, cx| {
                this.close_web();
                cx.notify();
            })))
            .child(button("open", "Abrir visor web").on_click(cx.listener(|this, _, window, cx| {
                this.open_web(window);
                cx.notify();
            })))
            .child(format!(
                "Visor: {} · ocultar con menú: {}",
                if self.web.borrow().is_some() { "abierto" } else { "cerrado" },
                if self.hide_with_menu { "sí" } else { "no" }
            ));

        // Área del visor: su rectángulo (en píxeles lógicos de la ventana) se copia al WebView2.
        let right = div().flex_1().h_full().p(px(12.)).bg(rgb(0x0f1115)).child(
            canvas(
                move |bounds, _, _| {
                    if area.get() != bounds {
                        area.set(bounds);
                        if let Some(w) = web.borrow().as_ref() {
                            let _ = w.set_bounds(rect(bounds));
                        }
                    }
                },
                |_, _, _, _| {},
            )
            .size_full(),
        );

        let menu = self.menu_open.then(|| {
            deferred(
                div()
                    .absolute()
                    .top(px(60.))
                    .left(px(200.))
                    .w(px(420.))
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .bg(rgb(0x2a2f3a))
                    .border_1()
                    .border_color(rgb(0x5b9bd5))
                    .rounded(px(8.))
                    .text_color(rgb(0xffffff))
                    .child("Menú GPUI · ¿se ve encima del visor web?")
                    .child("Elemento 1 — Vista previa")
                    .child("Elemento 2 — Abrir con su programa")
                    .child("Elemento 3 — Copiar ruta"),
            )
            .with_priority(1)
        });

        div()
            .relative()
            .flex()
            .size_full()
            .child(left)
            .child(right)
            .children(menu)
    }
}

fn place(options: &mut WindowOptions, cx: &App) {
    let Some(index) = std::env::var("SPIKE_DISPLAY").ok().and_then(|v| v.parse::<usize>().ok()) else {
        options.window_bounds = Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1200.), px(760.)), cx)));
        return;
    };
    options.focus = false;
    let mut displays = cx.displays();
    displays.sort_by(|a, b| {
        a.bounds()
            .origin
            .x
            .partial_cmp(&b.bounds().origin.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if let Some(d) = displays.get(index) {
        options.display_id = Some(d.id());
        options.window_bounds = Some(WindowBounds::Windowed(Bounds::centered(
            Some(d.id()),
            size(px(1200.), px(760.)),
            cx,
        )));
    }
}

fn main() {
    let file = std::env::var_os("SPIKE_FILE").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("datos").join("ejemplo.html")
    });
    let step = std::env::var("SPIKE_STEP").unwrap_or_default();
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        let mut options = WindowOptions::default();
        place(&mut options, cx);
        let step = step.clone();
        let file = file.clone();
        gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| {
                // El área aún no tiene tamaño: se crea en el primer cuadro (ver abajo).
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(500)).await;
                    let _ = this.update_in(cx, |s: &mut Spike, window, cx| {
                        s.open_web(window);
                        cx.notify();
                    });
                    cx.background_executor().timer(Duration::from_millis(3500)).await;
                    let _ = this.update_in(cx, |s: &mut Spike, window, cx| match step.as_str() {
                        "menu" => s.set_menu(true, cx),
                        "menu-hide" => {
                            s.hide_with_menu = true;
                            s.set_menu(true, cx);
                        }
                        "resize" => window.resize(size(px(900.), px(560.))),
                        "close" => {
                            s.close_web();
                            cx.notify();
                        }
                        _ => {}
                    });
                })
                .detach();
                Spike {
                    web: Rc::new(RefCell::new(None)),
                    area: Rc::new(Cell::new(Bounds::default())),
                    menu_open: false,
                    hide_with_menu: false,
                    file,
                }
            })
        })
        .expect("no se pudo abrir la ventana");
    });
}
