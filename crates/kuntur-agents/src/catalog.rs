//! Catálogo de agentes: los conocidos (Claude Code, Codex, OpenCode, Qwen, Gemini, Kimi, DeepSeek,
//! Antigravity) más los que el usuario agrega en `config.toml`, y la detección de los instalados.

use std::ffi::OsStr;
use std::path::Path;

use kuntur_usage::AgentKind;

/// Datos de un agente para la UI, la detección y para abrirlo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentDef {
    /// Identificador estable: `claude`, `codex`… o `extra:<comando>` para los de la configuración.
    pub id: String,
    pub name: String,
    /// Lo que se escribe en la terminal para abrirlo (y se busca en el PATH). Puede ser una ruta.
    pub command: String,
    /// Argumentos que se agregan al abrirlo.
    pub args: Vec<String>,
    /// Nombres del proceso (minúsculas, sin `.exe`) que lo identifican en el árbol de procesos.
    pub exe_names: Vec<String>,
    /// Nombres de paquete o carpeta que lo identifican cuando corre como script de Node
    /// (`node …\@openai\codex\bin\codex.js`).
    pub script_names: Vec<String>,
    /// Color de marca 0xRRGGBB.
    pub color: u32,
    pub letter: char,
    /// Agente con lector de uso / límites (`None` si Kuntur no lee su historial).
    pub usage: Option<AgentKind>,
    /// Es un IDE (Antigravity): se abre, pero no cuenta como "agente corriendo" en una sesión.
    pub ide: bool,
    /// Viene de `config.toml`.
    pub extra: bool,
    /// Cómo recibe una imagen pegada (ajustes v0.2, "imágenes como Warp").
    pub image_paste: ImagePaste,
}

/// Cómo recibe un agente una imagen que el usuario pega con Ctrl+V (ajustes v0.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImagePaste {
    /// Kuntur guarda la imagen como PNG y pega su ruta (sin Enter).
    #[default]
    Path,
    /// El agente lee el portapapeles con su propia tecla: Kuntur se la envía.
    Key(PasteKey),
}

/// Tecla que un agente usa para pegar una imagen del portapapeles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteKey {
    /// Ctrl + letra (por ejemplo Ctrl+V = 0x16).
    Ctrl(char),
    /// Alt + letra (ESC + letra), por ejemplo Alt+V de Claude Code en Windows.
    Alt(char),
}

impl PasteKey {
    /// La de Claude Code (`chat:imagePaste`): Alt+V en Windows (la terminal se queda con Ctrl+V),
    /// Ctrl+V en Linux y Mac.
    pub fn claude_default() -> Self {
        if cfg!(windows) {
            PasteKey::Alt('v')
        } else {
            PasteKey::Ctrl('v')
        }
    }

    /// Bytes que se escriben en la terminal.
    pub fn bytes(self) -> Vec<u8> {
        match self {
            PasteKey::Ctrl(c) => vec![(c.to_ascii_lowercase() as u8) & 0x1f],
            PasteKey::Alt(c) => vec![0x1b, c as u8],
        }
    }
}

impl ImagePaste {
    /// Lee `pegar_imagen` de la configuración: `"ctrl+v"`, `"alt+v"` o `"ruta"` / `"path"`.
    /// Cualquier otra cosa: `None` (se queda lo que trae el agente).
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim().to_ascii_lowercase().replace(' ', "");
        if t == "ruta" || t == "path" {
            return Some(ImagePaste::Path);
        }
        let (modifier, key) = t.split_once('+')?;
        let mut chars = key.chars();
        let c = chars.next().filter(|c| c.is_ascii_alphabetic())?;
        if chars.next().is_some() {
            return None;
        }
        match modifier {
            "ctrl" | "control" => Some(ImagePaste::Key(PasteKey::Ctrl(c))),
            "alt" => Some(ImagePaste::Key(PasteKey::Alt(c))),
            _ => None,
        }
    }
}

/// Agente que el usuario agrega en `config.toml` (`[[agentes]]`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtraAgent {
    pub name: String,
    /// Comando o ruta del ejecutable.
    pub command: String,
    pub args: Vec<String>,
    pub letter: Option<char>,
    /// `#RRGGBB` (o `RRGGBB`).
    pub color: Option<String>,
    /// Cómo pega imágenes (`pegar_imagen`: "ctrl+v", "alt+v" o "ruta").
    pub image_paste: Option<String>,
}

struct Builtin {
    id: &'static str,
    name: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    script_names: &'static [&'static str],
    color: u32,
    letter: char,
    usage: Option<AgentKind>,
    ide: bool,
    /// Lee imágenes del portapapeles con su propia tecla (solo Claude Code, documentado).
    native_image_paste: bool,
}

const BUILTINS: &[Builtin] = &[
    Builtin {
        id: "claude",
        name: "Claude Code",
        command: "claude",
        args: &[],
        script_names: &["claude", "claude-code"],
        color: 0xD97757,
        letter: 'C',
        usage: Some(AgentKind::ClaudeCode),
        ide: false,
        native_image_paste: true,
    },
    Builtin {
        id: "codex",
        name: "Codex",
        command: "codex",
        args: &[],
        script_names: &["codex"],
        color: 0x10A37F,
        letter: 'X',
        usage: Some(AgentKind::Codex),
        ide: false,
        native_image_paste: false,
    },
    Builtin {
        id: "opencode",
        name: "OpenCode",
        command: "opencode",
        args: &[],
        script_names: &["opencode", "opencode-ai"],
        color: 0xF5A623,
        letter: 'O',
        usage: Some(AgentKind::OpenCode),
        ide: false,
        native_image_paste: false,
    },
    Builtin {
        id: "qwen",
        name: "Qwen Code",
        command: "qwen",
        args: &[],
        script_names: &["qwen", "qwen-code"],
        color: 0x615CED,
        letter: 'Q',
        usage: Some(AgentKind::Qwen),
        ide: false,
        native_image_paste: false,
    },
    Builtin {
        id: "gemini",
        name: "Gemini CLI",
        command: "gemini",
        args: &[],
        script_names: &["gemini", "gemini-cli"],
        color: 0x4285F4,
        letter: 'G',
        usage: Some(AgentKind::Gemini),
        ide: false,
        native_image_paste: false,
    },
    // Kimi CLI (Moonshot AI): se instala como herramienta de Python (`kimi.exe`).
    Builtin {
        id: "kimi",
        name: "Kimi",
        command: "kimi",
        args: &[],
        script_names: &["kimi", "kimi-cli"],
        color: 0x1E88E5,
        letter: 'K',
        usage: None,
        ide: false,
        native_image_paste: false,
    },
    // DeepSeek no publica un CLI oficial: se detecta el comando `deepseek` de los CLIs de terceros.
    Builtin {
        id: "deepseek",
        name: "DeepSeek",
        command: "deepseek",
        args: &[],
        script_names: &["deepseek", "deepseek-cli"],
        color: 0x4D6BFE,
        letter: 'D',
        usage: None,
        ide: false,
        native_image_paste: false,
    },
    // Antigravity es un IDE (derivado de VS Code): `antigravity .` lo abre en la carpeta actual.
    Builtin {
        id: "antigravity",
        name: "Antigravity",
        command: "antigravity",
        args: &["."],
        script_names: &[],
        color: 0x34A853,
        letter: 'A',
        usage: None,
        ide: true,
        native_image_paste: false,
    },
];

impl Builtin {
    fn def(&self) -> AgentDef {
        AgentDef {
            id: self.id.into(),
            name: self.name.into(),
            command: self.command.into(),
            args: self.args.iter().map(|s| s.to_string()).collect(),
            exe_names: vec![self.command.into()],
            script_names: self.script_names.iter().map(|s| s.to_string()).collect(),
            color: self.color,
            letter: self.letter,
            usage: self.usage,
            ide: self.ide,
            extra: false,
            image_paste: if self.native_image_paste {
                ImagePaste::Key(PasteKey::claude_default())
            } else {
                ImagePaste::Path
            },
        }
    }
}

/// Color de marca de un agente con lector de uso (para la vista "Uso de agentes").
pub fn brand_color(kind: AgentKind) -> Option<u32> {
    BUILTINS.iter().find(|b| b.usage == Some(kind)).map(|b| b.color)
}

/// Lista de agentes que Kuntur conoce.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catalog {
    agents: Vec<AgentDef>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self::builtin()
    }
}

impl Catalog {
    /// Solo los conocidos.
    pub fn builtin() -> Self {
        Self {
            agents: BUILTINS.iter().map(Builtin::def).collect(),
        }
    }

    /// Los conocidos más los de la configuración. Un extra cuyo ejecutable es el de uno conocido lo
    /// ajusta (argumentos y, si los trae, nombre, letra y color); uno sin nombre o sin ejecutable se
    /// ignora, igual que uno repetido.
    pub fn with_extra(extra: &[ExtraAgent]) -> Self {
        let mut catalog = Self::builtin();
        for e in extra {
            let command = e.command.trim();
            let name = e.name.trim();
            if command.is_empty() || name.is_empty() {
                continue;
            }
            let base = base_name(command);
            let letter = e
                .letter
                .filter(|c| c.is_alphanumeric())
                .map(|c| c.to_uppercase().next().unwrap_or(c));
            let color = e.color.as_deref().and_then(parse_color);
            let image_paste = e.image_paste.as_deref().and_then(ImagePaste::parse);
            if let Some(known) = catalog.agents.iter_mut().find(|a| a.exe_names.contains(&base)) {
                if known.extra {
                    continue;
                }
                known.args = e.args.clone();
                known.name = name.to_string();
                if command != known.command && (command.contains(['\\', '/'])) {
                    known.command = command.to_string();
                }
                known.letter = letter.unwrap_or(known.letter);
                known.color = color.unwrap_or(known.color);
                known.image_paste = image_paste.unwrap_or(known.image_paste);
                continue;
            }
            catalog.agents.push(AgentDef {
                id: format!("extra:{base}"),
                name: name.to_string(),
                command: command.to_string(),
                args: e.args.clone(),
                exe_names: vec![base.clone()],
                script_names: vec![base],
                color: color.unwrap_or(0x8A8F98),
                letter: letter
                    .or_else(|| name.chars().find(|c| c.is_alphanumeric()))
                    .map(|c| c.to_uppercase().next().unwrap_or(c))
                    .unwrap_or('?'),
                usage: None,
                ide: false,
                extra: true,
                image_paste: image_paste.unwrap_or_default(),
            });
        }
        catalog
    }

    pub fn agents(&self) -> &[AgentDef] {
        &self.agents
    }

    pub fn get(&self, id: &str) -> Option<&AgentDef> {
        self.agents.iter().find(|a| a.id == id)
    }

    /// El agente que corresponde a un `AgentKind` de los historiales de uso.
    pub fn by_usage(&self, kind: AgentKind) -> Option<&AgentDef> {
        self.agents.iter().find(|a| a.usage == Some(kind))
    }

    /// Ids de los agentes instalados según el PATH del proceso.
    pub fn installed(&self) -> Vec<String> {
        let path = std::env::var_os("PATH").unwrap_or_default();
        self.installed_in(&path)
    }

    /// Ids de los agentes con un ejecutable (`.exe`, `.cmd`, `.bat`, `.ps1`) en alguna carpeta de
    /// `path_var`, o cuyo comando es una ruta a un archivo que existe.
    pub fn installed_in(&self, path_var: &OsStr) -> Vec<String> {
        let dirs: Vec<_> = std::env::split_paths(path_var).collect();
        self.agents
            .iter()
            .filter(|agent| {
                let command = agent.command.as_str();
                if command.contains(['\\', '/']) {
                    return Path::new(command).is_file();
                }
                let base = base_name(command);
                dirs.iter().any(|dir| {
                    [".exe", ".cmd", ".bat", ".ps1"]
                        .iter()
                        .any(|ext| dir.join(format!("{base}{ext}")).is_file())
                })
            })
            .map(|agent| agent.id.clone())
            .collect()
    }

    /// Qué agente es un proceso, por su nombre y línea de comando (`cmdline[0]` es el programa).
    /// Los IDE no cuentan.
    pub fn match_process(&self, name: &str, cmdline: &[String]) -> Option<&AgentDef> {
        let agents = || self.agents.iter().filter(|a| !a.ide);
        let base = base_name(name);
        if let Some(agent) = agents().find(|a| a.exe_names.contains(&base)) {
            return Some(agent);
        }
        if !SCRIPT_HOSTS.contains(&base.as_str()) {
            return None;
        }
        // El script es el primer argumento que no es una opción del intérprete; se mira cada tramo de
        // su ruta (`…\@openai\codex\bin\codex.js` → "codex"), nunca los argumentos del script.
        let script = cmdline.iter().skip(1).find(|a| !a.starts_with('-'))?;
        script
            .split(['/', '\\'])
            .filter(|s| !s.is_empty())
            .map(|segment| {
                let lower = segment.to_ascii_lowercase();
                match lower.rsplit_once('.') {
                    Some((stem, "js" | "mjs" | "cjs" | "ts")) => stem.to_string(),
                    _ => lower,
                }
            })
            .find_map(|segment| agents().find(|a| a.script_names.contains(&segment)))
    }
}

/// Nombre del programa en minúsculas, sin carpeta ni extensión de ejecutable.
fn base_name(name: &str) -> String {
    let file = name.rsplit(['\\', '/']).next().unwrap_or(name);
    let lower = file.to_ascii_lowercase();
    for ext in [".exe", ".cmd", ".bat", ".ps1"] {
        if let Some(stem) = lower.strip_suffix(ext) {
            return stem.to_string();
        }
    }
    lower
}

/// `#RRGGBB` o `RRGGBB` → 0xRRGGBB.
pub fn parse_color(text: &str) -> Option<u32> {
    let hex = text.trim().trim_start_matches('#');
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

/// Intérpretes con los que se distribuyen agentes como paquetes npm.
const SCRIPT_HOSTS: [&str; 3] = ["node", "bun", "deno"];

#[cfg(test)]
mod image_paste_tests {
    use super::*;

    #[test]
    fn claude_pega_imagenes_con_su_tecla_y_los_demas_por_ruta() {
        let c = Catalog::builtin();
        assert_eq!(
            c.get("claude").unwrap().image_paste,
            ImagePaste::Key(PasteKey::claude_default())
        );
        for id in ["codex", "opencode", "gemini", "qwen", "kimi"] {
            assert_eq!(c.get(id).unwrap().image_paste, ImagePaste::Path, "{id}");
        }
    }

    #[test]
    fn tecla_de_claude_segun_el_sistema_y_sus_bytes() {
        if cfg!(windows) {
            assert_eq!(PasteKey::claude_default(), PasteKey::Alt('v'));
        } else {
            assert_eq!(PasteKey::claude_default(), PasteKey::Ctrl('v'));
        }
        assert_eq!(PasteKey::Alt('v').bytes(), b"\x1bv");
        assert_eq!(PasteKey::Ctrl('v').bytes(), [0x16]);
        assert_eq!(PasteKey::Ctrl('V').bytes(), [0x16]);
    }

    #[test]
    fn pegar_imagen_de_la_configuracion() {
        assert_eq!(ImagePaste::parse("Ctrl+V"), Some(ImagePaste::Key(PasteKey::Ctrl('v'))));
        assert_eq!(
            ImagePaste::parse(" alt + v "),
            Some(ImagePaste::Key(PasteKey::Alt('v')))
        );
        assert_eq!(ImagePaste::parse("ruta"), Some(ImagePaste::Path));
        assert_eq!(ImagePaste::parse("path"), Some(ImagePaste::Path));
        assert_eq!(ImagePaste::parse("shift+v"), None);
        assert_eq!(ImagePaste::parse("ctrl+vv"), None);
        assert_eq!(ImagePaste::parse("ctrl+1"), None);
        assert_eq!(ImagePaste::parse(""), None);
    }

    #[test]
    fn la_configuracion_ajusta_la_tecla_de_un_agente() {
        let c = Catalog::with_extra(&[
            ExtraAgent {
                name: "Codex".into(),
                command: "codex".into(),
                image_paste: Some("ctrl+v".into()),
                ..ExtraAgent::default()
            },
            ExtraAgent {
                name: "Claude".into(),
                command: "claude".into(),
                image_paste: Some("ruta".into()),
                ..ExtraAgent::default()
            },
            ExtraAgent {
                name: "Propio".into(),
                command: "propio".into(),
                image_paste: Some("alt+i".into()),
                ..ExtraAgent::default()
            },
        ]);
        assert_eq!(
            c.get("codex").unwrap().image_paste,
            ImagePaste::Key(PasteKey::Ctrl('v'))
        );
        assert_eq!(c.get("claude").unwrap().image_paste, ImagePaste::Path);
        assert_eq!(
            c.get("extra:propio").unwrap().image_paste,
            ImagePaste::Key(PasteKey::Alt('i'))
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    fn id_of(c: &Catalog, name: &str, cmdline: &[String]) -> Option<String> {
        c.match_process(name, cmdline).map(|a| a.id.clone())
    }

    #[test]
    fn reconoce_ejecutables_por_nombre() {
        let c = Catalog::builtin();
        assert_eq!(
            id_of(&c, "claude.exe", &cmd(&["claude.exe"])).as_deref(),
            Some("claude")
        );
        assert_eq!(id_of(&c, "Codex.EXE", &[]).as_deref(), Some("codex"));
        assert_eq!(id_of(&c, "opencode.exe", &[]).as_deref(), Some("opencode"));
        assert_eq!(id_of(&c, "qwen", &[]).as_deref(), Some("qwen"));
        assert_eq!(id_of(&c, "gemini.exe", &[]).as_deref(), Some("gemini"));
        assert_eq!(id_of(&c, "kimi.exe", &[]).as_deref(), Some("kimi"));
        assert_eq!(id_of(&c, "deepseek.exe", &[]).as_deref(), Some("deepseek"));
        assert_eq!(id_of(&c, "pwsh.exe", &cmd(&["pwsh.exe", "-NoLogo"])), None);
        assert_eq!(id_of(&c, "claudette.exe", &[]), None);
        // Un IDE no es un agente corriendo en la sesión.
        assert_eq!(id_of(&c, "Antigravity.exe", &[]), None);
    }

    #[test]
    fn reconoce_agentes_que_corren_en_node() {
        let c = Catalog::builtin();
        let codex = cmd(&[
            r"C:\Program Files\nodejs\node.exe",
            r"C:\Users\u\AppData\Roaming\npm\node_modules\@openai\codex\bin\codex.js",
            "--model",
            "x",
        ]);
        assert_eq!(id_of(&c, "node.exe", &codex).as_deref(), Some("codex"));
        let gemini = cmd(&[
            "node",
            "--no-warnings",
            "/usr/lib/node_modules/@google/gemini-cli/dist/index.js",
        ]);
        assert_eq!(id_of(&c, "node", &gemini).as_deref(), Some("gemini"));
        let claude = cmd(&["node.exe", r"C:\npm\node_modules\@anthropic-ai\claude-code\cli.js"]);
        assert_eq!(id_of(&c, "node.exe", &claude).as_deref(), Some("claude"));
        let qwen = cmd(&["node", "/x/node_modules/@qwen-code/qwen-code/cli.js"]);
        assert_eq!(id_of(&c, "node", &qwen).as_deref(), Some("qwen"));
        // Un script cualquiera, aunque un argumento se llame "codex", no es un agente.
        let otro = cmd(&["node.exe", r"C:\src\servidor\index.js", "codex"]);
        assert_eq!(id_of(&c, "node.exe", &otro), None);
        let carpeta_parecida = cmd(&["node.exe", r"C:\src\codex-tools\run.js"]);
        assert_eq!(id_of(&c, "node.exe", &carpeta_parecida), None);
        assert_eq!(id_of(&c, "node.exe", &cmd(&["node.exe"])), None);
    }

    #[test]
    fn catalogo_completo() {
        let c = Catalog::builtin();
        let ids: Vec<&str> = c.agents().iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "claude",
                "codex",
                "opencode",
                "qwen",
                "gemini",
                "kimi",
                "deepseek",
                "antigravity"
            ]
        );
        for a in c.agents() {
            assert!(a.letter.is_ascii_uppercase(), "{}", a.id);
            assert!(!a.extra);
        }
        assert!(c.get("antigravity").unwrap().ide);
        assert_eq!(c.get("antigravity").unwrap().args, ["."]);
        assert_eq!(c.get("codex").unwrap().usage, Some(AgentKind::Codex));
        assert_eq!(c.get("kimi").unwrap().usage, None);
        assert_eq!(brand_color(AgentKind::ClaudeCode), Some(0xD97757));
        assert_eq!(brand_color(AgentKind::Other), None);
    }

    #[test]
    fn agentes_extra_de_la_configuracion() {
        let extra = vec![
            ExtraAgent {
                name: "Aider".into(),
                command: "aider".into(),
                args: vec!["--no-auto-commits".into()],
                letter: Some('r'),
                color: Some("#112233".into()),
                image_paste: None,
            },
            // Ajusta uno conocido en vez de duplicarlo.
            ExtraAgent {
                name: "Claude (Opus)".into(),
                command: "claude.exe".into(),
                args: vec!["--model".into(), "opus".into()],
                letter: None,
                color: Some("zzz".into()),
                image_paste: None,
            },
            // Con ruta completa, sin letra ni color.
            ExtraAgent {
                name: "mi agente".into(),
                command: r"C:\herramientas\MiAgente.exe".into(),
                ..ExtraAgent::default()
            },
            // Inválidos o repetidos: se ignoran.
            ExtraAgent {
                name: "".into(),
                command: "nada".into(),
                ..ExtraAgent::default()
            },
            ExtraAgent {
                name: "Aider otra vez".into(),
                command: "aider".into(),
                ..ExtraAgent::default()
            },
        ];
        let c = Catalog::with_extra(&extra);
        assert_eq!(c.agents().len(), BUILTINS.len() + 2);

        let aider = c.get("extra:aider").expect("agregado");
        assert_eq!(aider.name, "Aider");
        assert_eq!(aider.args, ["--no-auto-commits"]);
        assert_eq!(aider.letter, 'R');
        assert_eq!(aider.color, 0x112233);
        assert!(aider.extra && !aider.ide);

        let claude = c.get("claude").unwrap();
        assert_eq!(claude.name, "Claude (Opus)");
        assert_eq!(claude.args, ["--model", "opus"]);
        assert_eq!(claude.command, "claude");
        assert_eq!(claude.color, 0xD97757, "un color inválido no cambia el de marca");

        let mine = c.get("extra:miagente").unwrap();
        assert_eq!(mine.letter, 'M');
        assert_eq!(mine.command, r"C:\herramientas\MiAgente.exe");
        // Se reconoce su proceso.
        assert_eq!(id_of(&c, "MiAgente.exe", &[]).as_deref(), Some("extra:miagente"));
    }

    #[test]
    fn colores() {
        assert_eq!(parse_color("#D97757"), Some(0xD97757));
        assert_eq!(parse_color("10a37f"), Some(0x10A37F));
        assert_eq!(parse_color("#123"), None);
        assert_eq!(parse_color("#GGGGGG"), None);
    }

    #[test]
    fn detecta_instalados_en_el_path() {
        let base = std::env::temp_dir().join(format!("kuntur-agents-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (a, b) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("codex.cmd"), "").unwrap();
        std::fs::write(b.join("claude.exe"), "").unwrap();
        std::fs::write(b.join("gemini.ps1"), "").unwrap();
        std::fs::write(b.join("kimi.exe"), "").unwrap();
        std::fs::write(b.join("antigravity.cmd"), "").unwrap();
        std::fs::write(b.join("qwen.txt"), "").unwrap();
        std::fs::write(b.join("propio.exe"), "").unwrap();
        std::fs::create_dir_all(b.join("opencode.exe")).unwrap(); // una carpeta no cuenta
        let path = std::env::join_paths([&a, &b, &base.join("no-existe")]).unwrap();
        let c = Catalog::with_extra(&[
            ExtraAgent {
                name: "Propio".into(),
                command: b.join("propio.exe").to_string_lossy().into_owned(),
                ..ExtraAgent::default()
            },
            ExtraAgent {
                name: "Falta".into(),
                command: "falta".into(),
                ..ExtraAgent::default()
            },
        ]);
        let found = c.installed_in(&path);
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(
            found,
            ["claude", "codex", "gemini", "kimi", "antigravity", "extra:propio"]
        );
    }
}
