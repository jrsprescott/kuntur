//! Textos de la interfaz en español e inglés (ajustes v0.2).
//!
//! Cada texto es una variante de [`T`] con su par (español, inglés): agregar uno obliga a escribir
//! las dos traducciones. El idioma en uso es global (lo fija `Workspace` al arrancar y al cambiarlo
//! en Configuración), así el dibujo solo llama `t(T::Algo)`.
//!
//! Por ahora se traducen Configuración, el árbol de archivos y su menú; el resto de la app se pasa
//! por partes.

use std::sync::atomic::{AtomicU8, Ordering};

use kuntur_config::Language;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Fija el idioma de la interfaz.
pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::Es
    }
}

/// Idioma efectivo: el elegido o, en "automático", el del sistema (`system`, por ejemplo "es-PE",
/// "en_US.UTF-8"). Español si empieza con "es"; cualquier otro, inglés.
pub fn resolve(setting: Language, system: Option<&str>) -> Lang {
    match setting {
        Language::Es => Lang::Es,
        Language::En => Lang::En,
        Language::Auto => match system {
            Some(s) if s.trim().to_ascii_lowercase().starts_with("es") => Lang::Es,
            Some(_) => Lang::En,
            // Sin dato del sistema: el idioma original de Kuntur.
            None => Lang::Es,
        },
    }
}

/// Idioma del sistema (Windows, Linux y Mac).
pub fn system_locale() -> Option<String> {
    sys_locale::get_locale()
}

/// Texto en el idioma en uso.
pub fn t(key: T) -> &'static str {
    tr(current(), key)
}

/// Texto en un idioma dado.
pub fn tr(lang: Lang, key: T) -> &'static str {
    let (es, en) = key.pair();
    match lang {
        Lang::Es => es,
        Lang::En => en,
    }
}

macro_rules! texts {
    ($($name:ident => $es:literal, $en:literal;)*) => {
        /// Claves de los textos traducidos.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum T {
            $($name,)*
        }

        impl T {
            #[cfg(test)]
            pub const ALL: &'static [T] = &[$(T::$name,)*];

            fn pair(self) -> (&'static str, &'static str) {
                match self {
                    $(T::$name => ($es, $en),)*
                }
            }
        }
    };
}

texts! {
    // ── Configuración ──
    Settings => "Configuración", "Settings";
    SettingsTip => "Configuración (Ctrl+,)", "Settings (Ctrl+,)";
    ThemeTip => "Tema y apariencia", "Theme and appearance";
    Appearance => "Apariencia", "Appearance";
    Theme => "Tema", "Theme";
    FontSize => "Tamaño de letra", "Font size";
    FontSmaller => "Más chica", "Smaller";
    FontBigger => "Más grande", "Bigger";
    SidePanel => "Panel lateral", "Side panel";
    SidePanelFull => "Completo", "Full";
    SidePanelIcons => "Íconos", "Icons";
    Marquee => "Desplazar el título de la pestaña", "Scroll the tab title";
    // ── Uso de agentes (ajustes v0.2.1) ──
    UsageNotInstalled => "no instalado", "not installed";
    UsageLocal => "local", "local";
    // ── Barra de título ──
    BrandTip => "Kuntur Terminal IA Agente · ver el proyecto en GitHub", "Kuntur Terminal IA Agente · view the project on GitHub";
    TipPanelFull =>"Panel completo (Ctrl+Shift+B)", "Full panel (Ctrl+Shift+B)";
    TipPanelCompact => "Panel compacto (Ctrl+Shift+B)", "Compact panel (Ctrl+Shift+B)";
    TipNewSession => "Nueva sesión (Ctrl+Shift+T)", "New session (Ctrl+Shift+T)";
    TipCloseSession => "Cerrar sesión (Ctrl+Shift+W)", "Close session (Ctrl+Shift+W)";
    TipDock => "Terminal acoplada (Ctrl+`)", "Docked terminal (Ctrl+`)";
    TipUsage => "Uso de agentes (Ctrl+Shift+U)", "Agent usage (Ctrl+Shift+U)";
    TipAudit => "Auditoría de comandos (Ctrl+Shift+A)", "Command audit (Ctrl+Shift+A)";
    TipConnections => "Conexiones SSH (Ctrl+Shift+O)", "SSH connections (Ctrl+Shift+O)";
    TipMcp => "MCP de la sesión (Ctrl+Shift+M)", "Session MCP (Ctrl+Shift+M)";
    TipActions => "Acciones (Ctrl+Shift+P)", "Actions (Ctrl+Shift+P)";
    TipOneAlert => "1 alerta sin ver", "1 unseen alert";
    TipAlerts => "alertas sin ver", "unseen alerts";
    MarqueeHint => "En la pestaña activa, si no entra; se detiene con el mouse", "On the active tab, when it doesn't fit; hovering stops it";
    Language => "Idioma", "Language";
    LanguageAuto => "Automático", "Automatic";
    Notifications => "Notificaciones", "Notifications";
    NotifyEnabled => "Mostrar notificaciones", "Show notifications";
    NotifyEnabledHint => "La insignia de la pestaña se muestra siempre", "The tab badge is always shown";
    NotifyWaiting => "Cuando espera tu respuesta", "When it waits for your reply";
    NotifyPermission => "Cuando pide permiso", "When it asks for permission";
    NotifyFinished => "Cuando termina", "When it finishes";
    NotifyInterval => "Tiempo mínimo entre avisos", "Minimum time between alerts";
    NotifyIntervalHint => "Por sesión; lo demás solo suma a la insignia", "Per session; the rest only adds to the badge";
    IntervalAlways => "Sin límite", "No limit";
    NotifyBackground => "Solo si Kuntur no está al frente", "Only when Kuntur is in the background";
    SettingsKeys => "↑↓ elegir · ←→ cambiar · Enter alternar · Esc cerrar", "↑↓ select · ←→ change · Enter toggle · Esc close";
    // ── Menú del árbol de archivos (como el Explorer de VS Code) ──
    MenuOpen => "Abrir", "Open";
    MenuOpenPreview => "Abrir vista previa", "Open Preview";
    MenuNewFile => "Nuevo archivo…", "New File...";
    MenuNewFolder => "Nueva carpeta…", "New Folder...";
    MenuRevealWindows => "Mostrar en el Explorador de archivos", "Reveal in File Explorer";
    MenuRevealMac => "Mostrar en Finder", "Reveal in Finder";
    MenuRevealLinux => "Abrir la carpeta contenedora", "Open Containing Folder";
    MenuImagesPreview => "Abrir en vista previa de imágenes", "Open in Images Preview";
    MenuOpenInTerminal => "Abrir en la terminal integrada", "Open in Integrated Terminal";
    MenuShare => "Compartir", "Share";
    MenuCopyGithubLink => "Copiar enlace de GitHub", "Copy GitHub Link";
    MenuCopyRemoteAddress => "Copiar usuario@host:ruta", "Copy user@host:path";
    MenuFindInFolder => "Buscar en la carpeta…", "Find in Folder...";
    MenuOpenTimeline => "Abrir línea de tiempo", "Open Timeline";
    MenuAddFolderToChat => "Agregar carpeta al chat", "Add Folder to Chat";
    MenuAddFileToChat => "Agregar archivo al chat", "Add File to Chat";
    MenuClaudeAddToChat => "Claude Code: Agregar al chat", "Claude Code: Add to Chat";
    MenuCut => "Cortar", "Cut";
    MenuCopy => "Copiar", "Copy";
    MenuPaste => "Pegar", "Paste";
    MenuCopyPath => "Copiar ruta", "Copy Path";
    MenuCopyRelativePath => "Copiar ruta relativa", "Copy Relative Path";
    MenuRename => "Cambiar nombre…", "Rename...";
    TabRename => "Renombrar…", "Rename…";
    TabClose => "Cerrar pestaña", "Close tab";
    MenuDelete => "Eliminar", "Delete";
    MenuDownloadThis => "Descargar esto", "Download This";
    MenuUploadThis => "Subir esto", "Upload This";
    MenuWarningDelete => "ADVERTENCIA: Eliminar esto", "WARNING: Delete This";
    MenuAddPythonProject => "Agregar como proyecto Python", "Add as Python Project";
    Soon => "(próximamente)", "(coming soon)";
    KeyReveal => "Shift+Alt+R", "Shift+Alt+R";
    KeyFind => "Shift+Alt+F", "Shift+Alt+F";
    KeyCut => "Ctrl+X", "Ctrl+X";
    KeyCopy => "Ctrl+C", "Ctrl+C";
    KeyPaste => "Ctrl+V", "Ctrl+V";
    KeyCopyPath => "Shift+Alt+C", "Shift+Alt+C";
    KeyCopyRelative => "Ctrl+K Ctrl+Shift+C", "Ctrl+K Ctrl+Shift+C";
    KeyRename => "F2", "F2";
    KeyDelete => "Supr", "Del";
    // ── Árbol de archivos ──
    FilesTitle => "Archivos", "Files";
    FilesUp => "Subir la raíz a la carpeta superior", "Move the root up one folder";
    FilesBackToSession => "Volver a la carpeta de la sesión", "Back to the session folder";
    FilesCollapseAll => "Plegar todo", "Collapse all";
    FilesLoading => "Cargando…", "Loading…";
    FilesEmpty => "Carpeta vacía", "Empty folder";
    FilesDropUpload => "Suelta archivos aquí para subirlos", "Drop files here to upload them";
    FilesDropCopy => "Suelta archivos aquí para copiarlos", "Drop files here to copy them";
    FilesReadError => "No se pudo leer la carpeta", "Could not read the folder";
    FilesReadErrorHint => "Puede faltar permiso o ya no existir", "Permission may be missing or it no longer exists";
    FilesNewFileHint => "Enter crea · Esc cancela", "Enter creates · Esc cancels";
    FilesRenameHint => "Enter cambia el nombre · Esc cancela", "Enter renames · Esc cancels";
    NameEmpty => "Escribe un nombre", "Type a name";
    NameInvalid => "Nombre no válido", "Invalid name";
    NameExists => "Ya existe un elemento con ese nombre", "An item with that name already exists";
    NoticePathCopied => "Ruta copiada", "Path copied";
    NoticeLinkCopied => "Enlace copiado", "Link copied";
    NoticeNotGithub => "No está en un repositorio de GitHub (o sin rama)", "Not in a GitHub repository (or no branch)";
    NoticeNoAgent => "No hay un agente corriendo en esta sesión", "No agent is running in this session";
    NoticeSentToAgent => "Ruta escrita en el agente (sin Enter)", "Path typed into the agent (no Enter)";
    NoticeCopied => "Copiado: Ctrl+V sobre una carpeta para pegar", "Copied: Ctrl+V on a folder to paste";
    NoticeCut => "Cortado: Ctrl+V sobre una carpeta para mover", "Cut: Ctrl+V on a folder to move";
    NoticeMoveInside => "No se puede mover una carpeta dentro de sí misma", "A folder cannot be moved into itself";
    NoticeNoImages => "No hay imágenes en esta carpeta", "There are no images in this folder";
    NoticeFailed => "No se pudo", "Could not";
    ConfirmTrash => "¿Mover a la Papelera?", "Move to the Recycle Bin?";
    ConfirmTrashHint => "Se puede recuperar desde la Papelera.", "You can restore it from the Recycle Bin.";
    ConfirmRemoteDelete => "¿Eliminar del servidor?", "Delete from the server?";
    ConfirmRemoteDeleteHint => "Se borra en el servidor, sin papelera. No se puede deshacer.", "It is deleted on the server, with no trash. This cannot be undone.";
    ButtonCancel => "Cancelar", "Cancel";
    ButtonDelete => "Eliminar", "Delete";
    NoticeDeleted => "Se eliminó", "Deleted";
    PickDownloadHere => "Descargar aquí", "Download here";
    PickUploadHere => "Subir aquí", "Upload here";
    // ── IA local (ajustes v0.2.1) ──
    LocalAi => "IA local", "Local AI";
    LocalServerOn => "servidor encendido", "server on";
    LocalServerOff => "servidor apagado (clic para iniciarlo)", "server off (click to start it)";
    LocalOpenCode => "OpenCode con este modelo", "OpenCode with this model";
    LocalOpenCodeMissing => "Instala OpenCode para usar el modelo como agente", "Install OpenCode to use the model as an agent";
    LocalChat => "Chat rápido", "Quick chat";
    LocalStartServer => "Iniciar servidor", "Start server";
    LocalStarting => "Iniciando el servidor local…", "Starting the local server…";
    LocalStartFailed => "No se pudo iniciar el servidor local", "Could not start the local server";
    LocalNoModels => "Sin modelos descargados", "No models downloaded";
    LocalMore => "más", "more";
    LocalLoaded => "cargado en memoria", "loaded in memory";
    LocalRemoteTab => "La IA local corre en esta PC: usa una pestaña local", "Local AI runs on this PC: use a local tab";
    // ── Notificaciones ──
    ToastWaiting => "espera tu respuesta", "is waiting for your reply";
    ToastPermission => "pide permiso", "asks for permission";
    ToastFinished => "terminó", "finished";
    ToastSession => "Sesión", "Session";
    // ── Imágenes como Warp (barra del agente) ──
    ImagePick => "Imagen…", "Image…";
    ImagePickTip => "Elegir una imagen para el agente", "Choose an image for the agent";
    ImagePickPrompt => "Imagen para el agente", "Image for the agent";
    ImageClearTip => "Quitar las miniaturas de la barra", "Remove the thumbnails from the bar";
    ImageOpenPreview => "Abrir en la vista previa", "Open in the preview";
    ImageSentTo => "Imagen enviada a", "Image sent to";
    ImagePathPasted => "Imagen guardada; se pegó su ruta", "Image saved; its path was pasted";
    ImageUploading => "Subiendo la imagen al servidor…", "Uploading the image to the server…";
    ImageUploaded => "Imagen subida al servidor; se pegó su ruta", "Image uploaded to the server; its path was pasted";
    ImageFailed => "No se pudo usar la imagen", "Could not use the image";
    FilesPathsPasted => "Se pegaron las rutas", "The paths were pasted";
    // ── Actualizaciones ──
    Updates => "Actualizaciones", "Updates";
    UpdatesAuto => "Buscar actualizaciones automáticamente", "Check for updates automatically";
    UpdatesAutoHint => "Al abrir Kuntur y cada 24 h, en GitHub", "When Kuntur opens and every 24 h, on GitHub";
    UpdatesCheckNow => "Buscar ahora", "Check now";
    UpdatesVersion => "Versión instalada", "Installed version";
    UpdatesChecking => "Buscando…", "Checking…";
    UpdatesUpToDate => "Kuntur está al día", "Kuntur is up to date";
    UpdatesAvailable => "disponible", "available";
    UpdatesInstall => "Actualizar", "Update";
    UpdatesDownloading => "Descargando", "Downloading";
    UpdatesInstalling => "Abriendo el instalador…", "Starting the installer…";
    UpdatesFailed => "No se pudo actualizar", "Could not update";
    UpdatesOpenPage => "Ver la versión", "View release";
    UpdatesTip => "Descarga el instalador, verifica su SHA-256 y su firma, y lo ejecuta", "Downloads the installer, checks its SHA-256 and signature, and runs it";
}

#[cfg(test)]
mod tests {
    use super::{Lang, T, resolve, tr};
    use kuntur_config::Language;

    #[test]
    fn todos_los_textos_tienen_las_dos_traducciones() {
        for &key in T::ALL {
            assert!(!tr(Lang::Es, key).trim().is_empty(), "{key:?} sin español");
            assert!(!tr(Lang::En, key).trim().is_empty(), "{key:?} sin inglés");
        }
    }

    #[test]
    fn idioma_elegido_o_del_sistema() {
        assert_eq!(resolve(Language::Es, Some("en-US")), Lang::Es);
        assert_eq!(resolve(Language::En, Some("es-PE")), Lang::En);
        assert_eq!(resolve(Language::Auto, Some("es-PE")), Lang::Es);
        assert_eq!(resolve(Language::Auto, Some("ES_es.UTF-8")), Lang::Es);
        assert_eq!(resolve(Language::Auto, Some("en-GB")), Lang::En);
        assert_eq!(resolve(Language::Auto, Some("pt-BR")), Lang::En);
        assert_eq!(resolve(Language::Auto, None), Lang::Es);
    }

    #[test]
    fn el_menu_se_traduce() {
        assert_eq!(tr(Lang::Es, T::Settings), "Configuración");
        assert_eq!(tr(Lang::En, T::Settings), "Settings");
    }
}
