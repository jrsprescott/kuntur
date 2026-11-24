//! Firma Authenticode del instalador descargado (§7-§8 del plan).
//!
//! Regla ([`check_policy`]):
//! - Si el `kuntur.exe` en uso **está firmado**, el instalador nuevo debe tener una firma válida del
//!   **mismo firmante** (nombre del certificado); si no, no se instala.
//! - Si el `kuntur.exe` en uso **no está firmado** (versiones 0.1–0.2 sin certificado), basta el
//!   SHA-256 publicado (ya verificado al descargar), salvo que el instalador traiga una firma
//!   **rota**: eso nunca se instala.

/// Estado de la firma de un archivo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signature {
    Unsigned,
    /// Firma válida; `signer` = nombre simple del certificado (p. ej. "SignPath Foundation").
    Valid {
        signer: String,
    },
    /// Firma presente pero no válida (código de WinVerifyTrust).
    Invalid(String),
}

/// ¿Se puede instalar `new` estando en uso un ejecutable con la firma `current`?
pub fn check_policy(current: &Signature, new: &Signature) -> Result<(), String> {
    if let Signature::Invalid(why) = new {
        return Err(format!("la firma digital del instalador no es válida ({why})"));
    }
    match (current, new) {
        (Signature::Valid { signer: a }, Signature::Valid { signer: b }) if a.trim().eq_ignore_ascii_case(b.trim()) => {
            Ok(())
        }
        (Signature::Valid { signer }, Signature::Valid { signer: other }) => Err(format!(
            "el instalador está firmado por «{other}» y Kuntur por «{signer}»"
        )),
        (Signature::Valid { signer }, _) => Err(format!("el instalador no está firmado por «{signer}»")),
        _ => Ok(()),
    }
}

/// Firma de un archivo (Windows: WinVerifyTrust). En Linux/Mac no hay Authenticode: `Unsigned`.
pub fn inspect(path: &std::path::Path) -> Signature {
    #[cfg(windows)]
    {
        windows::inspect(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Signature::Unsigned
    }
}

#[cfg(windows)]
mod windows {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Security::Cryptography::{CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW};
    use windows_sys::Win32::Security::WinTrust::{
        WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO, WTD_CHOICE_FILE,
        WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvSignerFromChain,
        WTHelperProvDataFromStateData, WinVerifyTrust,
    };

    use super::Signature;

    /// Archivo sin firma (TRUST_E_NOSIGNATURE) y variantes "no hay firma para este tipo".
    const TRUST_E_NOSIGNATURE: i32 = 0x800B_0100_u32 as i32;
    const TRUST_E_SUBJECT_FORM_UNKNOWN: i32 = 0x800B_0003_u32 as i32;
    const TRUST_E_PROVIDER_UNKNOWN: i32 = 0x800B_0001_u32 as i32;

    pub(super) fn inspect(path: &Path) -> Signature {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: wide.as_ptr(),
            hFile: std::ptr::null_mut(),
            pgKnownSubject: std::ptr::null_mut(),
        };
        // SAFETY: WINTRUST_DATA es una estructura C sin invariantes; todo en cero es válido y se
        // completa abajo.
        let mut data: WINTRUST_DATA = unsafe { std::mem::zeroed() };
        data.cbStruct = size_of::<WINTRUST_DATA>() as u32;
        data.dwUIChoice = WTD_UI_NONE;
        // Sin revocación en línea: no se frena la actualización sin red; el SHA-256 ya se verificó.
        data.fdwRevocationChecks = WTD_REVOKE_NONE;
        data.dwUnionChoice = WTD_CHOICE_FILE;
        data.Anonymous = WINTRUST_DATA_0 { pFile: &mut file };
        data.dwStateAction = WTD_STATEACTION_VERIFY;
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        // SAFETY: `data` y `file` (y la ruta en `wide`) viven durante la llamada; sin ventana (UI_NONE).
        let status = unsafe {
            WinVerifyTrust(
                std::ptr::null_mut(),
                &mut action,
                (&mut data as *mut WINTRUST_DATA).cast(),
            )
        };
        let result = match status {
            0 => Signature::Valid {
                // SAFETY: con VERIFY correcto, `hWVTStateData` es válido hasta el CLOSE de abajo.
                signer: unsafe { signer_name(&data) }.unwrap_or_default(),
            },
            TRUST_E_NOSIGNATURE | TRUST_E_SUBJECT_FORM_UNKNOWN | TRUST_E_PROVIDER_UNKNOWN => Signature::Unsigned,
            other => Signature::Invalid(format!("0x{:08X}", other as u32)),
        };
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        // SAFETY: cierra el estado abierto por la llamada anterior (mismos punteros).
        unsafe {
            WinVerifyTrust(
                std::ptr::null_mut(),
                &mut action,
                (&mut data as *mut WINTRUST_DATA).cast(),
            )
        };
        result
    }

    /// Nombre simple del certificado del primer firmante.
    ///
    /// # Safety
    /// `data.hWVTStateData` debe venir de un `WTD_STATEACTION_VERIFY` correcto y no cerrado.
    unsafe fn signer_name(data: &WINTRUST_DATA) -> Option<String> {
        // SAFETY: lo garantiza quien llama; cada puntero se comprueba antes de usarlo.
        unsafe {
            let prov = WTHelperProvDataFromStateData(data.hWVTStateData);
            if prov.is_null() {
                return None;
            }
            let sgnr = WTHelperGetProvSignerFromChain(prov, 0, 0, 0);
            if sgnr.is_null() || (*sgnr).csCertChain == 0 || (*sgnr).pasCertChain.is_null() {
                return None;
            }
            let cert = (*(*sgnr).pasCertChain).pCert;
            if cert.is_null() {
                return None;
            }
            let mut buf = [0u16; 256];
            let n = CertGetNameStringW(
                cert,
                CERT_NAME_SIMPLE_DISPLAY_TYPE,
                0,
                std::ptr::null(),
                buf.as_mut_ptr(),
                buf.len() as u32,
            );
            // `n` cuenta el 0 final.
            (n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid(s: &str) -> Signature {
        Signature::Valid { signer: s.into() }
    }

    #[test]
    fn sin_firma_en_uso_basta_el_sha256() {
        assert_eq!(check_policy(&Signature::Unsigned, &Signature::Unsigned), Ok(()));
        assert_eq!(
            check_policy(&Signature::Unsigned, &valid("SignPath Foundation")),
            Ok(())
        );
    }

    #[test]
    fn firmado_en_uso_exige_el_mismo_firmante() {
        let actual = valid("SignPath Foundation");
        assert_eq!(check_policy(&actual, &valid("signpath foundation ")), Ok(()));
        assert!(check_policy(&actual, &valid("Otra Empresa")).is_err());
        assert!(check_policy(&actual, &Signature::Unsigned).is_err());
    }

    #[test]
    fn una_firma_rota_nunca_se_instala() {
        let rota = Signature::Invalid("0x80096010".into());
        assert!(check_policy(&Signature::Unsigned, &rota).is_err());
        assert!(check_policy(&valid("X"), &rota).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn un_archivo_de_texto_no_esta_firmado() {
        let p = std::env::temp_dir().join(format!("kuntur-firma-{}.txt", std::process::id()));
        std::fs::write(&p, "sin firma").unwrap();
        assert_eq!(inspect(&p), Signature::Unsigned);
        let _ = std::fs::remove_file(&p);
    }

    #[cfg(windows)]
    #[test]
    fn un_ejecutable_de_windows_esta_firmado_por_microsoft() {
        // notepad.exe viene firmado por catálogo en algunas ediciones; kernel32.dll siempre tiene
        // firma incrustada.
        let dll = std::path::Path::new(r"C:\Windows\System32\kernel32.dll");
        if dll.exists() {
            let sig = inspect(dll);
            eprintln!("kernel32.dll: {sig:?}");
            match sig {
                Signature::Valid { signer } => assert!(signer.contains("Microsoft"), "firmante: {signer}"),
                // Algunas imágenes de CI usan firma por catálogo: no es un error de Kuntur.
                Signature::Unsigned => {}
                other => panic!("inesperado: {other:?}"),
            }
        }
    }
}
