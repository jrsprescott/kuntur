//! Auditoría **sintética** para las pruebas de humo y las capturas (`KUNTUR_SMOKE_AUDIT`): nunca se
//! muestran comandos reales del usuario en una captura. Determinista: la misma fecha da siempre los
//! mismos registros.

use std::sync::Arc;

use kuntur_usage::AgentKind;

use crate::model::{AuditRecord, Interner, LOCAL_HOST, dedup};

/// Días hacia atrás que cubre (la ventana de 30 días y algo más, que la vista no muestra).
pub const DAYS: i64 = 45;

/// Títulos de chats inventados.
const CHATS: [&str; 6] = [
    "Pagos con tarjeta en la tienda",
    "Pruebas de la API de pagos",
    "Arreglar el build de la app",
    "Migrar la base de datos",
    "Revisión del pipeline de CI",
    "Limpieza de dependencias",
];

const CLAUDE_COMMANDS: [&str; 10] = [
    "cargo test --workspace",
    "git status",
    "git diff --stat",
    "Get-ChildItem -Recurse -Filter *.rs | Measure-Object",
    "npm run build",
    "cargo clippy --all-targets -- -D warnings",
    "git log --oneline -5",
    "rg \"TODO\" src",
    "python -m pytest -q",
    "Remove-Item -Recurse -Force .\\target\\tmp",
];
const CODEX_COMMANDS: [&str; 8] = [
    "cargo build --release",
    "ls -la",
    "git add -A && git commit -m \"wip\"",
    "npm test",
    "cat package.json",
    "go test ./...",
    "docker compose up -d",
    "pnpm install",
];
const FOLDERS: [&str; 4] = [
    "C:\\proyectos\\tienda-web",
    "C:\\proyectos\\api-pagos",
    "C:\\proyectos\\app-movil",
    "/srv/demo/backend",
];

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n.max(1)
    }
}

/// Comandos de los últimos `DAYS` días hasta `now` (ninguno posterior), del más nuevo al más viejo.
pub fn records(now: i64) -> Vec<Arc<AuditRecord>> {
    let mut rng = Rng(0x61_7564_6974);
    let mut shared = Interner::default();
    let mut out = Vec::new();
    for day in 0..DAYS {
        let per_day = 2 + rng.below(9);
        for i in 0..per_day {
            let ts = now - day * 86_400 - rng.below(86_400) as i64 - 60;
            let codex = rng.below(100) < 35;
            let (agent, command, tool) = if codex {
                let c = CODEX_COMMANDS[rng.below(CODEX_COMMANDS.len() as u64) as usize];
                (AgentKind::Codex, c, "shell_command")
            } else {
                let c = CLAUDE_COMMANDS[rng.below(CLAUDE_COMMANDS.len() as u64) as usize];
                let tool = if c.contains('-') && c.starts_with(char::is_uppercase) {
                    "PowerShell"
                } else {
                    "Bash"
                };
                (AgentKind::ClaudeCode, c, tool)
            };
            let folder = FOLDERS[rng.below(FOLDERS.len() as u64) as usize];
            let chat = rng.below(CHATS.len() as u64) as usize;
            out.push(Arc::new(AuditRecord {
                ts,
                agent,
                session: shared.get(&format!("demo-{chat:04x}")),
                chat: Some(shared.get(CHATS[chat])),
                host: LOCAL_HOST,
                folder: Some(shared.get(folder)),
                command: command.into(),
                tool,
                key: Some(format!("demo-{day}-{i}").into()),
            }));
        }
    }
    dedup(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministas_y_sin_futuro() {
        let now = 1_795_000_000;
        let a = records(now);
        assert_eq!(a, records(now));
        assert!(a.len() > 2 * DAYS as usize);
        assert!(a.iter().all(|r| r.ts < now && r.ts > now - (DAYS + 1) * 86_400));
        assert!(a.windows(2).all(|w| w[0].ts >= w[1].ts));
        assert!(a.iter().any(|r| r.agent == AgentKind::Codex));
        assert!(a.iter().any(|r| r.agent == AgentKind::ClaudeCode));
    }
}
