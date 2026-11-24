//! "Instalar ganchos de Claude Code": qué se agrega a `~/.claude/settings.json` y cómo se escribe.
//!
//! Lógica pura sobre el texto del archivo ([`plan`]) más la escritura con copia de seguridad
//! ([`apply`]). Kuntur muestra el plan y solo llama a `apply` cuando el usuario pulsa "Instalar".
//! Nunca se reemplaza lo que el usuario ya tiene: se agrega un gancho por evento (si no está ya) y la
//! línea de estado solo si no hay otra.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// Eventos de Claude Code que usa Kuntur y si llevan `matcher` (los de herramientas).
pub const HOOK_EVENTS: &[(&str, bool)] = &[
    ("SessionStart", false),
    ("UserPromptSubmit", false),
    ("PreToolUse", true),
    ("PostToolUse", true),
    ("PostToolUseFailure", true),
    ("PermissionRequest", true),
    ("Notification", false),
    ("Stop", false),
    ("SubagentStart", false),
    ("SubagentStop", false),
    ("SessionEnd", false),
];

/// Segundos máximos que Claude Code espera al gancho (tarda milisegundos).
pub const HOOK_TIMEOUT: u64 = 5;

/// Qué pasa con la línea de estado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatusLineChange {
    /// No había ninguna: se agrega la de Kuntur.
    Added,
    /// Ya está la de Kuntur.
    AlreadyKuntur,
    /// El usuario tiene otra (su comando): no se toca; el anillo de Claude queda sin datos.
    KeptExisting(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct InstallPlan {
    /// Eventos a los que se les agrega el gancho.
    pub added_events: Vec<String>,
    pub statusline: StatusLineChange,
    /// Comando que se registra (`C:/…/kuntur-hook.exe`).
    pub command: String,
    /// Fragmento JSON exacto que se agrega (para mostrarlo antes de confirmar).
    pub preview: String,
    /// Archivo completo después de instalar.
    pub new_text: String,
}

impl InstallPlan {
    /// ¿Hay algo que escribir?
    pub fn changes(&self) -> bool {
        !self.added_events.is_empty() || self.statusline == StatusLineChange::Added
    }
}

/// Comando para un ejecutable: barras `/` (sirven en Git Bash, cmd y PowerShell); entre comillas
/// solo si la ruta tiene espacios.
pub fn command_for(exe: &Path, arg: Option<&str>) -> String {
    let path = exe.to_string_lossy().replace('\\', "/");
    let path = if path.contains(' ') {
        format!("\"{path}\"")
    } else {
        path
    };
    match arg {
        Some(a) => format!("{path} {a}"),
        None => path,
    }
}

fn is_kuntur(command: &str) -> bool {
    command.to_ascii_lowercase().contains("kuntur-hook")
}

/// ¿Algún gancho del evento ya llama a kuntur-hook?
fn has_kuntur_hook(groups: &Value) -> bool {
    groups.as_array().into_iter().flatten().any(|g| {
        g.get("hooks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|h| h.get("command").and_then(Value::as_str).is_some_and(is_kuntur))
    })
}

fn hook_group(command: &str, matcher: bool) -> Value {
    let hook = json!({ "type": "command", "command": command, "timeout": HOOK_TIMEOUT });
    if matcher {
        json!({ "matcher": "*", "hooks": [hook] })
    } else {
        json!({ "hooks": [hook] })
    }
}

/// Plan de instalación sobre el contenido actual (`None` = el archivo no existe). Error si el archivo
/// no es un objeto JSON válido (no se toca).
pub fn plan(existing: Option<&str>, hook_exe: &Path) -> Result<InstallPlan, String> {
    // Los editores de Windows suelen guardar con BOM: no es parte del JSON.
    let mut root: Map<String, Value> = match existing.map(|t| t.trim_start_matches('\u{feff}').trim()) {
        None | Some("") => Map::new(),
        Some(text) => match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(m)) => m,
            Ok(_) => return Err("settings.json no es un objeto JSON".into()),
            Err(e) => return Err(format!("settings.json no es JSON válido: {e}")),
        },
    };
    let command = command_for(hook_exe, None);
    let mut preview_hooks = Map::new();
    let mut added_events = Vec::new();

    let hooks = root.entry("hooks").or_insert_with(|| Value::Object(Map::new()));
    let Some(hooks) = hooks.as_object_mut() else {
        return Err("\"hooks\" de settings.json no es un objeto".into());
    };
    for &(event, matcher) in HOOK_EVENTS {
        let groups = hooks.entry(event).or_insert_with(|| Value::Array(Vec::new()));
        if has_kuntur_hook(groups) {
            continue;
        }
        let Some(list) = groups.as_array_mut() else {
            return Err(format!("\"hooks.{event}\" de settings.json no es una lista"));
        };
        let group = hook_group(&command, matcher);
        list.push(group.clone());
        preview_hooks.insert(event.into(), Value::Array(vec![group]));
        added_events.push(event.to_string());
    }

    let status_command = command_for(hook_exe, Some("statusline"));
    let statusline = match root
        .get("statusLine")
        .and_then(|s| s.get("command"))
        .and_then(Value::as_str)
    {
        Some(c) if is_kuntur(c) => StatusLineChange::AlreadyKuntur,
        Some(c) => StatusLineChange::KeptExisting(c.to_string()),
        None => StatusLineChange::Added,
    };
    let mut preview = Map::new();
    if !preview_hooks.is_empty() {
        preview.insert("hooks".into(), Value::Object(preview_hooks));
    }
    if statusline == StatusLineChange::Added {
        let value = json!({ "type": "command", "command": status_command, "padding": 0 });
        root.insert("statusLine".into(), value.clone());
        preview.insert("statusLine".into(), value);
    }
    let new_text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())? + "\n";
    let preview = if preview.is_empty() {
        String::new()
    } else {
        serde_json::to_string_pretty(&Value::Object(preview)).map_err(|e| e.to_string())?
    };
    Ok(InstallPlan {
        added_events,
        statusline,
        command,
        preview,
        new_text,
    })
}

/// Ruta de la copia de seguridad: `settings.json.kuntur-<sello>.bak` junto al archivo.
pub fn backup_path(settings: &Path, stamp: &str) -> PathBuf {
    let name = settings
        .file_name()
        .map_or_else(|| "settings.json".into(), |n| n.to_string_lossy().into_owned());
    settings.with_file_name(format!("{name}.kuntur-{stamp}.bak"))
}

/// Escribe el plan: primero la copia de seguridad (si el archivo existía) y luego el archivo nuevo
/// (a través de un temporal, para no dejarlo a medias). Devuelve la ruta de la copia.
pub fn apply(settings: &Path, plan: &InstallPlan, stamp: &str) -> std::io::Result<Option<PathBuf>> {
    let backup = if settings.exists() {
        let b = backup_path(settings, stamp);
        std::fs::copy(settings, &b)?;
        Some(b)
    } else {
        if let Some(dir) = settings.parent() {
            std::fs::create_dir_all(dir)?;
        }
        None
    };
    let tmp = settings.with_extension("json.kuntur-tmp");
    std::fs::write(&tmp, &plan.new_text)?;
    std::fs::rename(&tmp, settings)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe() -> PathBuf {
        PathBuf::from(r"C:\Programas\Kuntur\kuntur-hook.exe")
    }

    #[test]
    fn archivo_nuevo_agrega_todos_los_eventos_y_la_linea_de_estado() {
        let p = plan(None, &exe()).unwrap();
        assert_eq!(p.added_events.len(), HOOK_EVENTS.len());
        assert_eq!(p.statusline, StatusLineChange::Added);
        assert_eq!(p.command, "C:/Programas/Kuntur/kuntur-hook.exe");
        let v: Value = serde_json::from_str(&p.new_text).unwrap();
        assert_eq!(
            v["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            "C:/Programas/Kuntur/kuntur-hook.exe"
        );
        assert_eq!(v["hooks"]["PreToolUse"][0]["matcher"], "*");
        assert!(v["hooks"]["Stop"][0].get("matcher").is_none());
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["timeout"], 5);
        assert_eq!(
            v["statusLine"]["command"],
            "C:/Programas/Kuntur/kuntur-hook.exe statusline"
        );
        assert!(p.preview.contains("SubagentStop"));
        assert!(p.changes());
    }

    #[test]
    fn conserva_lo_del_usuario_y_su_orden() {
        let existing = r#"{
  "model": "opus",
  "hooks": {
    "PreToolUse": [ { "matcher": "Bash", "hooks": [ { "type": "command", "command": "mi-script.sh" } ] } ]
  },
  "statusLine": { "type": "command", "command": "mi-linea.sh" },
  "zeta": 1,
  "alfa": 2
}"#;
        let p = plan(Some(existing), &exe()).unwrap();
        assert_eq!(p.statusline, StatusLineChange::KeptExisting("mi-linea.sh".into()));
        assert!(!p.preview.contains("statusLine"));
        let v: Value = serde_json::from_str(&p.new_text).unwrap();
        // El gancho del usuario sigue primero; el de Kuntur va después.
        assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "mi-script.sh");
        assert_eq!(
            v["hooks"]["PreToolUse"][1]["hooks"][0]["command"],
            "C:/Programas/Kuntur/kuntur-hook.exe"
        );
        assert_eq!(v["statusLine"]["command"], "mi-linea.sh");
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["model", "hooks", "statusLine", "zeta", "alfa"]);
    }

    #[test]
    fn instalar_dos_veces_no_duplica() {
        let first = plan(None, &exe()).unwrap();
        let second = plan(Some(&first.new_text), &exe()).unwrap();
        assert!(second.added_events.is_empty());
        assert_eq!(second.statusline, StatusLineChange::AlreadyKuntur);
        assert!(!second.changes());
        assert!(second.preview.is_empty());
        assert_eq!(second.new_text, first.new_text);
    }

    #[test]
    fn rutas_con_espacios_van_entre_comillas() {
        let c = command_for(
            Path::new(r"C:\Program Files\Kuntur\kuntur-hook.exe"),
            Some("statusline"),
        );
        assert_eq!(c, "\"C:/Program Files/Kuntur/kuntur-hook.exe\" statusline");
    }

    #[test]
    fn archivo_invalido_no_se_toca() {
        assert!(plan(Some("{ esto no es json"), &exe()).is_err());
        assert!(plan(Some("[1, 2]"), &exe()).is_err());
        assert!(plan(Some(r#"{"hooks": 3}"#), &exe()).is_err());
        assert!(plan(Some(r#"{"hooks": {"Stop": {}}}"#), &exe()).is_err());
        // Vacío cuenta como nuevo.
        assert!(plan(Some("  "), &exe()).is_ok());
        // Con BOM (como lo guarda el Bloc de notas o PowerShell 5.1) sí es válido.
        let p = plan(Some("\u{feff}{\"model\": \"opus\"}"), &exe()).unwrap();
        assert!(p.new_text.starts_with('{'));
        assert!(p.new_text.contains("\"model\": \"opus\""));
    }

    /// Siempre sobre una carpeta temporal: nunca contra `~/.claude`.
    #[test]
    fn escribe_con_copia_de_seguridad_en_un_archivo_temporal() {
        let dir = std::env::temp_dir().join(format!("kuntur-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let settings = dir.join(".claude").join("settings.json");
        // Sin archivo: se crea la carpeta y no hay copia.
        let p = plan(None, &exe()).unwrap();
        assert_eq!(apply(&settings, &p, "1").unwrap(), None);
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), p.new_text);
        // Con archivo: copia idéntica al original.
        std::fs::write(&settings, r#"{"model": "sonnet"}"#).unwrap();
        let p = plan(Some(r#"{"model": "sonnet"}"#), &exe()).unwrap();
        let backup = apply(&settings, &p, "20261123-1200").unwrap().unwrap();
        assert!(backup.ends_with("settings.json.kuntur-20261123-1200.bak"));
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), r#"{"model": "sonnet"}"#);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(v["model"], "sonnet");
        assert!(v["hooks"]["SessionEnd"].is_array());
        assert!(!dir.join(".claude").join("settings.json.kuntur-tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
