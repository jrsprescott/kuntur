// Prueba técnica de Fase 0: ventana mínima con gpui-kit.
// Ejecutar: cargo run --release
use gpui_kit::component::button::*;
use gpui_kit::component::label::Label;
use gpui_kit::component::*;
use gpui_kit::*;

pub struct Hola;

impl Render for Hola {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_2()
            .size_full()
            .items_center()
            .justify_center()
            .child(Label::new("Kuntur · prueba GPUI"))
            .child(
                Button::new("ok")
                    .primary()
                    .label("Funciona")
                    .on_click(|_, _, _| println!("clic")),
            )
    }
}

fn main() {
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| Hola))
            .expect("no se pudo abrir la ventana");
    });
}
