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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub sidebar: SidebarMode,
    pub font_size: f32,
    pub sessions: Vec<SessionConfig>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            sidebar: SidebarMode::Icons,
            font_size: 14.0,
            sessions: Vec::new(),
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
    fn limita_el_tamano_de_fuente() {
        assert_eq!(Settings::clamp_font(4.0), 9.0);
        assert_eq!(Settings::clamp_font(40.0), 28.0);
        assert_eq!(Settings::clamp_font(14.0), 14.0);
    }
}
