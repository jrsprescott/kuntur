//! Catálogo de servidores MCP del usuario: validación, lista activa de cada sesión, perfiles y
//! resolución (secretos + carpeta de salida) antes de generar la configuración de un CLI.

use std::collections::BTreeMap;
use std::path::Path;

use kuntur_config::{McpProfileConfig, McpServerConfig, McpTransport};

use crate::secrets::SecretStore;

/// Largo máximo del nombre de un servidor.
pub const MAX_NAME: usize = 40;
/// En argumentos y valores del entorno se reemplaza por la carpeta de salida de la sesión.
pub const OUTPUT_PLACEHOLDER: &str = "${KUNTUR_SALIDA}";
/// Variable que reciben todos los servidores stdio con la carpeta de salida de la sesión.
pub const OUTPUT_ENV: &str = "KUNTUR_MCP_SALIDA";
/// Cuenta del token *bearer* de un servidor http (no puede chocar con un nombre de variable).
pub const TOKEN_KEY: &str = "#token";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpError {
    EmptyName,
    NameTooLong,
    InvalidName,
    DuplicateName,
    MissingCommand,
    MissingUrl,
    InvalidUrl,
    InvalidEnvName(String),
    SecretInEnv(String),
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyName => f.write_str("Falta el nombre"),
            Self::NameTooLong => f.write_str("El nombre es demasiado largo"),
            Self::InvalidName => f.write_str("El nombre solo puede tener letras, números, «-» y «_»"),
            Self::DuplicateName => f.write_str("Ya hay un servidor MCP con ese nombre"),
            Self::MissingCommand => f.write_str("Falta el comando del servidor"),
            Self::MissingUrl => f.write_str("Falta la URL del servidor"),
            Self::InvalidUrl => f.write_str("La URL debe empezar con http:// o https://"),
            Self::InvalidEnvName(n) => write!(f, "Nombre de variable no válido: «{n}»"),
            Self::SecretInEnv(n) => write!(f, "«{n}» es secreta: no puede tener valor en «entorno»"),
        }
    }
}

impl std::error::Error for McpError {}

fn simple_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn env_name(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with(|c: char| c.is_ascii_digit())
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Revisa un servidor. `others` son los nombres de los demás (sin este).
pub fn validate_server<'a>(s: &McpServerConfig, others: impl IntoIterator<Item = &'a str>) -> Result<(), McpError> {
    let name = s.name.trim();
    if name.is_empty() {
        return Err(McpError::EmptyName);
    }
    if name.chars().count() > MAX_NAME {
        return Err(McpError::NameTooLong);
    }
    // El nombre va como clave en la configuración de cada CLI (`mcp_servers.<nombre>` en Codex).
    if !simple_name(name) {
        return Err(McpError::InvalidName);
    }
    if others.into_iter().any(|o| o.eq_ignore_ascii_case(name)) {
        return Err(McpError::DuplicateName);
    }
    match s.transport {
        McpTransport::Stdio => {
            if s.command.trim().is_empty() {
                return Err(McpError::MissingCommand);
            }
            for k in s.env.keys().chain(s.secrets.iter()) {
                if !env_name(k) {
                    return Err(McpError::InvalidEnvName(k.clone()));
                }
            }
            if let Some(k) = s.secrets.iter().find(|k| s.env.contains_key(*k)) {
                return Err(McpError::SecretInEnv(k.clone()));
            }
        }
        McpTransport::Http => {
            let url = s.url.trim();
            if url.is_empty() {
                return Err(McpError::MissingUrl);
            }
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(McpError::InvalidUrl);
            }
        }
    }
    Ok(())
}

/// Cuenta en el Administrador de credenciales: `<servidor>/<VARIABLE>` (o `<servidor>/#token`).
pub fn secret_account(server: &str, key: &str) -> String {
    format!("{server}/{key}")
}

/// Secretos que el servidor necesita y todavía no están guardados.
pub fn missing_secrets(s: &McpServerConfig, store: &dyn SecretStore) -> Vec<String> {
    let mut keys: Vec<String> = s.secrets.clone();
    if s.transport == McpTransport::Http && s.token {
        keys = vec![TOKEN_KEY.to_string()];
    }
    keys.into_iter()
        .filter(|k| store.get(&secret_account(&s.name, k)).is_none())
        .collect()
}

/// Servidor listo para escribir en la configuración de un CLI o para "Probar".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedServer {
    pub name: String,
    pub kind: ResolvedKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedKind {
    Stdio {
        command: String,
        args: Vec<String>,
        /// Variables visibles (incluye `KUNTUR_MCP_SALIDA` si la sesión tiene carpeta de salida).
        env: BTreeMap<String, String>,
        /// Variables secretas con su valor (las que faltan no se incluyen).
        secret_env: BTreeMap<String, String>,
    },
    Http {
        url: String,
        headers: BTreeMap<String, String>,
        /// Token *bearer* (`Authorization: Bearer …`), si está guardado.
        token: Option<String>,
    },
}

impl ResolvedServer {
    pub fn has_secrets(&self) -> bool {
        match &self.kind {
            ResolvedKind::Stdio { secret_env, .. } => !secret_env.is_empty(),
            ResolvedKind::Http { token, .. } => token.is_some(),
        }
    }
}

fn expand(text: &str, output: Option<&Path>) -> String {
    match output {
        Some(dir) if text.contains(OUTPUT_PLACEHOLDER) => text.replace(OUTPUT_PLACEHOLDER, &dir.to_string_lossy()),
        _ => text.to_string(),
    }
}

/// Junta el servidor con sus secretos y la carpeta de salida de la sesión.
pub fn resolve(s: &McpServerConfig, store: &dyn SecretStore, output: Option<&Path>) -> ResolvedServer {
    let kind = match s.transport {
        McpTransport::Stdio => {
            let mut env: BTreeMap<String, String> = s.env.iter().map(|(k, v)| (k.clone(), expand(v, output))).collect();
            if let Some(dir) = output {
                env.insert(OUTPUT_ENV.into(), dir.to_string_lossy().into_owned());
            }
            let secret_env = s
                .secrets
                .iter()
                .filter_map(|k| store.get(&secret_account(&s.name, k)).map(|v| (k.clone(), v)))
                .collect();
            ResolvedKind::Stdio {
                command: s.command.trim().to_string(),
                args: s.args.iter().map(|a| expand(a, output)).collect(),
                env,
                secret_env,
            }
        }
        McpTransport::Http => ResolvedKind::Http {
            url: s.url.trim().to_string(),
            headers: s.headers.clone(),
            token: s
                .token
                .then(|| store.get(&secret_account(&s.name, TOKEN_KEY)))
                .flatten(),
        },
    };
    ResolvedServer {
        name: s.name.trim().to_string(),
        kind,
    }
}

/// Servidores activos de una sesión, en el orden del catálogo (los nombres que ya no existen se
/// ignoran).
pub fn active_servers<'a>(catalog: &'a [McpServerConfig], active: &[String]) -> Vec<&'a McpServerConfig> {
    catalog.iter().filter(|s| active.iter().any(|a| a == &s.name)).collect()
}

/// Activa o desactiva un servidor en la lista de una sesión. Devuelve si quedó activo.
pub fn toggle(list: &mut Vec<String>, name: &str) -> bool {
    if let Some(i) = list.iter().position(|n| n == name) {
        list.remove(i);
        false
    } else {
        list.push(name.to_string());
        true
    }
}

/// Lista de servidores de un perfil (solo los que existen en el catálogo). `None` si no hay perfil.
pub fn apply_profile(profiles: &[McpProfileConfig], name: &str, catalog: &[McpServerConfig]) -> Option<Vec<String>> {
    let p = profiles.iter().find(|p| p.name == name)?;
    Some(
        p.servers
            .iter()
            .filter(|s| catalog.iter().any(|c| &c.name == *s))
            .cloned()
            .collect(),
    )
}

/// Al renombrar o borrar un servidor, las listas de las sesiones y de los perfiles lo siguen.
pub fn rename_in(lists: &mut [&mut Vec<String>], old: &str, new: Option<&str>) {
    for list in lists.iter_mut() {
        match new {
            Some(n) => list.iter_mut().filter(|x| *x == old).for_each(|x| *x = n.to_string()),
            None => list.retain(|x| x != old),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemorySecrets;

    fn stdio(name: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.into(),
            command: "npx".into(),
            args: vec!["-y".into(), "srv".into(), "--salida".into(), OUTPUT_PLACEHOLDER.into()],
            env: [("LOG".to_string(), "info".to_string())].into(),
            secrets: vec!["DB_PASSWORD".into()],
            ..Default::default()
        }
    }

    fn http(name: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.into(),
            transport: McpTransport::Http,
            url: "https://mcp.ejemplo.test/mcp".into(),
            token: true,
            ..Default::default()
        }
    }

    #[test]
    fn validacion() {
        assert_eq!(validate_server(&stdio("db"), []), Ok(()));
        assert_eq!(validate_server(&http("docs"), ["db"]), Ok(()));
        assert_eq!(validate_server(&stdio(""), []), Err(McpError::EmptyName));
        assert_eq!(validate_server(&stdio("con espacio"), []), Err(McpError::InvalidName));
        assert_eq!(validate_server(&stdio("a.b"), []), Err(McpError::InvalidName));
        assert_eq!(validate_server(&stdio("DB"), ["db"]), Err(McpError::DuplicateName));
        assert_eq!(validate_server(&stdio(&"x".repeat(41)), []), Err(McpError::NameTooLong));
        let mut s = stdio("db");
        s.command = " ".into();
        assert_eq!(validate_server(&s, []), Err(McpError::MissingCommand));
        let mut s = stdio("db");
        s.env.insert("1MAL".into(), "x".into());
        assert_eq!(validate_server(&s, []), Err(McpError::InvalidEnvName("1MAL".into())));
        let mut s = stdio("db");
        s.env.insert("DB_PASSWORD".into(), "texto plano".into());
        assert_eq!(
            validate_server(&s, []),
            Err(McpError::SecretInEnv("DB_PASSWORD".into()))
        );
        let mut h = http("docs");
        h.url = "ftp://x".into();
        assert_eq!(validate_server(&h, []), Err(McpError::InvalidUrl));
        h.url = String::new();
        assert_eq!(validate_server(&h, []), Err(McpError::MissingUrl));
    }

    #[test]
    fn resuelve_secretos_y_carpeta_de_salida() {
        let store = MemorySecrets::new();
        assert_eq!(missing_secrets(&stdio("db"), &store), ["DB_PASSWORD"]);
        assert_eq!(missing_secrets(&http("docs"), &store), [TOKEN_KEY]);
        store.set("db/DB_PASSWORD", "s3creto").unwrap();
        store.set("docs/#token", "tok").unwrap();
        assert!(missing_secrets(&stdio("db"), &store).is_empty());

        let out = Path::new(r"C:\tmp\mcp\1-2\salida");
        let r = resolve(&stdio("db"), &store, Some(out));
        let ResolvedKind::Stdio {
            args, env, secret_env, ..
        } = &r.kind
        else {
            panic!()
        };
        assert_eq!(args[3], r"C:\tmp\mcp\1-2\salida");
        assert_eq!(env[OUTPUT_ENV], r"C:\tmp\mcp\1-2\salida");
        assert_eq!(env["LOG"], "info");
        assert_eq!(secret_env["DB_PASSWORD"], "s3creto");
        assert!(r.has_secrets());

        let r = resolve(&http("docs"), &store, None);
        assert_eq!(
            r.kind,
            ResolvedKind::Http {
                url: "https://mcp.ejemplo.test/mcp".into(),
                headers: BTreeMap::new(),
                token: Some("tok".into())
            }
        );
    }

    #[test]
    fn listas_de_sesion_y_perfiles() {
        let catalog = vec![stdio("db"), http("docs"), stdio("godot")];
        let mut list = vec![];
        assert!(toggle(&mut list, "godot"));
        assert!(toggle(&mut list, "db"));
        let names: Vec<&str> = active_servers(&catalog, &list)
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, ["db", "godot"], "en el orden del catálogo");
        assert!(!toggle(&mut list, "db"));
        assert_eq!(list, ["godot"]);

        let profiles = vec![McpProfileConfig {
            name: "web".into(),
            servers: vec!["docs".into(), "borrado".into()],
            ..Default::default()
        }];
        assert_eq!(apply_profile(&profiles, "web", &catalog), Some(vec!["docs".into()]));
        assert_eq!(apply_profile(&profiles, "otro", &catalog), None);

        let mut a = vec!["db".to_string(), "docs".to_string()];
        let mut b = vec!["db".to_string()];
        rename_in(&mut [&mut a, &mut b], "db", Some("base"));
        assert_eq!(a, ["base", "docs"]);
        rename_in(&mut [&mut a, &mut b], "base", None);
        assert_eq!(a, ["docs"]);
        assert!(b.is_empty());
    }
}
