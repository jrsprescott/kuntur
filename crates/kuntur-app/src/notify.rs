//! Notificaciones de Windows (toasts) de las alertas (§12).
//!
//! Nunca se muestran en las pruebas de humo (cualquier variable `KUNTUR_SMOKE_*`) ni desde las
//! pruebas (`cfg(test)`): el usuario puede estar usando la pantalla principal (por ejemplo, jugando).
//! Se pueden apagar con `alertas = false` en `config.toml`.
//!
//! Sin instalador todavía no hay un AppUserModelID propio registrado (llega con el MSI de F11): se usa
//! el de PowerShell, que Windows acepta en apps sin empaquetar. Por eso el toast dice "Windows
//! PowerShell" como origen.

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

/// Muestra el toast en un hilo aparte (las llamadas WinRT no deben frenar la UI). Quien llama ya
/// comprobó `toasts_allowed_now`.
pub fn show_toast(title: String, body: String) {
    if cfg!(test) {
        return;
    }
    #[cfg(windows)]
    std::thread::spawn(move || {
        use tauri_winrt_notification::{Duration, Toast};
        let _ = Toast::new(Toast::POWERSHELL_APP_ID)
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
    use super::*;

    #[test]
    fn humo_o_configuracion_apagan_los_toasts() {
        assert!(toasts_allowed(true, ["PATH", "USERPROFILE"]));
        assert!(!toasts_allowed(false, ["PATH"]));
        assert!(!toasts_allowed(true, ["PATH", "KUNTUR_SMOKE_AUDIT"]));
        assert!(!toasts_allowed(true, ["KUNTUR_SMOKE_INPUT"]));
        // Otras variables de Kuntur no cuentan.
        assert!(toasts_allowed(true, ["KUNTUR_CONFIG"]));
    }
}
