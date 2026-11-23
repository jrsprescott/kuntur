//! Kuntur: terminal para Windows pensada para agentes de IA.
// En release no abre una consola negra extra.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agents_panel;
mod command_palette;
mod monitor;
mod sessions;
mod terminal_view;
mod theme_bridge;
mod workspace;

use std::borrow::Cow;
use std::path::PathBuf;

use gpui_kit::component::TitleBar;
use gpui_kit::*;
use kuntur_config::Settings;
use workspace::Workspace;

// Íconos que no vienen en el paquete por defecto de gpui-kit.
gpui_kit::assets::icon_assets!(ExtraIcons, [Command, ChartColumn]);

/// Íconos por defecto de gpui-kit + los que agrega Kuntur.
struct KunturAssets;

impl AssetSource for KunturAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match gpui_kit::assets::Assets.load(path) {
            Ok(Some(bytes)) => Ok(Some(bytes)),
            _ => ExtraIcons.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut all = gpui_kit::assets::Assets.list(path)?;
        all.extend(ExtraIcons.list(path)?);
        Ok(all)
    }
}

/// `KUNTUR_CONFIG` (pruebas) o `%APPDATA%\Kuntur\config.toml`.
fn config_path() -> Option<PathBuf> {
    std::env::var_os("KUNTUR_CONFIG")
        .map(PathBuf::from)
        .or_else(Settings::default_path)
}

fn main() {
    let config = config_path();
    let settings = config.as_deref().map(Settings::load).unwrap_or_default();

    gpui_kit::application().with_assets(KunturAssets).run(move |cx| {
        gpui_kit::init(cx);
        // Al cerrar la ventana termina la app (y con ella los shells).
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Kuntur".into()),
                ..TitleBar::title_bar_options()
            }),
            window_min_size: Some(size(px(480.), px(320.))),
            ..TitleBar::window_options()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Workspace::new(window, cx, settings, config))
        })
        .expect("no se pudo abrir la ventana");
    });
}
