//! Kuntur: terminal para Windows pensada para agentes de IA.
// En release no abre una consola negra extra.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent_icons;
mod agents_panel;
mod clipboard_image;
mod command_palette;
mod connections;
mod dock;
mod file_menu;
mod i18n;
mod idle;
mod image_paste;
mod keynav;
mod marquee;
mod monitor;
mod notify;
mod sessions;
mod terminal_view;
mod theme_bridge;
mod usage_format;
mod workspace;

use std::borrow::Cow;
use std::path::PathBuf;

use gpui_kit::component::TitleBar;
use gpui_kit::*;
use kuntur_config::Settings;
use workspace::Workspace;

// Íconos que no vienen en el paquete por defecto de gpui-kit.
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        Command,
        ChartColumn,
        ScrollText,
        Server,
        Pencil,
        Trash,
        Download,
        Plug,
        FileCode,
        FileImage,
        FileArchive
    ]
);

/// Íconos por defecto de gpui-kit + los que agrega Kuntur.
struct KunturAssets;

impl AssetSource for KunturAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        // Ajustes v0.2.1: íconos de marca de los agentes (`agents/…`).
        if let Some(bytes) = agent_icons::asset(path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        match gpui_kit::assets::Assets.load(path) {
            Ok(Some(bytes)) => Ok(Some(bytes)),
            _ => ExtraIcons.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut all = gpui_kit::assets::Assets.list(path)?;
        all.extend(ExtraIcons.list(path)?);
        if agent_icons::PREFIX.starts_with(path) {
            all.extend(agent_icons::paths().map(SharedString::from));
        }
        Ok(all)
    }
}

/// `KUNTUR_CONFIG` (pruebas) o `%APPDATA%\Kuntur\config.toml`.
fn config_path() -> Option<PathBuf> {
    std::env::var_os("KUNTUR_CONFIG")
        .map(PathBuf::from)
        .or_else(Settings::default_path)
}

/// Pruebas de humo: `KUNTUR_SMOKE_DISPLAY=n` abre la ventana en la pantalla n (contando de
/// izquierda a derecha desde 0) y **sin tomar el foco**, para no interrumpir lo que el usuario hace
/// en la pantalla principal (por ejemplo, un juego a pantalla completa).
fn smoke_window(options: &mut WindowOptions, cx: &App) {
    let Some(index) = std::env::var("KUNTUR_SMOKE_DISPLAY")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
    else {
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
    if let Some(display) = displays.get(index) {
        let id = display.id();
        options.display_id = Some(id);
        options.window_bounds = Some(WindowBounds::Windowed(Bounds::centered(
            Some(id),
            size(px(1536.), px(1000.)),
            cx,
        )));
    }
}

fn main() {
    let config = config_path();
    let settings = config.as_deref().map(Settings::load).unwrap_or_default();
    terminal_view::ghost::init(settings.autocompletado);

    gpui_kit::application().with_assets(KunturAssets).run(move |cx| {
        gpui_kit::init(cx);
        // Texto por carácter en la terminal (ñ por atajos, teclas muertas, AltGr, IME).
        terminal_view::install_keyboard_tracking(cx);
        // Instalado con el MSI (F11): la ventana usa el AUMID del acceso del menú Inicio (se agrupa
        // con él en la barra de tareas). Sin instalar no se toca nada.
        if notify::app_id_registered() {
            cx.set_app_identity(notify::APP_ID, notify::APP_NAME);
        }
        // Al cerrar la ventana termina la app (y con ella los shells).
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let mut options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Kuntur".into()),
                ..TitleBar::title_bar_options()
            }),
            window_min_size: Some(size(px(480.), px(320.))),
            ..TitleBar::window_options()
        };
        smoke_window(&mut options, cx);
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Workspace::new(window, cx, settings, config))
        })
        .expect("no se pudo abrir la ventana");
    });
}
