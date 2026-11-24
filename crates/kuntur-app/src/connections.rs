//! Lógica del gestor de conexiones (§8): formulario de alta/edición, lista guardada y paso de una
//! conexión guardada a un destino SSH. Sin GPUI: se prueba aparte; la vista vive en
//! `workspace/connections_view.rs`.

use std::path::{Path, PathBuf};

use kuntur_agents::{AgentKind, CATALOG};
use kuntur_config::{ConnectionConfig, parse_destination, parse_port};
use kuntur_ssh::SshTarget;

/// Campos del formulario, en el orden de Tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Host,
    Port,
    User,
    Key,
}

pub const FIELDS: [Field; 5] = [Field::Name, Field::Host, Field::Port, Field::User, Field::Key];

impl Field {
    pub fn label(self) -> &'static str {
        match self {
            Field::Name => "Nombre",
            Field::Host => "Host",
            Field::Port => "Puerto",
            Field::User => "Usuario",
            Field::Key => "Llave (opcional)",
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Field::Name => "produccion",
            Field::Host => "servidor.ejemplo.com o usuario@host:puerto",
            Field::Port => "22",
            Field::User => "deploy",
            Field::Key => "agente o ~/.ssh/id_ed25519",
        }
    }

    fn max_len(self) -> usize {
        match self {
            Field::Port => 5,
            Field::Key => 260,
            _ => 120,
        }
    }
}

/// Formulario abierto (nueva conexión o edición de una guardada).
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionForm {
    /// Nombre original si se está editando (para reemplazarla y renombrar sus pestañas).
    pub editing: Option<String>,
    pub name: String,
    pub host: String,
    pub port: String,
    pub user: String,
    pub key: String,
    pub remember: bool,
    pub focus: Field,
    pub error: Option<String>,
}

impl Default for ConnectionForm {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionForm {
    pub fn new() -> Self {
        Self {
            editing: None,
            name: String::new(),
            host: String::new(),
            port: "22".into(),
            user: String::new(),
            key: String::new(),
            remember: false,
            focus: Field::Name,
            error: None,
        }
    }

    pub fn edit(c: &ConnectionConfig) -> Self {
        Self {
            editing: Some(c.name.clone()),
            name: c.name.clone(),
            host: c.host.clone(),
            port: c.port.to_string(),
            user: c.user.clone(),
            key: c.key.clone().unwrap_or_default(),
            remember: c.remember,
            focus: Field::Name,
            error: None,
        }
    }

    pub fn value(&self, f: Field) -> &str {
        match f {
            Field::Name => &self.name,
            Field::Host => &self.host,
            Field::Port => &self.port,
            Field::User => &self.user,
            Field::Key => &self.key,
        }
    }

    fn value_mut(&mut self, f: Field) -> &mut String {
        match f {
            Field::Name => &mut self.name,
            Field::Host => &mut self.host,
            Field::Port => &mut self.port,
            Field::User => &mut self.user,
            Field::Key => &mut self.key,
        }
    }

    /// Texto escrito o pegado en el campo con foco (una línea, sin controles). En el puerto solo
    /// dígitos. Pegar `usuario@host:puerto` en el host reparte las partes.
    pub fn insert(&mut self, text: &str) {
        self.error = None;
        let line: String = text
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        if self.focus == Field::Host
            && (line.contains('@') || line.contains(':'))
            && self.host.is_empty()
            && let Ok((user, host, port)) = parse_destination(&line)
        {
            self.host = host;
            if let Some(u) = user {
                self.user = u;
            }
            if let Some(p) = port {
                self.port = p.to_string();
            }
            if self.name.trim().is_empty() {
                self.name = self.host.clone();
            }
            return;
        }
        let f = self.focus;
        let max = f.max_len();
        let target = self.value_mut(f);
        for c in line.chars() {
            if target.chars().count() >= max {
                break;
            }
            if f == Field::Port && !c.is_ascii_digit() {
                continue;
            }
            target.push(c);
        }
    }

    pub fn backspace(&mut self) {
        self.error = None;
        let f = self.focus;
        self.value_mut(f).pop();
    }

    pub fn clear_field(&mut self) {
        let f = self.focus;
        self.value_mut(f).clear();
    }

    pub fn next_field(&mut self) {
        let i = FIELDS.iter().position(|f| *f == self.focus).unwrap_or(0);
        self.focus = FIELDS[(i + 1) % FIELDS.len()];
    }

    pub fn prev_field(&mut self) {
        let i = FIELDS.iter().position(|f| *f == self.focus).unwrap_or(0);
        self.focus = FIELDS[(i + FIELDS.len() - 1) % FIELDS.len()];
    }

    /// La conexión lista para guardar, o el error para mostrar en el formulario.
    pub fn to_config(&self, saved: &[ConnectionConfig]) -> Result<ConnectionConfig, String> {
        let port = parse_port(&self.port).map_err(|e| e.to_string())?;
        let conn = ConnectionConfig {
            name: self.name.clone(),
            host: self.host.clone(),
            port,
            user: self.user.clone(),
            key: Some(self.key.clone()),
            remember: self.remember,
        }
        .normalized();
        let others = saved
            .iter()
            .filter(|c| Some(&c.name) != self.editing.as_ref())
            .map(|c| c.name.as_str());
        conn.validate(others).map_err(|e| e.to_string())?;
        Ok(conn)
    }
}

/// Guarda la conexión: reemplaza la original (si se editó) o la agrega al final.
pub fn upsert(list: &mut Vec<ConnectionConfig>, original: Option<&str>, conn: ConnectionConfig) {
    match original.and_then(|name| list.iter().position(|c| c.name == name)) {
        Some(i) => list[i] = conn,
        None => list.push(conn),
    }
}

/// `~` al principio de la ruta de la llave = la carpeta personal.
pub fn expand_home(path: &str, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")), home) {
        (Some(rest), Some(h)) => h.join(rest),
        _ => PathBuf::from(path),
    }
}

pub fn target_for(c: &ConnectionConfig, home: Option<&Path>) -> SshTarget {
    SshTarget {
        host: c.host.clone(),
        port: c.port,
        user: c.user.clone(),
        key: c.key.as_deref().map(|k| expand_home(k, home)),
        remember: c.remember,
    }
}

/// Agentes del servidor: los del catálogo (con insignia) y el resto por nombre.
pub fn remote_agent_kinds(names: &[String]) -> (Vec<AgentKind>, Vec<String>) {
    let mut kinds = Vec::new();
    let mut others = Vec::new();
    for n in names {
        match CATALOG.iter().find(|i| i.exe_names.contains(&n.as_str())) {
            Some(info) => kinds.push(info.kind),
            None => others.push(n.clone()),
        }
    }
    (kinds, others)
}

/// Conexiones de mentira para las pruebas de humo (dominios `.test` reservados y `127.0.0.1`).
pub fn synthetic_connections() -> Vec<ConnectionConfig> {
    let c = |name: &str, host: &str, port: u16, user: &str, key: Option<&str>, remember: bool| ConnectionConfig {
        name: name.into(),
        host: host.into(),
        port,
        user: user.into(),
        key: key.map(Into::into),
        remember,
    };
    vec![
        c(
            "produccion",
            "app.ejemplo.test",
            22,
            "deploy",
            Some("~/.ssh/id_ed25519"),
            false,
        ),
        c("staging", "staging.ejemplo.test", 2222, "ubuntu", None, true),
        c("gpu-lab", "10.20.30.40", 22, "investigacion", None, false),
        // Puerto 1 de la propia máquina: nadie escucha, sirve para ver el error de conexión.
        c("local-apagado", "127.0.0.1", 1, "demo", None, false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved() -> Vec<ConnectionConfig> {
        synthetic_connections()
    }

    #[test]
    fn formulario_nuevo_y_tab() {
        let mut f = ConnectionForm::new();
        assert_eq!(f.port, "22");
        assert_eq!(f.focus, Field::Name);
        f.insert("web");
        f.next_field();
        f.insert("srv.ejemplo.test");
        f.next_field();
        assert_eq!(f.focus, Field::Port);
        f.clear_field();
        f.insert("22a22");
        assert_eq!(f.port, "2222", "solo dígitos");
        f.next_field();
        f.insert("deploy");
        f.next_field();
        f.next_field();
        assert_eq!(f.focus, Field::Name, "da la vuelta");
        f.prev_field();
        assert_eq!(f.focus, Field::Key);
        let c = f.to_config(&saved()).unwrap();
        assert_eq!(c.destination(), "deploy@srv.ejemplo.test:2222");
        assert_eq!(c.key, None, "llave vacía = agente o las de ~/.ssh");
    }

    #[test]
    fn pegar_un_destino_completo_en_el_host() {
        let mut f = ConnectionForm::new();
        f.focus = Field::Host;
        f.insert("root@10.0.0.7:2200\n");
        assert_eq!(
            (f.host.as_str(), f.user.as_str(), f.port.as_str()),
            ("10.0.0.7", "root", "2200")
        );
        assert_eq!(f.name, "10.0.0.7", "sin nombre, usa el host");
    }

    #[test]
    fn errores_en_palabras() {
        let mut f = ConnectionForm::new();
        assert_eq!(f.to_config(&[]).unwrap_err(), "Falta el nombre");
        f.name = "produccion".into();
        f.host = "h.test".into();
        f.user = "u".into();
        assert_eq!(f.to_config(&saved()).unwrap_err(), "Ya hay una conexión con ese nombre");
        f.port = "0".into();
        assert_eq!(f.to_config(&[]).unwrap_err(), "El puerto va de 1 a 65535");
        f.insert("x");
        assert_eq!(f.error, None, "escribir borra el error");
    }

    #[test]
    fn editar_conserva_el_nombre_y_reemplaza() {
        let mut list = saved();
        let mut f = ConnectionForm::edit(&list[1]);
        assert_eq!(f.editing.as_deref(), Some("staging"));
        assert_eq!(f.port, "2222");
        assert!(f.remember);
        // Guardar sin cambiar el nombre no choca consigo misma.
        f.user = "admin".into();
        let c = f.to_config(&list).unwrap();
        upsert(&mut list, f.editing.as_deref(), c);
        assert_eq!(list.len(), 4);
        assert_eq!(list[1].user, "admin");
        // Una nueva va al final.
        let mut nueva = ConnectionForm::new();
        nueva.name = "otra".into();
        nueva.host = "o.test".into();
        nueva.user = "u".into();
        let c = nueva.to_config(&list).unwrap();
        upsert(&mut list, None, c);
        assert_eq!(list.last().unwrap().name, "otra");
    }

    #[test]
    fn llave_con_tilde_y_destino() {
        let home = Path::new("C:/Users/demo");
        assert_eq!(expand_home("~/.ssh/id_rsa", Some(home)), home.join(".ssh/id_rsa"));
        assert_eq!(expand_home("~\\k", Some(home)), home.join("k"));
        assert_eq!(expand_home("D:/k", Some(home)), PathBuf::from("D:/k"));
        let t = target_for(&saved()[0], Some(home));
        assert_eq!(t.key, Some(home.join(".ssh/id_ed25519")));
        assert_eq!(t.destination(), "deploy@app.ejemplo.test");
        assert_eq!(target_for(&saved()[1], Some(home)).key, None);
    }

    #[test]
    fn agentes_del_servidor_con_y_sin_insignia() {
        let (kinds, others) = remote_agent_kinds(&["claude".into(), "kimi".into(), "codex".into()]);
        assert_eq!(kinds, vec![AgentKind::ClaudeCode, AgentKind::Codex]);
        assert_eq!(others, vec!["kimi"]);
    }

    #[test]
    fn las_sinteticas_son_validas_y_no_reales() {
        let list = saved();
        for (i, c) in list.iter().enumerate() {
            let others: Vec<&str> = list
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, c)| c.name.as_str())
                .collect();
            assert_eq!(c.validate(others), Ok(()));
            assert!(c.host.ends_with(".test") || c.host.starts_with("10.") || c.host == "127.0.0.1");
        }
    }
}
