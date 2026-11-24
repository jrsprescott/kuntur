//! Aviso de actualización (ajustes v0.2, §7 del plan).
//!
//! - Busca versiones nuevas en GitHub 10 s después de abrir Kuntur y luego cada 24 h, solo con
//!   `buscar_actualizaciones = true` (Configuración → Actualizaciones). Es la única conexión de red
//!   propia de Kuntur; nunca en las pruebas de humo ni en `cargo test`.
//! - Si hay una versión nueva, la línea inferior muestra "Kuntur X disponible · Actualizar".
//! - "Actualizar" (Windows): descarga el `.msi` a la carpeta local de Kuntur (`actualizacion\`),
//!   verifica su SHA-256 y su firma (ver `kuntur_update::signature`), ejecuta
//!   `msiexec /i … /passive` (vía `cmd`, que al terminar vuelve a abrir Kuntur) y cierra Kuntur. En
//!   Linux/Mac (sin instalador propio todavía) abre la página de la versión en el navegador.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use kuntur_update::Update;
use kuntur_update::signature::{self, Signature};

use super::*;
use crate::i18n::{T, t};

/// Primera búsqueda después de abrir Kuntur (no compite con el arranque).
const FIRST_CHECK: Duration = Duration::from_secs(10);
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// Repintado del progreso de la descarga.
const PROGRESS_TICK: Duration = Duration::from_millis(250);

#[derive(Clone, Debug, Default)]
pub(super) enum UpdatePhase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Update),
    Downloading(Update),
    Installing,
    Failed(String),
}

#[derive(Default)]
pub(super) struct UpdateState {
    pub(super) phase: UpdatePhase,
    /// La búsqueda la pidió el usuario: se muestra "al día" o el error.
    manual: bool,
    check: Option<Task<()>>,
    download: Option<Task<()>>,
    poll: Option<Task<()>>,
    timer: Option<Task<()>>,
    cancel: Arc<AtomicBool>,
    done: Arc<AtomicU64>,
    /// 0 = tamaño desconocido.
    total: Arc<AtomicU64>,
}

/// ¿Se puede usar la red? Nunca en las pruebas de humo (variables `KUNTUR_SMOKE_*`).
fn network_allowed() -> bool {
    !std::env::vars_os().any(|(k, _)| k.to_string_lossy().starts_with("KUNTUR_SMOKE_"))
}

/// Porcentaje de la descarga (None si no se sabe el total).
fn percent(done: u64, total: u64) -> Option<u64> {
    (total > 0).then(|| (done.min(total) * 100) / total)
}

impl Workspace {
    /// Arranca la búsqueda periódica (o, en la prueba de humo `KUNTUR_SMOKE_UPDATE`, un aviso
    /// sintético sin red).
    pub(super) fn start_updates(&mut self, cx: &mut Context<Self>) {
        if let Ok(v) = std::env::var("KUNTUR_SMOKE_UPDATE") {
            self.update.phase = UpdatePhase::Available(Update {
                version: kuntur_update::release::parse_tag(&v).unwrap_or_else(semver_next),
                page: format!("https://github.com/{}/releases", kuntur_update::REPO),
                installer: None,
            });
            return;
        }
        if !network_allowed() {
            return;
        }
        self.update.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FIRST_CHECK).await;
            loop {
                let alive = this.update(cx, |w, cx| {
                    if w.settings.check_updates {
                        w.check_updates(false, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
                cx.background_executor().timer(EVERY).await;
            }
        }));
    }

    /// Busca una versión nueva fuera del hilo de la UI. `manual` = "Buscar ahora".
    pub(super) fn check_updates(&mut self, manual: bool, cx: &mut Context<Self>) {
        if matches!(
            self.update.phase,
            UpdatePhase::Checking | UpdatePhase::Downloading(_) | UpdatePhase::Installing
        ) || !network_allowed()
        {
            return;
        }
        self.update.manual = manual;
        let previous = std::mem::replace(&mut self.update.phase, UpdatePhase::Checking);
        self.update.check = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    kuntur_update::net::fetch_releases(&kuntur_update::releases_url())
                        .map(|list| kuntur_update::choose_update(&list, kuntur_update::CURRENT_VERSION))
                })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.update.phase = match result {
                    Ok(Some(u)) => UpdatePhase::Available(u),
                    Ok(None) => UpdatePhase::UpToDate,
                    Err(e) if w.update.manual => UpdatePhase::Failed(e),
                    // Una búsqueda automática que falla (sin red) no molesta: queda lo anterior.
                    Err(_) => match previous {
                        UpdatePhase::Available(u) => UpdatePhase::Available(u),
                        _ => UpdatePhase::Idle,
                    },
                };
                w.update.check = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// "Actualizar": descarga, verifica y ejecuta el instalador (Windows) o abre la página.
    pub(super) fn start_update(&mut self, cx: &mut Context<Self>) {
        let UpdatePhase::Available(update) = self.update.phase.clone() else {
            return;
        };
        let installer = match update.installer.clone() {
            Some(i) if cfg!(windows) && network_allowed() => i,
            _ => {
                cx.open_url(&update.page);
                return;
            }
        };
        let (cancel, done, total) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(installer.size)),
        );
        self.update.cancel = cancel.clone();
        self.update.done = done.clone();
        self.update.total = total.clone();
        self.update.phase = UpdatePhase::Downloading(update);
        let dir = kuntur_config::dirs::local_dir().join("actualizacion");
        self.update.download = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let path = kuntur_update::net::download(&installer, &dir, &cancel, |n, t| {
                        done.store(n, Ordering::Relaxed);
                        if let Some(t) = t {
                            total.store(t, Ordering::Relaxed);
                        }
                    })
                    .map_err(|e| e.to_string())?;
                    // Firma: la del kuntur.exe en uso decide qué se exige al instalador.
                    let current = std::env::current_exe()
                        .map(|p| signature::inspect(&p))
                        .unwrap_or(Signature::Unsigned);
                    if let Err(e) = signature::check_policy(&current, &signature::inspect(&path)) {
                        let _ = std::fs::remove_file(&path);
                        return Err(e);
                    }
                    Ok::<PathBuf, String>(path)
                })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.update.poll = None;
                match result {
                    Ok(path) => w.run_installer(path, cx),
                    Err(e) => w.update.phase = UpdatePhase::Failed(e),
                }
                cx.notify();
            });
        }));
        self.update.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PROGRESS_TICK).await;
                let downloading = this.update(cx, |w, cx| {
                    cx.notify();
                    matches!(w.update.phase, UpdatePhase::Downloading(_))
                });
                if !downloading.unwrap_or(false) {
                    break;
                }
            }
        }));
        cx.notify();
    }

    /// Ejecuta el instalador verificado y cierra Kuntur (las sesiones quedan guardadas).
    fn run_installer(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            /// Sin ventana de consola para el `cmd` intermedio.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("kuntur.exe"));
            match std::process::Command::new("cmd")
                .raw_arg(installer_command_line(&path, &exe))
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
            {
                Ok(_) => {
                    self.update.phase = UpdatePhase::Installing;
                    self.persist();
                    cx.quit();
                }
                Err(e) => self.update.phase = UpdatePhase::Failed(format!("msiexec: {e}")),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (path, cx);
        }
    }

    /// Texto del estado para Configuración.
    pub(super) fn update_status_text(&self) -> String {
        match &self.update.phase {
            UpdatePhase::Checking => t(T::UpdatesChecking).into(),
            UpdatePhase::UpToDate => t(T::UpdatesUpToDate).into(),
            UpdatePhase::Available(u) => format!("Kuntur {} {}", u.version, t(T::UpdatesAvailable)),
            UpdatePhase::Downloading(u) => self.download_text(u),
            UpdatePhase::Installing => t(T::UpdatesInstalling).into(),
            UpdatePhase::Failed(e) => format!("{}: {e}", t(T::UpdatesFailed)),
            UpdatePhase::Idle => kuntur_update::CURRENT_VERSION.to_string(),
        }
    }

    fn download_text(&self, u: &Update) -> String {
        let done = self.update.done.load(Ordering::Relaxed);
        let total = self.update.total.load(Ordering::Relaxed);
        match percent(done, total) {
            Some(p) => format!("{} Kuntur {}… {p} %", t(T::UpdatesDownloading), u.version),
            None => format!("{} Kuntur {}…", t(T::UpdatesDownloading), u.version),
        }
    }

    /// ¿Hay una versión lista para "Actualizar"?
    pub(super) fn update_available(&self) -> bool {
        matches!(self.update.phase, UpdatePhase::Available(_))
    }

    /// Texto del botón: "Actualizar" si se puede instalar aquí (Windows con instalador
    /// verificable), "Ver la versión" si no, y "Buscar ahora" sin novedades.
    pub(super) fn update_action_label(&self) -> &'static str {
        match &self.update.phase {
            UpdatePhase::Available(u) if cfg!(windows) && u.installer.is_some() => t(T::UpdatesInstall),
            UpdatePhase::Available(_) => t(T::UpdatesOpenPage),
            _ => t(T::UpdatesCheckNow),
        }
    }

    /// Aviso de la línea inferior (nada si no hay novedades).
    pub(super) fn render_update_status(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = &self.colors;
        let hover = c.hover;
        let base = |id: &'static str| {
            h_flex()
                .id(id)
                .flex_shrink_0()
                .gap(px(5.))
                .items_center()
                .px(px(6.))
                .rounded(px(4.))
                .whitespace_nowrap()
        };
        match &self.update.phase {
            UpdatePhase::Available(u) => Some(
                base("update-available")
                    .text_color(c.accent)
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .tooltip(|window, cx| Tooltip::new(t(T::UpdatesTip)).build(window, cx))
                    .on_click(cx.listener(|this, _, _, cx| this.start_update(cx)))
                    .child(Icon::new(IconName::ArrowDown).size(px(12.)))
                    .child(format!(
                        "Kuntur {} {} · {}",
                        u.version,
                        t(T::UpdatesAvailable),
                        self.update_action_label()
                    ))
                    .into_any_element(),
            ),
            UpdatePhase::Downloading(u) => {
                Some(base("update-progress").child(self.download_text(u)).into_any_element())
            }
            UpdatePhase::Installing => Some(
                base("update-installing")
                    .child(t(T::UpdatesInstalling))
                    .into_any_element(),
            ),
            UpdatePhase::Failed(e) => {
                let tip: SharedString = e.clone().into();
                Some(
                    base("update-failed")
                        .text_color(c.warn)
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                        // Clic = descartar el aviso.
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.update.phase = UpdatePhase::Idle;
                            cx.notify();
                        }))
                        .child(t(T::UpdatesFailed))
                        .into_any_element(),
                )
            }
            _ => None,
        }
    }
}

/// Línea de `cmd` que instala y vuelve a abrir Kuntur: espera a `msiexec` (instalación pasiva:
/// solo la barra de progreso; pide permisos de administrador si hace falta) y abre `exe` con los
/// permisos normales del usuario. No se usa una acción del MSI para abrirlo porque, en una
/// instalación por máquina, Kuntur quedaría abierto con permisos elevados.
#[cfg_attr(not(windows), allow(dead_code))]
fn installer_command_line(msi: &std::path::Path, exe: &std::path::Path) -> String {
    format!(
        r#"/d /c start "" /wait msiexec /i "{}" /passive & start "" "{}""#,
        msi.display(),
        exe.display()
    )
}

/// Versión sintética para la prueba de humo: la actual con el menor subido.
fn semver_next() -> kuntur_update::Version {
    let mut v = kuntur_update::release::parse_tag(kuntur_update::CURRENT_VERSION)
        .unwrap_or(kuntur_update::Version::new(0, 0, 0));
    v.minor += 1;
    v.patch = 0;
    v
}

#[cfg(test)]
mod tests {
    use super::{installer_command_line, percent, semver_next};
    use std::path::Path;

    #[test]
    fn instala_y_vuelve_a_abrir_kuntur() {
        let line = installer_command_line(
            Path::new(r"C:\Users\demo\AppData\Local\Kuntur\actualizacion\kuntur-0.2.0-x86_64.msi"),
            Path::new(r"C:\Program Files\Kuntur\kuntur.exe"),
        );
        assert_eq!(
            line,
            r#"/d /c start "" /wait msiexec /i "C:\Users\demo\AppData\Local\Kuntur\actualizacion\kuntur-0.2.0-x86_64.msi" /passive & start "" "C:\Program Files\Kuntur\kuntur.exe""#
        );
    }

    #[test]
    fn porcentaje_de_la_descarga() {
        assert_eq!(percent(0, 0), None);
        assert_eq!(percent(50, 200), Some(25));
        assert_eq!(percent(300, 200), Some(100));
    }

    #[test]
    fn version_sintetica_mayor_que_la_actual() {
        let now = kuntur_update::release::parse_tag(kuntur_update::CURRENT_VERSION).unwrap();
        assert!(semver_next() > now);
    }
}
