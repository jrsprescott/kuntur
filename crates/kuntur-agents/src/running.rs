//! Agentes corriendo dentro de una sesión: descendientes del shell de la sesión en el árbol de procesos.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use kuntur_usage::AgentKind;
use sysinfo::System;

use crate::catalog::match_process;

/// Tiempo sin salida a partir del cual un agente se considera esperando.
pub const WORKING_WINDOW: Duration = Duration::from_secs(3);

/// Estado de un agente (heurística v1 por actividad de la sesión; luego hooks / OSC 7501).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentState {
    Working,
    Waiting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunningAgent {
    pub kind: AgentKind,
    pub pid: u32,
    /// Segundos Unix en que arrancó el proceso.
    pub started_at: u64,
}

/// Lo que se necesita de un proceso (copia independiente de `sysinfo`, para probar sin procesos reales).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: String,
    pub cmd: Vec<String>,
    pub start_time: u64,
}

/// `Working` si la sesión tuvo salida en los últimos 3 s; si no, `Waiting`.
pub fn state_for(last_output_age: Duration) -> AgentState {
    if last_output_age < WORKING_WINDOW {
        AgentState::Working
    } else {
        AgentState::Waiting
    }
}

/// Agentes entre los descendientes de `root` (sin contar `root`), ordenados por PID. Un agente que
/// corre dentro de otro del mismo tipo (p. ej. `node codex.js` → `codex.exe`) se cuenta una vez.
pub fn find_agents(root: u32, procs: &[ProcInfo]) -> Vec<RunningAgent> {
    let by_pid: HashMap<u32, &ProcInfo> = procs.iter().map(|p| (p.pid, p)).collect();
    let mut children: HashMap<u32, Vec<&ProcInfo>> = HashMap::new();
    for p in procs {
        let Some(parent) = p.parent.and_then(|pp| by_pid.get(&pp)) else {
            continue;
        };
        // Windows reutiliza PIDs: un "hijo" que arrancó antes que su padre es de otro proceso ya muerto.
        if p.start_time >= parent.start_time {
            children.entry(parent.pid).or_default().push(p);
        }
    }

    let mut found = Vec::new();
    let mut visited = HashSet::from([root]);
    // (proceso, agente del ancestro más cercano dentro de la sesión)
    let mut pending: Vec<(u32, Option<AgentKind>)> = vec![(root, None)];
    while let Some((pid, inherited)) = pending.pop() {
        for child in children.get(&pid).into_iter().flatten() {
            if !visited.insert(child.pid) {
                continue;
            }
            let kind = match_process(&child.name, &child.cmd);
            if let Some(kind) = kind.filter(|k| Some(*k) != inherited) {
                found.push(RunningAgent {
                    kind,
                    pid: child.pid,
                    started_at: child.start_time,
                });
            }
            pending.push((child.pid, kind.or(inherited)));
        }
    }
    found.sort_by_key(|a| a.pid);
    found
}

/// Agentes corriendo bajo el proceso `pid` (el shell de la sesión). `sys` debe tener los procesos
/// actualizados con su línea de comando (`ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always)`).
pub fn agents_under(pid: u32, sys: &System) -> Vec<RunningAgent> {
    let procs: Vec<ProcInfo> = sys
        .processes()
        .values()
        .map(|p| ProcInfo {
            pid: p.pid().as_u32(),
            parent: p.parent().map(|pp| pp.as_u32()),
            name: p.name().to_string_lossy().into_owned(),
            cmd: p.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect(),
            start_time: p.start_time(),
        })
        .collect();
    find_agents(pid, &procs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, parent: u32, name: &str, cmd: &[&str], start: u64) -> ProcInfo {
        ProcInfo {
            pid,
            parent: Some(parent),
            name: name.into(),
            cmd: cmd.iter().map(|s| s.to_string()).collect(),
            start_time: start,
        }
    }

    #[test]
    fn encuentra_agentes_descendientes_del_shell() {
        let procs = vec![
            p(1, 0, "pwsh.exe", &["pwsh.exe"], 100),
            p(2, 1, "cmd.exe", &["cmd.exe", "/c", "codex.cmd"], 110),
            p(
                3,
                2,
                "node.exe",
                &["node.exe", r"C:\npm\node_modules\@openai\codex\bin\codex.js"],
                111,
            ),
            // El binario nativo que lanza el script: mismo agente, no se repite.
            p(4, 3, "codex.exe", &["codex.exe"], 112),
            p(5, 1, "claude.exe", &["claude.exe"], 120),
            // Fuera de la sesión.
            p(6, 99, "claude.exe", &["claude.exe"], 130),
            // PID reutilizado: dice ser hijo del shell pero arrancó antes que él.
            p(7, 1, "opencode.exe", &["opencode.exe"], 50),
            // Un agente dentro de otro de distinto tipo sí cuenta.
            p(8, 5, "gemini.exe", &["gemini.exe"], 125),
        ];
        let found = find_agents(1, &procs);
        assert_eq!(
            found,
            vec![
                RunningAgent {
                    kind: AgentKind::Codex,
                    pid: 3,
                    started_at: 111
                },
                RunningAgent {
                    kind: AgentKind::ClaudeCode,
                    pid: 5,
                    started_at: 120
                },
                RunningAgent {
                    kind: AgentKind::Gemini,
                    pid: 8,
                    started_at: 125
                },
            ]
        );
        assert!(find_agents(42, &procs).is_empty());
    }

    #[test]
    fn el_shell_mismo_no_cuenta_y_los_ciclos_no_cuelgan() {
        let procs = vec![p(1, 2, "claude.exe", &[], 100), p(2, 1, "node.exe", &[], 100)];
        assert!(find_agents(1, &procs).is_empty());
    }

    #[test]
    fn estado_por_actividad_reciente() {
        assert_eq!(state_for(Duration::from_millis(500)), AgentState::Working);
        assert_eq!(state_for(Duration::from_secs(3)), AgentState::Waiting);
        assert_eq!(state_for(Duration::from_secs(60)), AgentState::Waiting);
    }

    #[test]
    fn lee_el_arbol_real_sin_fallar() {
        let mut sys = System::new().expect("sysinfo disponible");
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::nothing().with_cmd(sysinfo::UpdateKind::Always),
        );
        // El proceso de pruebas no tiene agentes como hijos.
        assert!(agents_under(std::process::id(), &sys).is_empty());
    }
}
