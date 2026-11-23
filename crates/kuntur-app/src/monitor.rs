//! Lecturas periódicas fuera del hilo de la UI: agentes corriendo en cada sesión (árbol de
//! procesos), memoria de Kuntur, agentes instalados y límites de Codex.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use kuntur_agents::{AgentKind, ProcInfo, RunningAgent, find_agents};
use kuntur_usage::codex::Limits;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// Resultado de una lectura.
#[derive(Clone, Debug, Default)]
pub struct Sample {
    /// (sesión, agente), en el orden de las sesiones pedidas.
    pub agents: Vec<(u64, RunningAgent)>,
    /// Memoria de trabajo de Kuntur (bytes).
    pub ram: Option<u64>,
}

/// PIDs de todos los descendientes de `roots` (sin incluirlos). Ignora "hijos" que arrancaron
/// antes que su padre: Windows reutiliza PIDs.
pub fn descendants(roots: &[u32], procs: &[ProcInfo]) -> HashSet<u32> {
    let start: HashMap<u32, u64> = procs.iter().map(|p| (p.pid, p.start_time)).collect();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in procs {
        if let Some(parent) = p.parent
            && start.get(&parent).is_some_and(|&s| p.start_time >= s)
        {
            children.entry(parent).or_default().push(p.pid);
        }
    }
    let mut found = HashSet::new();
    let mut pending: Vec<u32> = roots.to_vec();
    while let Some(pid) = pending.pop() {
        for &child in children.get(&pid).into_iter().flatten() {
            if !roots.contains(&child) && found.insert(child) {
                pending.push(child);
            }
        }
    }
    found
}

/// Conserva el estado de `sysinfo` entre lecturas (la línea de comando de cada proceso se lee una
/// sola vez).
pub struct Sampler {
    sys: Option<System>,
    own: Pid,
}

impl Sampler {
    pub fn new() -> Self {
        Self {
            sys: System::new().ok(),
            own: Pid::from_u32(std::process::id()),
        }
    }

    /// Lee procesos y memoria. `shells` = (id de sesión, PID del shell).
    pub fn sample(&mut self, shells: &[(u64, u32)]) -> Sample {
        let Some(sys) = self.sys.as_mut() else {
            return Sample::default();
        };
        // 1) Lista liviana de todos los procesos (nombre, padre, inicio).
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
        let snapshot = |sys: &System| -> Vec<ProcInfo> {
            sys.processes()
                .values()
                .map(|p| ProcInfo {
                    pid: p.pid().as_u32(),
                    parent: p.parent().map(|pp| pp.as_u32()),
                    name: p.name().to_string_lossy().into_owned(),
                    cmd: p.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect(),
                    start_time: p.start_time(),
                })
                .collect()
        };
        let roots: Vec<u32> = shells.iter().map(|&(_, pid)| pid).collect();
        let procs = snapshot(sys);
        // 2) Línea de comando solo de lo que corre dentro de las sesiones (`node codex.js`…).
        let inside: Vec<Pid> = descendants(&roots, &procs).into_iter().map(Pid::from_u32).collect();
        if !inside.is_empty() {
            sys.refresh_processes_specifics(
                ProcessesToUpdate::Some(&inside),
                false,
                ProcessRefreshKind::nothing().with_cmd(UpdateKind::OnlyIfNotSet),
            );
        }
        // 3) Memoria propia.
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.own]),
            false,
            ProcessRefreshKind::nothing().with_memory(),
        );
        let procs = if inside.is_empty() { procs } else { snapshot(sys) };
        let agents = shells
            .iter()
            .flat_map(|&(id, pid)| find_agents(pid, &procs).into_iter().map(move |a| (id, a)))
            .collect();
        Sample {
            agents,
            ram: sys.process(self.own).map(|p| p.memory()),
        }
    }
}

/// Carpeta personal (`%USERPROFILE%`).
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Carpeta de los rollouts de Codex. `KUNTUR_USAGE_ROOT` (pruebas) apunta a datos sintéticos:
/// `<raíz>\codex\sessions`.
pub fn codex_sessions_dir() -> Option<PathBuf> {
    match std::env::var_os("KUNTUR_USAGE_ROOT") {
        Some(root) => Some(PathBuf::from(root).join("codex").join("sessions")),
        None => home_dir().map(|h| h.join(".codex").join("sessions")),
    }
}

/// Lecturas lentas que cambian poco: agentes instalados y límites de Codex.
#[derive(Clone, Debug, Default)]
pub struct SlowSample {
    pub installed: Vec<AgentKind>,
    pub codex_limits: Option<Limits>,
}

pub fn slow_sample() -> SlowSample {
    SlowSample {
        installed: kuntur_agents::installed(),
        codex_limits: codex_sessions_dir().and_then(|d| kuntur_usage::codex::latest_limits(&d)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, parent: u32, start: u64) -> ProcInfo {
        ProcInfo {
            pid,
            parent: Some(parent),
            name: format!("p{pid}"),
            cmd: Vec::new(),
            start_time: start,
        }
    }

    #[test]
    fn descendientes_de_varias_sesiones() {
        let procs = vec![
            p(1, 0, 100),
            p(2, 1, 110),
            p(3, 2, 120),
            p(4, 0, 100),
            p(5, 4, 130),
            p(6, 99, 100),
            // PID reutilizado: arrancó antes que su "padre".
            p(7, 1, 50),
        ];
        let d = descendants(&[1, 4], &procs);
        assert_eq!(d, HashSet::from([2, 3, 5]));
        assert!(descendants(&[42], &procs).is_empty());
    }

    #[test]
    fn ciclos_no_cuelgan_y_las_raices_no_cuentan() {
        let procs = vec![p(1, 2, 100), p(2, 1, 100)];
        assert_eq!(descendants(&[1], &procs), HashSet::from([2]));
    }

    #[test]
    fn lee_la_memoria_propia() {
        let mut s = Sampler::new();
        let sample = s.sample(&[(1, std::process::id())]);
        assert!(sample.ram.is_some_and(|b| b > 0));
        assert!(sample.agents.is_empty());
    }
}
