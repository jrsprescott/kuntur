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
    use super::{APP_ID, APP_ID_KEY, POWERSHELL_APP_ID, toast_app_id, toasts_allowed};

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
