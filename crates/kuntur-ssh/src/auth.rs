//! A quién se conecta y en qué orden se prueba la autenticación. Sin red: se prueba aparte.

use std::path::{Path, PathBuf};

/// Servidor y usuario de una sesión remota (lo que sale de una conexión guardada).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshTarget {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// Llave elegida en el gestor; `None` = las de siempre de `~/.ssh`.
    pub key: Option<PathBuf>,
    /// Guardar contraseñas y passphrases en el Administrador de credenciales de Windows.
    pub remember: bool,
}

impl SshTarget {
    /// `usuario@host` (y `:puerto` si no es el 22).
    pub fn destination(&self) -> String {
        if self.port == 22 {
            format!("{}@{}", self.user, self.host)
        } else {
            format!("{}@{}:{}", self.user, self.host, self.port)
        }
    }
}

/// Un intento de autenticación.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthStep {
    /// Llaves del ssh-agent de Windows (servicio OpenSSH Authentication Agent).
    Agent,
    /// Una llave privada en un archivo (si está cifrada, se pide la passphrase).
    KeyFile(PathBuf),
    /// Contraseña (el último recurso).
    Password,
}

/// Llaves que `ssh` prueba por defecto, en su orden.
pub const DEFAULT_KEYS: [&str; 3] = ["id_ed25519", "id_ecdsa", "id_rsa"];

/// Orden de los intentos: agente → llave del gestor (o las de `~/.ssh` que existan) → contraseña.
/// `exists` dice si un archivo existe (en las pruebas, sin tocar el disco).
pub fn auth_plan(target: &SshTarget, ssh_dir: Option<&Path>, exists: impl Fn(&Path) -> bool) -> Vec<AuthStep> {
    let mut steps = vec![AuthStep::Agent];
    match &target.key {
        Some(key) => steps.push(AuthStep::KeyFile(key.clone())),
        None => {
            if let Some(dir) = ssh_dir {
                steps.extend(
                    DEFAULT_KEYS
                        .iter()
                        .map(|name| dir.join(name))
                        .filter(|p| exists(p))
                        .map(AuthStep::KeyFile),
                );
            }
        }
    }
    steps.push(AuthStep::Password);
    steps
}

/// Nombre de la credencial de una contraseña en el Administrador de credenciales.
pub fn password_account(target: &SshTarget) -> String {
    format!("contraseña:{}@{}:{}", target.user, target.host, target.port)
}

/// Nombre de la credencial de la passphrase de una llave.
pub fn passphrase_account(key: &Path) -> String {
    format!("passphrase:{}", key.display())
}

/// Ruta para mostrar: `~\.ssh\id_ed25519` en vez de la ruta completa del usuario.
pub fn display_key(key: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| key.strip_prefix(h).ok()) {
        Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        None => key.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(key: Option<&str>) -> SshTarget {
        SshTarget {
            host: "srv.test".into(),
            port: 22,
            user: "deploy".into(),
            key: key.map(PathBuf::from),
            remember: false,
        }
    }

    #[test]
    fn con_llave_elegida() {
        let plan = auth_plan(&target(Some("C:/llaves/mia")), Some(Path::new("C:/h/.ssh")), |_| true);
        assert_eq!(
            plan,
            vec![
                AuthStep::Agent,
                AuthStep::KeyFile("C:/llaves/mia".into()),
                AuthStep::Password
            ]
        );
    }

    #[test]
    fn sin_llave_prueba_las_de_ssh_que_existan() {
        let dir = Path::new("C:/h/.ssh");
        let plan = auth_plan(&target(None), Some(dir), |p| !p.ends_with("id_ecdsa"));
        assert_eq!(
            plan,
            vec![
                AuthStep::Agent,
                AuthStep::KeyFile(dir.join("id_ed25519")),
                AuthStep::KeyFile(dir.join("id_rsa")),
                AuthStep::Password
            ]
        );
        assert_eq!(
            auth_plan(&target(None), None, |_| true),
            vec![AuthStep::Agent, AuthStep::Password]
        );
    }

    #[test]
    fn destino_y_credenciales() {
        let mut t = target(None);
        assert_eq!(t.destination(), "deploy@srv.test");
        t.port = 2222;
        assert_eq!(t.destination(), "deploy@srv.test:2222");
        assert_eq!(password_account(&t), "contraseña:deploy@srv.test:2222");
        assert_eq!(passphrase_account(Path::new("C:/k/id")), "passphrase:C:/k/id");
    }

    #[test]
    fn llave_para_mostrar() {
        let home = Path::new("C:/Users/demo");
        assert_eq!(
            display_key(Path::new("C:/Users/demo/.ssh/id_rsa"), Some(home)),
            format!("~{}.ssh/id_rsa", std::path::MAIN_SEPARATOR)
        );
        assert_eq!(display_key(Path::new("D:/k/id"), Some(home)), "D:/k/id");
    }
}
