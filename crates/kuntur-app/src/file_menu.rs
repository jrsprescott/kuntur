//! Menú contextual del árbol de archivos (ajustes v0.2), con las secciones y separadores del
//! Explorer de VS Code (sin "Diff This"). Lógica pura: qué opciones hay, en qué orden, con qué
//! atajo y si están activas; la UI lo dibuja y ejecuta la acción. También: validar nombres, la
//! referencia que se escribe en el chat del agente y la línea del entorno de Python.

use kuntur_agents::ShellFlavor;
use kuntur_files::PathStyle;

use crate::i18n::T;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    /// Archivo local: abrir con su programa.
    Open,
    /// Archivo local: vista previa (F8).
    OpenPreview,
    NewFile,
    NewFolder,
    Reveal,
    ImagesPreview,
    OpenInTerminal,
    /// Abre el submenú "Compartir ▸".
    Share,
    CopyGithubLink,
    CopyRemoteAddress,
    FindInFolder,
    OpenTimeline,
    AddToChat,
    ClaudeAddToChat,
    Cut,
    Copy,
    Paste,
    CopyPath,
    CopyRelativePath,
    Rename,
    /// Local: a la Papelera (con confirmación).
    Delete,
    Download,
    Upload,
    /// Remoto: borra en el servidor, sin papelera (con confirmación fuerte).
    WarningDelete,
    AddPythonProject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub action: MenuAction,
    pub label: T,
    pub shortcut: Option<T>,
    pub enabled: bool,
    /// Se muestra con "(próximamente)".
    pub soon: bool,
    /// Abre un submenú (▸).
    pub submenu: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    Item(MenuItem),
    Separator,
}

/// Lo que el menú necesita saber de la entrada y la sesión.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuContext {
    pub is_dir: bool,
    pub remote: bool,
    /// Hay archivos copiados o cortados para pegar.
    pub can_paste: bool,
    /// Corre un agente en la sesión activa.
    pub agent: bool,
    /// Ese agente es Claude Code.
    pub claude: bool,
}

fn item(action: MenuAction, label: T, shortcut: Option<T>, enabled: bool) -> MenuEntry {
    MenuEntry::Item(MenuItem {
        action,
        label,
        shortcut,
        enabled,
        soon: false,
        submenu: false,
    })
}

fn soon(action: MenuAction, label: T, shortcut: Option<T>) -> MenuEntry {
    MenuEntry::Item(MenuItem {
        action,
        label,
        shortcut,
        enabled: false,
        soon: true,
        submenu: false,
    })
}

/// Texto de "Mostrar en el gestor de archivos" según el sistema.
pub fn reveal_label() -> T {
    if cfg!(windows) {
        T::MenuRevealWindows
    } else if cfg!(target_os = "macos") {
        T::MenuRevealMac
    } else {
        T::MenuRevealLinux
    }
}

/// Las opciones del menú, en el orden de VS Code.
pub fn build_menu(ctx: MenuContext) -> Vec<MenuEntry> {
    use MenuAction as A;
    let local = !ctx.remote;
    let mut out = Vec::new();
    if !ctx.is_dir && local {
        out.push(item(A::Open, T::MenuOpen, None, true));
        out.push(item(A::OpenPreview, T::MenuOpenPreview, None, true));
        out.push(MenuEntry::Separator);
    }
    out.push(item(A::NewFile, T::MenuNewFile, None, true));
    out.push(item(A::NewFolder, T::MenuNewFolder, None, true));
    out.push(item(A::Reveal, reveal_label(), Some(T::KeyReveal), local));
    if ctx.is_dir {
        out.push(item(A::ImagesPreview, T::MenuImagesPreview, None, local));
    }
    out.push(item(A::OpenInTerminal, T::MenuOpenInTerminal, None, true));
    out.push(MenuEntry::Separator);
    out.push(MenuEntry::Item(MenuItem {
        action: A::Share,
        label: T::MenuShare,
        shortcut: None,
        enabled: true,
        soon: false,
        submenu: true,
    }));
    out.push(MenuEntry::Separator);
    if ctx.is_dir {
        out.push(soon(A::FindInFolder, T::MenuFindInFolder, Some(T::KeyFind)));
        out.push(MenuEntry::Separator);
    }
    out.push(soon(A::OpenTimeline, T::MenuOpenTimeline, None));
    out.push(MenuEntry::Separator);
    out.push(item(
        A::AddToChat,
        if ctx.is_dir {
            T::MenuAddFolderToChat
        } else {
            T::MenuAddFileToChat
        },
        None,
        ctx.agent,
    ));
    out.push(item(A::ClaudeAddToChat, T::MenuClaudeAddToChat, None, ctx.claude));
    out.push(MenuEntry::Separator);
    out.push(item(A::Cut, T::MenuCut, Some(T::KeyCut), true));
    out.push(item(A::Copy, T::MenuCopy, Some(T::KeyCopy), true));
    out.push(item(A::Paste, T::MenuPaste, Some(T::KeyPaste), ctx.can_paste));
    out.push(MenuEntry::Separator);
    out.push(item(A::CopyPath, T::MenuCopyPath, Some(T::KeyCopyPath), true));
    out.push(item(
        A::CopyRelativePath,
        T::MenuCopyRelativePath,
        Some(T::KeyCopyRelative),
        true,
    ));
    out.push(MenuEntry::Separator);
    out.push(item(A::Rename, T::MenuRename, Some(T::KeyRename), true));
    if local {
        out.push(item(A::Delete, T::MenuDelete, Some(T::KeyDelete), true));
    }
    if ctx.remote {
        out.push(MenuEntry::Separator);
        out.push(item(A::Download, T::MenuDownloadThis, None, true));
        out.push(item(A::Upload, T::MenuUploadThis, None, true));
        out.push(item(A::WarningDelete, T::MenuWarningDelete, Some(T::KeyDelete), true));
    }
    if ctx.is_dir {
        out.push(MenuEntry::Separator);
        out.push(item(A::AddPythonProject, T::MenuAddPythonProject, None, true));
    }
    out
}

/// Opciones del submenú "Compartir ▸".
pub fn share_menu(remote: bool) -> Vec<MenuItem> {
    let mk = |action, label, enabled| MenuItem {
        action,
        label,
        shortcut: None,
        enabled,
        soon: false,
        submenu: false,
    };
    vec![
        mk(MenuAction::CopyGithubLink, T::MenuCopyGithubLink, !remote),
        mk(MenuAction::CopyRemoteAddress, T::MenuCopyRemoteAddress, remote),
    ]
}

/// Nombre nuevo de un archivo o carpeta: `Err` con el motivo si no sirve.
pub fn validate_name(name: &str) -> Result<String, T> {
    let name = name.trim();
    if name.is_empty() {
        return Err(T::NameEmpty);
    }
    let bad: &[char] = if cfg!(windows) {
        &['/', '\\', '<', '>', ':', '"', '|', '?', '*']
    } else {
        &['/', '\\']
    };
    if name == "." || name == ".." || name.contains(bad) || name.chars().any(char::is_control) {
        return Err(T::NameInvalid);
    }
    // Windows no acepta nombres que terminen en punto o espacio.
    if cfg!(windows) && name.ends_with('.') {
        return Err(T::NameInvalid);
    }
    Ok(name.to_string())
}

/// Lo que se escribe en el chat del agente para referirse a `path` (sin Enter): `@ruta` relativa a
/// la carpeta del agente para los que entienden menciones (Claude Code, Gemini, OpenCode, Qwen); si
/// no, la ruta (entre comillas si tiene espacios). Termina en espacio para seguir escribiendo.
pub fn chat_reference(agent: &str, path: &str, cwd: Option<&str>, style: PathStyle) -> String {
    let mentions = matches!(agent, "claude" | "gemini" | "opencode" | "qwen");
    let rel = cwd
        .and_then(|c| style.relative(c, path))
        .filter(|r| r != ".")
        .unwrap_or_else(|| path.to_string());
    let rel = if style == PathStyle::Posix || !cfg!(windows) {
        rel
    } else {
        // Las menciones de los agentes usan "/" también en Windows.
        rel.replace('\\', "/")
    };
    match (mentions, rel.contains(' ')) {
        (true, false) => format!("@{rel} "),
        (_, true) => format!("\"{path}\" "),
        (false, false) => format!("{path} "),
    }
}

/// Línea (sin Enter) que usa el `.venv` de la carpeta o lo crea, y lo activa, según el shell.
pub fn python_venv_line(flavor: ShellFlavor) -> &'static str {
    match flavor {
        ShellFlavor::PowerShell => {
            "if (-not (Test-Path .venv)) { python -m venv .venv }; .\\.venv\\Scripts\\Activate.ps1"
        }
        ShellFlavor::Cmd => "if not exist .venv python -m venv .venv & .venv\\Scripts\\activate.bat",
        ShellFlavor::Posix => "[ -d .venv ] || python3 -m venv .venv; . .venv/bin/activate",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actions(entries: &[MenuEntry]) -> Vec<MenuAction> {
        entries
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Item(i) => Some(i.action),
                MenuEntry::Separator => None,
            })
            .collect()
    }

    fn find(entries: &[MenuEntry], a: MenuAction) -> Option<MenuItem> {
        entries.iter().find_map(|e| match e {
            MenuEntry::Item(i) if i.action == a => Some(*i),
            _ => None,
        })
    }

    #[test]
    fn carpeta_local_en_el_orden_de_vs_code() {
        use MenuAction as A;
        let m = build_menu(MenuContext {
            is_dir: true,
            ..Default::default()
        });
        assert_eq!(
            actions(&m),
            [
                A::NewFile,
                A::NewFolder,
                A::Reveal,
                A::ImagesPreview,
                A::OpenInTerminal,
                A::Share,
                A::FindInFolder,
                A::OpenTimeline,
                A::AddToChat,
                A::ClaudeAddToChat,
                A::Cut,
                A::Copy,
                A::Paste,
                A::CopyPath,
                A::CopyRelativePath,
                A::Rename,
                A::Delete,
                A::AddPythonProject,
            ]
        );
        // Sin separadores repetidos ni al principio/final.
        assert!(!matches!(m.first(), Some(MenuEntry::Separator)));
        assert!(!matches!(m.last(), Some(MenuEntry::Separator)));
        assert!(
            m.windows(2)
                .all(|w| !(w[0] == MenuEntry::Separator && w[1] == MenuEntry::Separator))
        );
    }

    #[test]
    fn lo_que_no_aplica_queda_en_gris() {
        let m = build_menu(MenuContext {
            is_dir: true,
            ..Default::default()
        });
        assert!(!find(&m, MenuAction::Paste).unwrap().enabled, "nada copiado");
        assert!(!find(&m, MenuAction::AddToChat).unwrap().enabled, "sin agente");
        assert!(!find(&m, MenuAction::ClaudeAddToChat).unwrap().enabled);
        let timeline = find(&m, MenuAction::OpenTimeline).unwrap();
        assert!(!timeline.enabled && timeline.soon);
        let m = build_menu(MenuContext {
            is_dir: true,
            can_paste: true,
            agent: true,
            claude: true,
            ..Default::default()
        });
        assert!(find(&m, MenuAction::Paste).unwrap().enabled);
        assert!(find(&m, MenuAction::ClaudeAddToChat).unwrap().enabled);
    }

    #[test]
    fn remoto_tiene_las_opciones_de_sftp_y_borra_con_advertencia() {
        let m = build_menu(MenuContext {
            is_dir: true,
            remote: true,
            ..Default::default()
        });
        let a = actions(&m);
        assert!(a.contains(&MenuAction::Download) && a.contains(&MenuAction::Upload));
        assert!(a.contains(&MenuAction::WarningDelete));
        assert!(!a.contains(&MenuAction::Delete), "en remoto no hay papelera");
        assert!(!find(&m, MenuAction::Reveal).unwrap().enabled);
        assert!(!find(&m, MenuAction::ImagesPreview).unwrap().enabled);
        let share = share_menu(true);
        assert!(!share[0].enabled && share[1].enabled);
    }

    #[test]
    fn archivo_local_empieza_con_abrir() {
        let m = build_menu(MenuContext::default());
        let a = actions(&m);
        assert_eq!(&a[..2], [MenuAction::Open, MenuAction::OpenPreview]);
        assert!(!a.contains(&MenuAction::ImagesPreview));
        assert!(!a.contains(&MenuAction::FindInFolder));
        assert!(!a.contains(&MenuAction::AddPythonProject));
        assert_eq!(find(&m, MenuAction::AddToChat).unwrap().label, T::MenuAddFileToChat);
    }

    #[test]
    fn nunca_aparece_diff() {
        for is_dir in [true, false] {
            for remote in [true, false] {
                let m = build_menu(MenuContext {
                    is_dir,
                    remote,
                    ..Default::default()
                });
                for e in &m {
                    if let MenuEntry::Item(i) = e {
                        let es = crate::i18n::tr(crate::i18n::Lang::Es, i.label).to_lowercase();
                        let en = crate::i18n::tr(crate::i18n::Lang::En, i.label).to_lowercase();
                        assert!(!en.contains("diff") && !es.contains("diff"));
                    }
                }
            }
        }
    }

    #[test]
    fn nombres_validos() {
        assert_eq!(validate_name("  notas.md "), Ok("notas.md".into()));
        assert_eq!(validate_name(""), Err(T::NameEmpty));
        assert_eq!(validate_name("   "), Err(T::NameEmpty));
        assert_eq!(validate_name(".."), Err(T::NameInvalid));
        assert_eq!(validate_name("a/b"), Err(T::NameInvalid));
        assert_eq!(validate_name("a\\b"), Err(T::NameInvalid));
        #[cfg(windows)]
        assert_eq!(validate_name("que?.txt"), Err(T::NameInvalid));
    }

    #[test]
    fn referencia_en_el_chat_del_agente() {
        let p = PathStyle::Posix;
        assert_eq!(
            chat_reference("claude", "/home/demo/web/src", Some("/home/demo/web"), p),
            "@src "
        );
        // Fuera de la carpeta del agente: absoluta.
        assert_eq!(
            chat_reference("claude", "/srv/datos", Some("/home/demo/web"), p),
            "@/srv/datos "
        );
        // Con espacios: entre comillas, sin mención.
        assert_eq!(
            chat_reference("claude", "/home/demo/mis datos", Some("/home/demo"), p),
            "\"/home/demo/mis datos\" "
        );
        // Agentes sin menciones: la ruta.
        assert_eq!(
            chat_reference("codex", "/home/demo/web/src", Some("/home/demo/web"), p),
            "/home/demo/web/src "
        );
    }

    #[test]
    fn linea_del_entorno_de_python_por_shell() {
        assert!(python_venv_line(ShellFlavor::PowerShell).contains("Activate.ps1"));
        assert!(python_venv_line(ShellFlavor::Cmd).contains("activate.bat"));
        assert!(python_venv_line(ShellFlavor::Posix).contains(". .venv/bin/activate"));
        for f in [ShellFlavor::PowerShell, ShellFlavor::Cmd, ShellFlavor::Posix] {
            assert!(!python_venv_line(f).contains('\n'), "una sola línea, sin Enter");
        }
    }
}
