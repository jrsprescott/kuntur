//! Kuntur: terminal para Windows pensada para agentes de IA.
// En release no abre una consola negra extra.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod sessions;
mod terminal_view;

use gpui_kit::*;
use terminal_view::TerminalView;

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Kuntur".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| TerminalView::new(window, cx)))
            .expect("no se pudo abrir la ventana");
    });
}
