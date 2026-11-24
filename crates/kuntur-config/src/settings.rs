use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::connections::ConnectionConfig;
use crate::mcp::{McpProfileConfig, McpServerConfig, ShortcutCommand};

/// Vista del riel de sesiones. No existe "oculta": el mínimo son los íconos.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarMode {
    Full,
    /// Va al final: serde exige que `#[serde(other)]` sea la última variante.
    #[default]
    #[serde(other)]
    Icons,
}

/// Idioma de la interfaz. `Auto` = el del sistema (español si empieza con "es"; si no, inglés).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Es,
    En,
    /// Va al final: serde exige que `#[serde(other)]` sea la última variante.
    #[default]
    #[serde(other)]
    Auto,
}

/// Notificaciones del sistema de las alertas (§12). La insignia de la pestaña sale siempre; esto
/// solo decide cuándo además aparece la notificación.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationSettings {
    /// El agente espera tu respuesta.
    #[serde(rename = "espera")]
    pub waiting: bool,
    /// El agente pide permiso para usar una herramienta.
    #[serde(rename = "permiso")]
    pub permission: bool,
    /// El agente terminó.
    #[serde(rename = "termino")]
    pub finished: bool,
    /// Minutos mínimos entre dos notificaciones de la misma sesión (0 = sin límite).
    #[serde(rename = "intervalo_minutos")]
    pub interval_min: u32,
    /// Solo si Kuntur no está en primer plano.
    #[serde(rename = "solo_en_segundo_plano")]
    pub only_background: bool,
}

impl NotificationSettings {
    pub const MAX_INTERVAL_MIN: u32 = 24 * 60;
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            waiting: true,
            permission: true,
            finished: true,
            interval_min: 5,
            only_background: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionConfig {
    pub name: String,
    /// Programa del shell; `None` = el predeterminado (PowerShell 7 o Windows PowerShell).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// Alto de la terminal acoplada (proporción del área de terminales); `None` = el de siempre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dock_ratio: Option<f32>,
    /// Sesión remota: nombre de la conexión guardada (`None` = local). §6 de la especificación.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    /// Última carpeta conocida (OSC 7 / OSC 9;9): una sesión remota se reconecta ahí.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Servidores MCP activos en esta sesión (F10, §11).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<String>,
    /// Perfil cuyas opciones (modelo, esfuerzo, permisos) se usan al lanzar agentes aquí.
    #[serde(rename = "perfil", default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// El usuario la renombró: su nombre gana sobre el título del agente (ajustes v0.2).
    #[serde(rename = "nombre_manual", default, skip_serializing_if = "std::ops::Not::not")]
    pub manual_name: bool,
}

impl SessionConfig {
    pub fn is_remote(&self) -> bool {
        self.connection.is_some()
    }
}

/// Agente agregado por el usuario (`[[agentes]]` en `config.toml`). Si el ejecutable es el de uno
/// que Kuntur ya conoce, lo ajusta (argumentos, nombre, letra, color).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(rename = "nombre")]
    pub name: String,
    /// Comando (buscado en el PATH) o ruta completa del ejecutable.
    #[serde(rename = "ejecutable")]
    pub command: String,
    #[serde(rename = "argumentos", default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Letra de la insignia (por defecto, la primera del nombre).
    #[serde(rename = "letra", default, skip_serializing_if = "Option::is_none")]
    pub letter: Option<char>,
    /// Color `#RRGGBB` de la insignia.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Carpeta de sus registros (reservado: la auditoría solo tiene lectores para Claude Code y Codex).
    #[serde(rename = "registros", default, skip_serializing_if = "Option::is_none")]
    pub logs: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub sidebar: SidebarMode,
    pub font_size: f32,
    /// Alertas (§12): insignia en la pestaña y notificación de Windows cuando un agente de una
    /// pestaña no visible queda esperando o termina. `false` apaga la notificación (la insignia queda).
    #[serde(rename = "alertas")]
    pub alerts: bool,
    /// Sugerencias en gris mientras se escribe en el prompt (Tab o → aceptan).
    pub autocompletado: bool,
    /// El título de la pestaña activa se desplaza si no entra (ajustes v0.2).
    pub marquesina: bool,
    /// Claude Code: además de `--mcp-config`, `--strict-mcp-config` (ignora los MCP del usuario y del
    /// proyecto; solo los de la sesión).
    pub mcp_exclusivo: bool,
    /// OpenCode: formato V2 de la configuración (`mcp.servers`) en vez del V1 (`mcp.<nombre>`).
    pub mcp_opencode_v2: bool,
    /// Perfil que se aplica a las sesiones nuevas (`None` = sin MCP).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_perfil_inicial: Option<String>,
    /// Idioma de la interfaz (`auto`, `es`, `en`).
    #[serde(rename = "idioma")]
    pub language: Language,
    /// Cuándo se muestran las notificaciones (con `alertas = true`).
    #[serde(rename = "notificaciones")]
    pub notifications: NotificationSettings,
    pub sessions: Vec<SessionConfig>,
    /// Agentes extra (§5).
    #[serde(rename = "agentes", skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<AgentConfig>,
    /// Conexiones SSH guardadas (gestor de conexiones, F6).
    pub connections: Vec<ConnectionConfig>,
    /// Catálogo de servidores MCP (F10).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<McpServerConfig>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mcp_perfiles: Vec<McpProfileConfig>,
    /// Comandos con atajo (F10).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comandos: Vec<ShortcutCommand>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            sidebar: SidebarMode::Icons,
            font_size: 14.0,
            alerts: true,
            autocompletado: true,
            marquesina: true,
            mcp_exclusivo: false,
            mcp_opencode_v2: false,
            mcp_perfil_inicial: None,
            language: Language::Auto,
            notifications: NotificationSettings::default(),
            sessions: Vec::new(),
            agents: Vec::new(),
            connections: Vec::new(),
            mcp: Vec::new(),
            mcp_perfiles: Vec::new(),
            comandos: Vec::new(),
        }
    }
}

impl Settings {
    /// `%APPDATA%\Kuntur\config.toml`
    pub fn default_path() -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Kuntur").join("config.toml"))
    }

    /// Nunca falla: si no hay archivo o está roto, devuelve los valores por defecto.
    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| toml::from_str::<Settings>(&t).ok())
            .map(|mut s| {
                s.font_size = Self::clamp_font(s.font_size);
                s.notifications.interval_min = s.notifications.interval_min.min(NotificationSettings::MAX_INTERVAL_MIN);
                s
            })
            .unwrap_or_default()
    }

    /// Guarda de forma segura: escribe a un archivo temporal y luego lo renombra.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    pub fn clamp_font(size: f32) -> f32 {
        size.clamp(9.0, 28.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kuntur-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("config.toml")
    }

    #[test]
    fn sin_archivo_usa_valores_por_defecto() {
        let s = Settings::load(&tmp("vacio"));
        assert_eq!(s, Settings::default());
        assert_eq!(s.theme, "dark");
        assert_eq!(s.sidebar, SidebarMode::Icons);
    }

    #[test]
    fn guarda_y_vuelve_a_cargar() {
        let path = tmp("ida-vuelta");
        let s = Settings {
            theme: "blue".into(),
            sidebar: SidebarMode::Full,
            font_size: 15.0,
            alerts: false,
            agents: vec![AgentConfig {
                name: "Aider".into(),
                command: "aider".into(),
                args: vec!["--no-auto-commits".into()],
                letter: Some('R'),
                color: Some("#112233".into()),
                logs: None,
            }],
            autocompletado: false,
            sessions: vec![
                SessionConfig {
                    name: "web".into(),
                    shell: None,
                    dock_ratio: None,
                    ..Default::default()
                },
                SessionConfig {
                    name: "api".into(),
                    shell: Some("cmd.exe".into()),
                    dock_ratio: Some(0.5),
                    ..Default::default()
                },
                SessionConfig {
                    name: "servidor".into(),
                    connection: Some("produccion".into()),
                    cwd: Some("/var/www".into()),
                    ..Default::default()
                },
            ],
            connections: vec![ConnectionConfig {
                name: "produccion".into(),
                host: "srv.ejemplo.test".into(),
                port: 2222,
                user: "deploy".into(),
                key: Some("C:\\llaves\\id_ed25519".into()),
                remember: true,
            }],
            ..Default::default()
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[[connections]]"));
        assert!(
            !text.to_lowercase().contains("password"),
            "nunca se guardan contraseñas"
        );
        assert!(Settings::load(&path).sessions[2].is_remote());
    }

    #[test]
    fn archivo_roto_no_rompe_la_app() {
        let path = tmp("roto");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "esto no es toml [[[").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn la_vista_oculta_ya_no_existe() {
        // Regla del producto: el riel de sesiones nunca se oculta.
        let path = tmp("oculta");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "sidebar = \"hidden\"\n").unwrap();
        assert_eq!(Settings::load(&path).sidebar, SidebarMode::Icons);
    }

    #[test]
    fn sesiones_de_archivos_viejos_sin_alto_acoplado() {
        let path = tmp("sin-dock");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "[[sessions]]
name = \"web\"
",
        )
        .unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.sessions.len(), 1);
        assert_eq!(s.sessions[0].dock_ratio, None);
        // Y sin alto propio no se escribe el campo.
        let text = toml::to_string_pretty(&s).unwrap();
        assert!(!text.contains("dock_ratio"));
    }

    #[test]
    fn alertas_y_agentes_extra() {
        let path = tmp("agentes");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Archivo viejo: alertas encendidas y sin agentes extra (que tampoco se escriben).
        std::fs::write(
            &path,
            "theme = \"blue\"
",
        )
        .unwrap();
        let s = Settings::load(&path);
        assert!(s.alerts);
        assert!(s.agents.is_empty());
        assert!(!toml::to_string_pretty(&s).unwrap().contains("agentes"));

        std::fs::write(
            &path,
            "alertas = false

[[agentes]]
nombre = \"Kimi rápido\"
ejecutable = \"kimi\"
argumentos = [\"--yolo\"]
color = \"#1E88E5\"

[[agentes]]
nombre = \"Mío\"
ejecutable = 'C:\\herramientas\\mio.exe'
letra = \"M\"
",
        )
        .unwrap();
        let s = Settings::load(&path);
        assert!(!s.alerts);
        assert_eq!(s.agents.len(), 2);
        assert_eq!(s.agents[0].name, "Kimi rápido");
        assert_eq!(s.agents[0].args, ["--yolo"]);
        assert_eq!(s.agents[0].letter, None);
        assert_eq!(s.agents[1].command, r"C:\herramientas\mio.exe");
        assert_eq!(s.agents[1].letter, Some('M'));
    }

    #[test]
    fn autocompletado_encendido_por_defecto() {
        let path = tmp("autocompletado");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "theme = \"blue\"\n").unwrap();
        assert!(Settings::load(&path).autocompletado);
        std::fs::write(&path, "autocompletado = false\n").unwrap();
        assert!(!Settings::load(&path).autocompletado);
    }

    #[test]
    fn mcp_perfiles_comandos_y_sesiones_guardan_su_lista() {
        use crate::mcp::{CommandTarget, McpTransport, PermissionMode};
        let path = tmp("mcp");
        let mut s = Settings {
            mcp_perfil_inicial: Some("web".into()),
            ..Default::default()
        };
        s.sessions = vec![SessionConfig {
            name: "api".into(),
            mcp: vec!["db".into(), "docs".into()],
            profile: Some("web".into()),
            ..Default::default()
        }];
        s.mcp = vec![
            McpServerConfig {
                name: "db".into(),
                command: "npx".into(),
                args: vec!["-y".into(), "srv".into()],
                secrets: vec!["DB_PASSWORD".into()],
                env: [("LOG".to_string(), "info".to_string())].into(),
                ..Default::default()
            },
            McpServerConfig {
                name: "docs".into(),
                transport: McpTransport::Http,
                url: "https://mcp.ejemplo.test/mcp".into(),
                token: true,
                ..Default::default()
            },
        ];
        s.mcp_perfiles = vec![McpProfileConfig {
            name: "web".into(),
            servers: vec!["docs".into()],
            model: Some("sonnet".into()),
            effort: Some("high".into()),
            permissions: Some(PermissionMode::Ediciones),
        }];
        s.comandos = vec![ShortcutCommand {
            name: "Pruebas".into(),
            text: "cargo test".into(),
            shortcut: "Ctrl+Alt+T".into(),
            target: CommandTarget::Acoplada,
            enter: false,
            session: Some("api".into()),
        }];
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[[mcp]]") && text.contains("[[mcp_perfiles]]") && text.contains("[[comandos]]"));
        assert!(
            !text.to_lowercase().contains("password ="),
            "los secretos nunca van al archivo"
        );
    }

    #[test]
    fn idioma_y_notificaciones_por_defecto_en_archivos_viejos() {
        let path = tmp("notificaciones-viejo");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "theme = \"blue\"\nalertas = true\n").unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.language, Language::Auto);
        assert_eq!(s.notifications, NotificationSettings::default());
        let n = &s.notifications;
        assert!(n.waiting && n.permission && n.finished && n.only_background);
        assert_eq!(n.interval_min, 5);
    }

    #[test]
    fn idioma_y_notificaciones_se_guardan() {
        let path = tmp("notificaciones");
        let s = Settings {
            language: Language::En,
            notifications: NotificationSettings {
                waiting: false,
                permission: true,
                finished: false,
                interval_min: 15,
                only_background: false,
            },
            ..Default::default()
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("idioma = \"en\""));
        assert!(text.contains("[notificaciones]") && text.contains("intervalo_minutos = 15"));
    }

    #[test]
    fn idioma_desconocido_vuelve_a_automatico_y_el_intervalo_se_limita() {
        let path = tmp("idioma-raro");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "idioma = \"klingon\"\n[notificaciones]\nintervalo_minutos = 100000\n",
        )
        .unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.language, Language::Auto);
        assert_eq!(s.notifications.interval_min, NotificationSettings::MAX_INTERVAL_MIN);
    }

    #[test]
    fn marquesina_y_nombre_manual() {
        let path = tmp("nombre-manual");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Archivo viejo: marquesina encendida y nombres sin la marca.
        std::fs::write(&path, "[[sessions]]
name = \"web\"
").unwrap();
        let s = Settings::load(&path);
        assert!(s.marquesina);
        assert!(!s.sessions[0].manual_name);
        // La marca solo se escribe si es verdadera.
        assert!(!toml::to_string_pretty(&s).unwrap().contains("nombre_manual"));
        let mut s = s;
        s.marquesina = false;
        s.sessions[0].manual_name = true;
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("nombre_manual = true") && text.contains("marquesina = false"));
    }

    #[test]
    fn limita_el_tamano_de_fuente() {
        assert_eq!(Settings::clamp_font(4.0), 9.0);
        assert_eq!(Settings::clamp_font(40.0), 28.0);
        assert_eq!(Settings::clamp_font(14.0), 14.0);
    }
}
