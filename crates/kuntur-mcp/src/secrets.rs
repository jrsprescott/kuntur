//! Valores secretos de los servidores MCP (variables de entorno listadas en `secretos`, token de
//! los http): **nunca** en `config.toml`. Se guardan en el Administrador de credenciales de Windows
//! con el servicio «Kuntur MCP» y la cuenta `<servidor>/<VARIABLE>` (igual que `kuntur-ssh`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Servicio con el que aparecen en el Administrador de credenciales.
pub const SERVICE: &str = "Kuntur MCP";

pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Option<String>;
    fn set(&self, account: &str, secret: &str) -> anyhow::Result<()>;
    fn delete(&self, account: &str);
}

#[cfg(windows)]
pub struct WindowsCredentials {
    store: Arc<keyring_core::CredentialStore>,
}

#[cfg(windows)]
impl WindowsCredentials {
    pub fn new() -> anyhow::Result<Self> {
        let store: Arc<keyring_core::CredentialStore> = windows_native_keyring_store::Store::new()?;
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

/// El de Windows si se puede abrir; si no, uno en memoria.
pub fn system_store() -> Arc<dyn SecretStore> {
    #[cfg(windows)]
    if let Ok(w) = WindowsCredentials::new() {
        return Arc::new(w);
    }
    Arc::new(MemorySecrets::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memoria_guarda_lee_y_borra() {
        let s = MemorySecrets::new();
        s.set("db/CLAVE", "x").unwrap();
        assert_eq!(s.get("db/CLAVE").as_deref(), Some("x"));
        s.delete("db/CLAVE");
        assert_eq!(s.get("db/CLAVE"), None);
    }
}
