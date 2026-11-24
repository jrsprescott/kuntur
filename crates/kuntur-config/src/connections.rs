//! Conexiones SSH guardadas (§8 de la especificación): nombre, host, puerto, usuario y ruta de la
//! llave. **Nunca** contraseñas ni passphrases: esas van al Administrador de credenciales de
//! Windows, y solo si `remember` está marcado.

use serde::{Deserialize, Serialize};

pub const DEFAULT_PORT: u16 = 22;
/// Largo máximo del nombre de una conexión.
pub const MAX_CONNECTION_NAME: usize = 40;

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn is_default_port(port: &u16) -> bool {
    *port == DEFAULT_PORT
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Nombre visible y clave de la conexión (las sesiones la guardan por nombre).
    pub name: String,
    pub host: String,
    #[serde(default = "default_port", skip_serializing_if = "is_default_port")]
    pub port: u16,
    pub user: String,
    /// Ruta de la llave privada; `None` = la del agente o las de `~/.ssh` (`id_ed25519`…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Recordar la contraseña o la passphrase en el Administrador de credenciales de Windows.
    #[serde(default, skip_serializing_if = "is_false")]
    pub remember: bool,
}

/// Por qué una conexión no se puede guardar (el texto se muestra tal cual en el formulario).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionError {
    EmptyName,
    NameTooLong,
    DuplicateName,
    EmptyHost,
    InvalidHost,
    EmptyUser,
    InvalidUser,
    InvalidPort,
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::EmptyName => "Falta el nombre",
            Self::NameTooLong => "El nombre es demasiado largo",
            Self::DuplicateName => "Ya hay una conexión con ese nombre",
            Self::EmptyHost => "Falta el host",
            Self::InvalidHost => "El host no puede tener espacios, «@» ni «/»",
            Self::EmptyUser => "Falta el usuario",
            Self::InvalidUser => "El usuario no puede tener espacios ni «@»",
            Self::InvalidPort => "El puerto va de 1 a 65535",
        })
    }
}

impl std::error::Error for ConnectionError {}

impl ConnectionConfig {
    /// `usuario@host` (y `:puerto` si no es el 22).
    pub fn destination(&self) -> String {
        if self.port == DEFAULT_PORT {
            format!("{}@{}", self.user, self.host)
        } else {
            format!("{}@{}:{}", self.user, self.host, self.port)
        }
    }

    /// Revisa los campos. `others` son los nombres de las demás conexiones (sin esta).
    pub fn validate<'a>(&self, others: impl IntoIterator<Item = &'a str>) -> Result<(), ConnectionError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(ConnectionError::EmptyName);
        }
        if name.chars().count() > MAX_CONNECTION_NAME {
            return Err(ConnectionError::NameTooLong);
        }
        if others.into_iter().any(|o| o.trim().eq_ignore_ascii_case(name)) {
            return Err(ConnectionError::DuplicateName);
        }
        let host = self.host.trim();
        if host.is_empty() {
            return Err(ConnectionError::EmptyHost);
        }
        if host
            .chars()
            .any(|c| c.is_whitespace() || c == '@' || c == '/' || c.is_control())
        {
            return Err(ConnectionError::InvalidHost);
        }
        let user = self.user.trim();
        if user.is_empty() {
            return Err(ConnectionError::EmptyUser);
        }
        if user.chars().any(|c| c.is_whitespace() || c == '@' || c.is_control()) {
            return Err(ConnectionError::InvalidUser);
        }
        if self.port == 0 {
            return Err(ConnectionError::InvalidPort);
        }
        Ok(())
    }

    /// Copia sin espacios sobrantes y sin llave vacía (lo que se guarda).
    pub fn normalized(&self) -> ConnectionConfig {
        ConnectionConfig {
            name: self.name.trim().to_string(),
            host: self
                .host
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string(),
            port: self.port,
            user: self.user.trim().to_string(),
            key: self
                .key
                .as_deref()
                .map(|k| k.trim().trim_matches('"').to_string())
                .filter(|k| !k.is_empty()),
            remember: self.remember,
        }
    }
}

/// Puerto escrito en el formulario: vacío = 22.
pub fn parse_port(text: &str) -> Result<u16, ConnectionError> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(DEFAULT_PORT);
    }
    match t.parse::<u16>() {
        Ok(0) | Err(_) => Err(ConnectionError::InvalidPort),
        Ok(p) => Ok(p),
    }
}

/// Destino al estilo `ssh`: `usuario@host`, `usuario@host:2222`, `host`, `usuario@[::1]:22`.
/// Devuelve (usuario, host, puerto); lo que falta queda en `None`.
pub fn parse_destination(text: &str) -> Result<(Option<String>, String, Option<u16>), ConnectionError> {
    let t = text.trim();
    let (user, rest) = match t.rsplit_once('@') {
        Some((u, r)) => (Some(u.to_string()).filter(|u| !u.is_empty()), r),
        None => (None, t),
    };
    let (host, port) = if let Some(inner) = rest.strip_prefix('[') {
        // IPv6 entre corchetes, con puerto opcional.
        let (h, after) = inner.split_once(']').ok_or(ConnectionError::InvalidHost)?;
        let port = match after.strip_prefix(':') {
            Some(p) => Some(parse_port(p)?),
            None if after.is_empty() => None,
            None => return Err(ConnectionError::InvalidHost),
        };
        (h.to_string(), port)
    } else {
        match rest.split_once(':') {
            // Más de un ":" sin corchetes: IPv6 sin puerto.
            Some((h, p)) if !p.contains(':') => (h.to_string(), Some(parse_port(p)?)),
            _ => (rest.to_string(), None),
        }
    };
    if host.is_empty() {
        return Err(ConnectionError::EmptyHost);
    }
    Ok((user, host, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(name: &str, host: &str, user: &str) -> ConnectionConfig {
        ConnectionConfig {
            name: name.into(),
            host: host.into(),
            port: 22,
            user: user.into(),
            key: None,
            remember: false,
        }
    }

    #[test]
    fn destino_con_y_sin_puerto() {
        assert_eq!(
            conn("a", "srv.ejemplo.test", "deploy").destination(),
            "deploy@srv.ejemplo.test"
        );
        let mut c = conn("a", "10.0.0.5", "root");
        c.port = 2222;
        assert_eq!(c.destination(), "root@10.0.0.5:2222");
    }

    #[test]
    fn valida_los_campos() {
        assert_eq!(conn("web", "h.test", "u").validate([]), Ok(()));
        assert_eq!(conn(" ", "h.test", "u").validate([]), Err(ConnectionError::EmptyName));
        assert_eq!(
            conn(&"x".repeat(41), "h.test", "u").validate([]),
            Err(ConnectionError::NameTooLong)
        );
        assert_eq!(
            conn("Web", "h.test", "u").validate(["api", "web "]),
            Err(ConnectionError::DuplicateName)
        );
        assert_eq!(conn("web", "", "u").validate([]), Err(ConnectionError::EmptyHost));
        assert_eq!(conn("web", "u@h", "u").validate([]), Err(ConnectionError::InvalidHost));
        assert_eq!(conn("web", "h h", "u").validate([]), Err(ConnectionError::InvalidHost));
        assert_eq!(conn("web", "h.test", "").validate([]), Err(ConnectionError::EmptyUser));
        assert_eq!(
            conn("web", "h.test", "a b").validate([]),
            Err(ConnectionError::InvalidUser)
        );
        let mut c = conn("web", "h.test", "u");
        c.port = 0;
        assert_eq!(c.validate([]), Err(ConnectionError::InvalidPort));
        assert_eq!(
            ConnectionError::DuplicateName.to_string(),
            "Ya hay una conexión con ese nombre"
        );
    }

    #[test]
    fn normaliza_lo_que_se_guarda() {
        let c = ConnectionConfig {
            name: "  web ".into(),
            host: " [::1] ".into(),
            port: 22,
            user: " deploy ".into(),
            key: Some("  \"C:\\llaves\\id_ed25519\" ".into()),
            remember: true,
        }
        .normalized();
        assert_eq!(c.name, "web");
        assert_eq!(c.host, "::1");
        assert_eq!(c.user, "deploy");
        assert_eq!(c.key.as_deref(), Some("C:\\llaves\\id_ed25519"));
        let vacia = ConnectionConfig {
            key: Some("   ".into()),
            ..conn("a", "h", "u")
        };
        assert_eq!(vacia.normalized().key, None);
    }

    #[test]
    fn puerto_del_formulario() {
        assert_eq!(parse_port(""), Ok(22));
        assert_eq!(parse_port(" 2222 "), Ok(2222));
        assert_eq!(parse_port("0"), Err(ConnectionError::InvalidPort));
        assert_eq!(parse_port("70000"), Err(ConnectionError::InvalidPort));
        assert_eq!(parse_port("abc"), Err(ConnectionError::InvalidPort));
    }

    #[test]
    fn destinos_al_estilo_ssh() {
        assert_eq!(
            parse_destination("deploy@srv.test:2222"),
            Ok((Some("deploy".into()), "srv.test".into(), Some(2222)))
        );
        assert_eq!(parse_destination("srv.test"), Ok((None, "srv.test".into(), None)));
        assert_eq!(
            parse_destination("root@[::1]:22"),
            Ok((Some("root".into()), "::1".into(), Some(22)))
        );
        assert_eq!(parse_destination("fe80::1"), Ok((None, "fe80::1".into(), None)));
        assert_eq!(parse_destination("u@"), Err(ConnectionError::EmptyHost));
        assert_eq!(parse_destination("u@h:x"), Err(ConnectionError::InvalidPort));
    }

    #[test]
    fn toml_minimo_y_sin_campos_por_defecto() {
        let c: ConnectionConfig = toml::from_str("name = \"web\"\nhost = \"h.test\"\nuser = \"u\"\n").unwrap();
        assert_eq!(c, conn("web", "h.test", "u"));
        let text = toml::to_string(&c).unwrap();
        assert!(!text.contains("port"));
        assert!(!text.contains("remember"));
        assert!(!text.contains("key"));
    }
}
