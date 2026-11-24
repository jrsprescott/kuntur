//! Notificaciones de Windows (toasts) de las alertas (§12).
//!
//! Nunca se muestran en las pruebas de humo (cualquier variable `KUNTUR_SMOKE_*`) ni desde las
//! pruebas (`cfg(test)`): el usuario puede estar usando la pantalla principal (por ejemplo, jugando).
//! Se pueden apagar con `alertas = false` en `config.toml`.
//!
//! Origen del toast (F11): el instalador MSI registra el AppUserModelID [`APP_ID`] (clave
//! [`APP_ID_KEY`], con nombre e ícono) y lo pone en el acceso del menú Inicio. Si está registrado,
//! los toasts dicen "Kuntur". Sin instalar (compilado a mano) Windows no muestra toasts de un AUMID
//! desconocido: se usa el de PowerShell, que siempre existe, y el toast dice "Windows PowerShell".
//! La app nunca escribe ese registro por su cuenta.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use kuntur_agents::AlertKind;
use kuntur_config::NotificationSettings;

/// AppUserModelID de Kuntur: el mismo de `wix/main.wxs` (acceso directo y registro).
pub const APP_ID: &str = "Kuntur.Kuntur";
/// Nombre que muestra Windows para ese AUMID.
pub const APP_NAME: &str = "Kuntur";
/// AUMID de PowerShell (lo trae Windows): origen de los toasts si Kuntur no está instalado.
const POWERSHELL_APP_ID: &str = "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe";
/// Clave del registro (bajo HKLM, o HKCU) que crea el instalador.
const APP_ID_KEY: &str = "Software\\Classes\\AppUserModelId\\Kuntur.Kuntur";

/// ¿Se pueden mostrar toasts? `alerts` = `alertas` de la configuración; `env_names` = nombres de las
/// variables de entorno del proceso.
pub fn toasts_allowed<I, S>(alerts: bool, env_names: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    alerts && !env_names.into_iter().any(|n| n.as_ref().starts_with("KUNTUR_SMOKE_"))
}

/// Lo mismo con el entorno real del proceso.
pub fn toasts_allowed_now(alerts: bool) -> bool {
    toasts_allowed(
        alerts,
        std::env::vars_os().map(|(k, _)| k.to_string_lossy().into_owned()),
    )
}

/// Decide qué alertas además muestran la notificación del sistema (ajustes v0.2): por tipo, solo
/// con Kuntur en segundo plano y como mucho una por sesión cada `interval_min` minutos. Lo que no
/// pasa solo suma a la insignia de la pestaña.
#[derive(Debug, Default)]
pub struct ToastGate {
    /// Última notificación mostrada de cada sesión.
    last: HashMap<u64, Instant>,
}

impl ToastGate {
    /// ¿Se muestra la notificación de esta alerta? Si sí, cuenta para el tiempo mínimo.
    pub fn allow(
        &mut self,
        session: u64,
        kind: AlertKind,
        window_active: bool,
        prefs: &NotificationSettings,
        now: Instant,
    ) -> bool {
        let wanted = match kind {
            AlertKind::Waiting => prefs.waiting,
            AlertKind::Permission => prefs.permission,
            AlertKind::Finished => prefs.finished,
        };
        if !wanted || (prefs.only_background && window_active) {
            return false;
        }
        let gap = Duration::from_secs(u64::from(prefs.interval_min) * 60);
        if let Some(&last) = self.last.get(&session)
            && now.saturating_duration_since(last) < gap
        {
            return false;
        }
        self.last.insert(session, now);
        true
    }

    /// Olvida las sesiones cerradas.
    pub fn retain(&mut self, open: impl Fn(u64) -> bool) {
        self.last.retain(|&id, _| open(id));
    }
}

/// AUMID con el que salen los toasts: el propio solo si el instalador lo registró.
pub fn toast_app_id(registered: bool) -> &'static str {
    if registered { APP_ID } else { POWERSHELL_APP_ID }
}

/// ¿Registró el instalador el AUMID de Kuntur? Se consulta una sola vez y solo se lee el registro.
pub fn app_id_registered() -> bool {
    static REGISTERED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *REGISTERED.get_or_init(registry_key_exists)
}

#[cfg(windows)]
fn registry_key_exists() -> bool {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, RegCloseKey, RegOpenKeyExW,
    };
    let name: Vec<u16> = APP_ID_KEY.encode_utf16().chain(std::iter::once(0)).collect();
    [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER].into_iter().any(|root| {
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: `name` termina en 0 y vive durante la llamada; `key` se cierra si se abrió.
        let ok = unsafe { RegOpenKeyExW(root, name.as_ptr(), 0, KEY_READ, &mut key) } == ERROR_SUCCESS;
        if ok {
            // SAFETY: `key` es la clave que abrió la llamada anterior.
            unsafe { RegCloseKey(key) };
        }
        ok
    })
}

#[cfg(not(windows))]
fn registry_key_exists() -> bool {
    false
}

/// Muestra el toast en un hilo aparte (las llamadas WinRT no deben frenar la UI). Quien llama ya
/// comprobó `toasts_allowed_now`.
pub fn show_toast(title: String, body: String) {
    if cfg!(test) {
        return;
    }
    #[cfg(windows)]
    std::thread::spawn(move || {
        use tauri_winrt_notification::{Duration, Toast};
        let _ = Toast::new(toast_app_id(app_id_registered()))
            .title(&title)
            .text1(&body)
            .duration(Duration::Short)
            .show();
    });
    #[cfg(not(windows))]
    let _ = (title, body);
}

#[cfg(test)]
mod tests {
    // Importes explícitos: `super::*` no hace falta y en este crate puede traer la macro `test` de GPUI.
    use super::{APP_ID, APP_ID_KEY, POWERSHELL_APP_ID, ToastGate, toast_app_id, toasts_allowed};
    use kuntur_agents::AlertKind;
    use kuntur_config::NotificationSettings;
    use std::time::{Duration, Instant};

    #[test]
    fn una_notificacion_por_sesion_cada_intervalo() {
        let prefs = NotificationSettings::default(); // 5 min, solo en segundo plano
        let mut gate = ToastGate::default();
        let t0 = Instant::now();
        assert!(gate.allow(1, AlertKind::Waiting, false, &prefs, t0));
        // Otra alerta de la misma sesión antes de 5 min: solo insignia.
        assert!(!gate.allow(1, AlertKind::Permission, false, &prefs, t0 + Duration::from_secs(60)));
        // Otra sesión no comparte el tiempo.
        assert!(gate.allow(2, AlertKind::Waiting, false, &prefs, t0 + Duration::from_secs(60)));
        // Pasados los 5 min, vuelve a avisar.
        assert!(gate.allow(1, AlertKind::Finished, false, &prefs, t0 + Duration::from_secs(301)));
    }

    #[test]
    fn sin_limite_avisa_siempre() {
        let prefs = NotificationSettings {
            interval_min: 0,
            ..Default::default()
        };
        let mut gate = ToastGate::default();
        let t0 = Instant::now();
        assert!(gate.allow(1, AlertKind::Waiting, false, &prefs, t0));
        assert!(gate.allow(1, AlertKind::Waiting, false, &prefs, t0));
    }

    #[test]
    fn con_kuntur_al_frente_o_tipo_apagado_no_avisa() {
        let mut gate = ToastGate::default();
        let t0 = Instant::now();
        let prefs = NotificationSettings::default();
        assert!(!gate.allow(1, AlertKind::Waiting, true, &prefs, t0));
        // Lo que no se mostró no cuenta para el intervalo.
        assert!(gate.allow(1, AlertKind::Waiting, false, &prefs, t0));

        let todo = NotificationSettings {
            only_background: false,
            interval_min: 0,
            waiting: false,
            ..Default::default()
        };
        assert!(gate.allow(3, AlertKind::Permission, true, &todo, t0));
        assert!(!gate.allow(3, AlertKind::Waiting, true, &todo, t0));
    }

    #[test]
    fn olvida_las_sesiones_cerradas() {
        let prefs = NotificationSettings::default();
        let mut gate = ToastGate::default();
        let t0 = Instant::now();
        assert!(gate.allow(1, AlertKind::Waiting, false, &prefs, t0));
        gate.retain(|id| id != 1);
        assert!(gate.allow(1, AlertKind::Waiting, false, &prefs, t0));
    }

    #[test]
    fn humo_o_configuracion_apagan_los_toasts() {
        assert!(toasts_allowed(true, ["PATH", "USERPROFILE"]));
        assert!(!toasts_allowed(false, ["PATH"]));
        assert!(!toasts_allowed(true, ["PATH", "KUNTUR_SMOKE_AUDIT"]));
        assert!(!toasts_allowed(true, ["KUNTUR_SMOKE_INPUT"]));
        // Otras variables de Kuntur no cuentan.
        assert!(toasts_allowed(true, ["KUNTUR_CONFIG"]));
    }

    #[test]
    fn aumid_propio_solo_si_el_instalador_lo_registro() {
        assert_eq!(toast_app_id(true), "Kuntur.Kuntur");
        assert_eq!(toast_app_id(false), POWERSHELL_APP_ID);
        #[cfg(windows)]
        assert_eq!(POWERSHELL_APP_ID, tauri_winrt_notification::Toast::POWERSHELL_APP_ID);
        // El instalador usa el mismo AUMID en el acceso directo y en el registro.
        let wxs = include_str!("../wix/main.wxs");
        assert!(wxs.contains(&format!("Value='{APP_ID}'")));
        assert!(wxs.contains(&format!("Key='{APP_ID_KEY}'")));
    }
}
