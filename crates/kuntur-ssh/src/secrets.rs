//! Contraseñas y passphrases: **nunca** en `config.toml`. Se guardan en el Administrador de
//! credenciales de Windows y solo cuando la conexión tiene "Recordar" marcado.

use std::collections::HashMap;
use std::sync::Mutex;

/// Servicio con el que aparecen en el Administrador de credenciales.
pub const SERVICE: &str = "Kuntur SSH";

/// Dónde se guardan los secretos recordados.
pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Option<String>;
    fn set(&self, account: &str, secret: &str) -> anyhow::Result<()>;
    fn delete(&self, account: &str);
}

/// Administrador de credenciales de Windows (vía `keyring-core`).
#[cfg(windows)]
pub struct WindowsCredentials {
    store: std::sync::Arc<keyring_core::CredentialStore>,
}

#[cfg(windows)]
impl WindowsCredentials {
    pub fn new() -> anyhow::Result<Self> {
        let store: std::sync::Arc<keyring_core::CredentialStore> = windows_native_keyring_store::Store::new()?;
        Ok(Self { store })
    }

    fn entry(&self, account: &str) -> Option<keyring_core::Entry> {
        keyring_core::Entry::new_in_store(&self.store, SERVICE, account).ok()
    }
}

#[cfg(windows)]
impl SecretStore for WindowsCredentials {
    fn get(&self, account: &str) -> Option<String> {
        self.entry(account)?.get_password().ok()
    }

    fn set(&self, account: &str, secret: &str) -> anyhow::Result<()> {
        let entry = self
            .entry(account)
            .ok_or_else(|| anyhow::anyhow!("no se pudo crear la credencial"))?;
        entry.set_password(secret)?;
        Ok(())
    }

    fn delete(&self, account: &str) {
        if let Some(e) = self.entry(account) {
            let _ = e.delete_credential();
        }
    }
}

/// En memoria: pruebas y pruebas de humo (nunca toca el Administrador real).
#[derive(Default)]
pub struct MemorySecrets {
    map: Mutex<HashMap<String, String>>,
}

impl MemorySecrets {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.map.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SecretStore for MemorySecrets {
    fn get(&self, account: &str) -> Option<String> {
        self.map.lock().unwrap().get(account).cloned()
    }

    fn set(&self, account: &str, secret: &str) -> anyhow::Result<()> {
        self.map.lock().unwrap().insert(account.into(), secret.into());
        Ok(())
    }

    fn delete(&self, account: &str) {
        self.map.lock().unwrap().remove(account);
    }
}

/// El de Windows si se puede abrir; si no, uno en memoria (no se recuerda nada entre arranques).
pub fn system_store() -> std::sync::Arc<dyn SecretStore> {
    #[cfg(windows)]
    if let Ok(w) = WindowsCredentials::new() {
        return std::sync::Arc::new(w);
    }
    std::sync::Arc::new(MemorySecrets::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memoria_guarda_lee_y_borra() {
        let s = MemorySecrets::new();
        assert_eq!(s.get("a"), None);
        s.set("a", "x").unwrap();
        assert_eq!(s.get("a").as_deref(), Some("x"));
        s.delete("a");
        assert!(s.is_empty());
    }
}
