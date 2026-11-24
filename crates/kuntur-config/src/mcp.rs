//! Servidores MCP (§11 de la especificación), perfiles y comandos con atajo, tal como se guardan en
//! `config.toml`. **Nunca** secretos: los valores de las variables listadas en `secretos` y el token
//! de los servidores http van al Administrador de credenciales de Windows (`kuntur-mcp`).
//!
//! ```toml
//! [[mcp]]
//! nombre = "base-datos"
//! comando = "npx"
//! argumentos = ["-y", "servidor-mcp-db"]
//! secretos = ["DB_PASSWORD"]
//! [mcp.entorno]
//! LOG_LEVEL = "info"
//!
//! [[mcp]]
//! nombre = "docs"
//! tipo = "http"
//! url = "https://mcp.ejemplo.test/mcp"
//! token = true
//!
//! [[mcp_perfiles]]
//! nombre = "web"
//! servidores = ["docs"]
//! modelo = "sonnet"
//! esfuerzo = "high"
//! permisos = "ediciones"
//!
//! [[comandos]]
//! nombre = "Pruebas"
//! texto = "cargo test --workspace"
//! atajo = "Ctrl+Alt+T"
//! destino = "acoplada"
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_true(b: &bool) -> bool {
    *b
}

fn yes() -> bool {
    true
}

/// Cómo se habla con el servidor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    /// Proceso local que habla JSON-RPC por stdin/stdout.
    #[default]
    Stdio,
    /// *Streamable HTTP* en una URL.
    Http,
}

impl McpTransport {
    fn is_stdio(&self) -> bool {
        *self == McpTransport::Stdio
    }
}

/// Un servidor del catálogo (`[[mcp]]`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Nombre visible y clave (solo letras, números, `-` y `_`: va en la configuración de cada CLI).
    #[serde(rename = "nombre")]
    pub name: String,
    #[serde(rename = "tipo", default, skip_serializing_if = "McpTransport::is_stdio")]
    pub transport: McpTransport,
    /// stdio: programa (buscado en el PATH) o ruta completa.
    #[serde(rename = "comando", default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(rename = "argumentos", default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// stdio: nombres de variables de entorno cuyo valor está en el Administrador de credenciales.
    #[serde(rename = "secretos", default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<String>,
    /// http: dirección del servidor.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// http: token *bearer* (encabezado `Authorization`) guardado en el Administrador de credenciales.
    #[serde(default, skip_serializing_if = "is_false")]
    pub token: bool,
    #[serde(rename = "descripcion", default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// stdio: variables de entorno visibles (sin secretos).
    #[serde(rename = "entorno", default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// http: encabezados fijos (sin secretos).
    #[serde(rename = "encabezados", default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
}

/// Modo de permisos del agente, traducido a los flags de cada CLI (`kuntur-mcp::options`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    /// Pregunta antes de editar o ejecutar (el modo normal de cada CLI).
    Preguntar,
    /// Acepta ediciones de archivos sin preguntar; pregunta por los comandos.
    Ediciones,
    /// Solo lectura / planificación.
    Plan,
    /// Sin preguntas ni sandbox. Peligroso: solo en entornos aislados.
    Total,
}

/// Perfil reutilizable (`[[mcp_perfiles]]`): un conjunto de servidores con nombre y las opciones del
/// agente (modelo, esfuerzo, permisos).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpProfileConfig {
    #[serde(rename = "nombre")]
    pub name: String,
    #[serde(rename = "servidores", default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<String>,
    #[serde(rename = "modelo", default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// `low`, `medium`, `high`, `xhigh`, `max`… (lo que acepte el CLI).
    #[serde(rename = "esfuerzo", default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(rename = "permisos", default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionMode>,
}

/// Dónde se escribe un comando con atajo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandTarget {
    /// La terminal principal de la sesión activa.
    #[default]
    Sesion,
    /// La terminal acoplada de la sesión activa (se abre si hace falta).
    Acoplada,
}

impl CommandTarget {
    fn is_default(&self) -> bool {
        *self == CommandTarget::Sesion
    }
}

/// Comando con atajo (`[[comandos]]`): texto que se escribe en la terminal con un atajo.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutCommand {
    #[serde(rename = "nombre")]
    pub name: String,
    #[serde(rename = "texto")]
    pub text: String,
    /// `Ctrl+Alt+T`, `Ctrl+Shift+F5`… (ver `kuntur-mcp::shortcuts`).
    #[serde(rename = "atajo", default, skip_serializing_if = "String::is_empty")]
    pub shortcut: String,
    #[serde(rename = "destino", default, skip_serializing_if = "CommandTarget::is_default")]
    pub target: CommandTarget,
    /// `true` = también Enter (lo ejecuta); `false` = solo lo escribe.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enter: bool,
    /// Solo en la sesión con este nombre; `None` = global.
    #[serde(rename = "sesion", default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servidor_stdio_y_http_desde_toml() {
        #[derive(Deserialize)]
        struct F {
            mcp: Vec<McpServerConfig>,
        }
        let f: F = toml::from_str(
            r#"
[[mcp]]
nombre = "db"
comando = "npx"
argumentos = ["-y", "srv"]
secretos = ["DB_PASSWORD"]
[mcp.entorno]
LOG = "info"

[[mcp]]
nombre = "docs"
tipo = "http"
url = "https://mcp.ejemplo.test/mcp"
token = true
"#,
        )
        .unwrap();
        assert_eq!(f.mcp[0].transport, McpTransport::Stdio);
        assert_eq!(f.mcp[0].args, ["-y", "srv"]);
        assert_eq!(f.mcp[0].env["LOG"], "info");
        assert_eq!(f.mcp[0].secrets, ["DB_PASSWORD"]);
        assert_eq!(f.mcp[1].transport, McpTransport::Http);
        assert!(f.mcp[1].token);
    }

    #[test]
    fn comando_con_valores_por_defecto() {
        let c: ShortcutCommand = toml::from_str("nombre = \"x\"\ntexto = \"ls\"\n").unwrap();
        assert!(c.enter);
        assert_eq!(c.target, CommandTarget::Sesion);
        assert_eq!(c.session, None);
        let back = toml::to_string(&c).unwrap();
        assert!(!back.contains("enter") && !back.contains("destino"));
    }

    #[test]
    fn perfil_con_permisos() {
        let p: McpProfileConfig =
            toml::from_str("nombre = \"web\"\nservidores = [\"a\"]\npermisos = \"ediciones\"\n").unwrap();
        assert_eq!(p.permissions, Some(PermissionMode::Ediciones));
        assert_eq!(p.model, None);
    }
}
