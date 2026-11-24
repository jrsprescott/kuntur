use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionConfig {
    pub name: String,
    /// Programa del shell; `None` = el predeterminado (PowerShell 7 o Windows PowerShell).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// Alto de la terminal acoplada (proporción del área de terminales); `None` = el de siempre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dock_ratio: Option<f32>,
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
    pub sessions: Vec<SessionConfig>,
    /// Agentes extra (§5).
    #[serde(rename = "agentes", skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<AgentConfig>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            sidebar: SidebarMode::Icons,
            font_size: 14.0,
            alerts: true,
            sessions: Vec::new(),
            agents: Vec::new(),
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
            sessions: vec![
                SessionConfig {
                    name: "web".into(),
                    shell: None,
                    dock_ratio: None,
                },
                SessionConfig {
                    name: "api".into(),
                    shell: Some("cmd.exe".into()),
                    dock_ratio: Some(0.5),
                },
            ],
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
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
    fn limita_el_tamano_de_fuente() {
        assert_eq!(Settings::clamp_font(4.0), 9.0);
        assert_eq!(Settings::clamp_font(40.0), 28.0);
        assert_eq!(Settings::clamp_font(14.0), 14.0);
    }
}
