//! Lecturas periódicas fuera del hilo de la UI: agentes corriendo en cada terminal (principal y
//! acoplada, por el árbol de procesos), memoria de Kuntur, agentes instalados y límites de Codex.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use kuntur_agents::{Catalog, ProcInfo, RunningAgent, find_agents};
use kuntur_usage::codex::Limits;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// El shell de una terminal: la principal de una sesión o su acoplada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellRef {
    pub session: u64,
    pub docked: bool,
    pub pid: u32,
}

/// Resultado de una lectura.
#[derive(Clone, Debug, Default)]
pub struct Sample {
    /// (sesión, ¿acoplada?, agente), en el orden de las terminales pedidas.
    pub agents: Vec<(u64, bool, RunningAgent)>,
    /// Terminales (sesión, ¿acoplada?) cuyo shell tiene procesos hijos: no se les escribe un comando.
    pub busy: HashSet<(u64, bool)>,
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

/// Terminales cuyo shell tiene algún proceso descendiente (otro programa ocupa la terminal).
pub fn busy_shells(shells: &[ShellRef], procs: &[ProcInfo]) -> HashSet<(u64, bool)> {
    shells
        .iter()
        .filter(|s| !descendants(&[s.pid], procs).is_empty())
        .map(|s| (s.session, s.docked))
        .collect()
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

    /// Lee procesos y memoria de las terminales `shells`, reconociendo los agentes de `catalog`.
    pub fn sample(&mut self, shells: &[ShellRef], catalog: &Catalog) -> Sample {
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
        let roots: Vec<u32> = shells.iter().map(|s| s.pid).collect();
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
            .flat_map(|s| {
                find_agents(catalog, s.pid, &procs)
                    .into_iter()
                    .map(move |a| (s.session, s.docked, a))
            })
            .collect();
        let busy = busy_shells(shells, &procs);
        Sample {
            agents,
            busy,
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

/// Lecturas lentas que cambian poco: agentes instalados (ids del catálogo) y límites de Codex.
#[derive(Clone, Debug, Default)]
pub struct SlowSample {
    pub installed: Vec<String>,
    pub codex_limits: Option<Limits>,
    /// IA local (ajustes v0.2.1): proveedores instalados y sus modelos (solo `localhost` y disco).
    pub local: Vec<kuntur_agents::ProviderStatus>,
}

pub fn slow_sample(catalog: &Catalog) -> SlowSample {
    SlowSample {
        installed: catalog.installed(),
        codex_limits: codex_sessions_dir().and_then(|d| kuntur_usage::codex::latest_limits(&d)),
        local: home_dir()
            .map(|h| kuntur_agents::local::detect_now(&h))
            .unwrap_or_default(),
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
    fn terminales_ocupadas() {
        let procs = vec![p(1, 0, 100), p(2, 1, 110), p(4, 0, 100), p(7, 4, 50)];
        let shell = |session, docked, pid| ShellRef { session, docked, pid };
        let busy = busy_shells(&[shell(1, false, 1), shell(1, true, 4), shell(2, false, 9)], &procs);
        // La acoplada solo tiene un "hijo" con PID reutilizado: está libre.
        assert_eq!(busy, HashSet::from([(1, false)]));
    }

    #[test]
    fn lee_la_memoria_propia() {
        let mut s = Sampler::new();
        let me = ShellRef {
            session: 1,
            docked: false,
            pid: std::process::id(),
        };
        let sample = s.sample(&[me], &Catalog::builtin());
        assert!(sample.ram.is_some_and(|b| b > 0));
        assert!(sample.agents.is_empty());
    }
}
