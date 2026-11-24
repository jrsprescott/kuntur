//! Íconos de marca de los agentes y de los proveedores de IA local (ajustes v0.2.1).
//!
//! Los SVG vienen de Lobe Icons (MIT; ver `assets/agents/LICENSE-lobe-icons.txt`) y van dentro del
//! ejecutable. Dos estilos:
//! - [`IconStyle::Color`]: logo a color, se dibuja con `img()` tal cual.
//! - [`IconStyle::Mono`]: logo de un solo color (`currentColor`), se dibuja con `svg()` en el color
//!   de marca del agente: se ve bien en los temas oscuros y en el claro.
//!
//! Un agente sin ícono (los propios de `[[agentes]]`, Antigravity) conserva la letra de color.

/// Cómo se dibuja el ícono.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconStyle {
    Color,
    Mono,
}

/// Ícono de un agente: ruta del recurso dentro de la app y su estilo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentIcon {
    pub path: &'static str,
    pub style: IconStyle,
}

/// Prefijo de las rutas de estos recursos (ver `KunturAssets` en `main.rs`).
pub const PREFIX: &str = "agents/";

/// (ruta, estilo, contenido) de cada ícono incluido.
const FILES: [(&str, IconStyle, &[u8]); 9] = [
    (
        "agents/claude-color.svg",
        IconStyle::Color,
        include_bytes!("../assets/agents/claude-color.svg"),
    ),
    (
        "agents/gemini-color.svg",
        IconStyle::Color,
        include_bytes!("../assets/agents/gemini-color.svg"),
    ),
    (
        "agents/qwen-color.svg",
        IconStyle::Color,
        include_bytes!("../assets/agents/qwen-color.svg"),
    ),
    (
        "agents/deepseek-color.svg",
        IconStyle::Color,
        include_bytes!("../assets/agents/deepseek-color.svg"),
    ),
    (
        "agents/openai.svg",
        IconStyle::Mono,
        include_bytes!("../assets/agents/openai.svg"),
    ),
    (
        "agents/opencode.svg",
        IconStyle::Mono,
        include_bytes!("../assets/agents/opencode.svg"),
    ),
    (
        "agents/kimi.svg",
        IconStyle::Mono,
        include_bytes!("../assets/agents/kimi.svg"),
    ),
    (
        "agents/lmstudio.svg",
        IconStyle::Mono,
        include_bytes!("../assets/agents/lmstudio.svg"),
    ),
    (
        "agents/ollama.svg",
        IconStyle::Mono,
        include_bytes!("../assets/agents/ollama.svg"),
    ),
];

/// Ícono del agente (id del catálogo) o del proveedor local (`lmstudio`, `ollama`).
pub fn for_agent(id: &str) -> Option<AgentIcon> {
    let path = match id {
        "claude" => "agents/claude-color.svg",
        "codex" => "agents/openai.svg",
        "opencode" => "agents/opencode.svg",
        "qwen" => "agents/qwen-color.svg",
        "gemini" => "agents/gemini-color.svg",
        "kimi" => "agents/kimi.svg",
        "deepseek" => "agents/deepseek-color.svg",
        "lmstudio" => "agents/lmstudio.svg",
        "ollama" => "agents/ollama.svg",
        _ => return None,
    };
    FILES
        .iter()
        .find(|(p, _, _)| *p == path)
        .map(|&(path, style, _)| AgentIcon { path, style })
}

/// Contenido de un recurso `agents/…` (para el `AssetSource` de la app).
pub fn asset(path: &str) -> Option<&'static [u8]> {
    FILES.iter().find(|(p, _, _)| *p == path).map(|&(_, _, bytes)| bytes)
}

/// Rutas de todos los íconos (para `AssetSource::list`).
pub fn paths() -> impl Iterator<Item = &'static str> {
    FILES.iter().map(|&(p, _, _)| p)
}

#[cfg(test)]
mod tests {
    use super::{FILES, IconStyle, asset, for_agent, paths};

    #[test]
    fn cada_agente_conocido_y_cada_proveedor_local_tiene_icono() {
        for id in [
            "claude", "codex", "opencode", "qwen", "gemini", "kimi", "deepseek", "lmstudio", "ollama",
        ] {
            let icon = for_agent(id).unwrap_or_else(|| panic!("{id} sin ícono"));
            assert!(asset(icon.path).is_some(), "{id}: falta el archivo {}", icon.path);
        }
        assert_eq!(for_agent("claude").unwrap().style, IconStyle::Color);
        assert_eq!(for_agent("codex").unwrap().style, IconStyle::Mono);
    }

    #[test]
    fn los_demas_conservan_la_letra() {
        assert_eq!(for_agent("antigravity"), None);
        assert_eq!(for_agent("extra:mi-agente"), None);
        assert_eq!(for_agent(""), None);
    }

    #[test]
    fn el_catalogo_entero_tiene_icono_o_letra() {
        // Todo agente del catálogo integrado o tiene ícono o se dibuja con su letra (nunca falla).
        let catalog = kuntur_agents::Catalog::with_extra(&[]);
        for agent in catalog.agents() {
            if for_agent(&agent.id).is_none() {
                assert!(agent.letter.is_alphanumeric(), "{} sin ícono ni letra", agent.id);
            }
        }
    }

    #[test]
    fn los_svg_incluidos_son_seguros() {
        // Revisados al incluirlos; esta prueba evita que un cambio futuro meta algo activo.
        for (path, _, bytes) in FILES {
            let text = std::str::from_utf8(bytes).unwrap().to_ascii_lowercase();
            assert!(text.starts_with("<svg"), "{path}");
            for bad in ["<script", "foreignobject", "href", "javascript", " on", "<!entity"] {
                assert!(!text.contains(bad), "{path} contiene {bad}");
            }
        }
        assert_eq!(paths().count(), FILES.len());
    }
}
