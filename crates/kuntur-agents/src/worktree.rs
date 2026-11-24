//! Worktree por agente (F5b): cada agente nuevo puede trabajar en su propio `git worktree`, en una
//! carpeta hermana del repositorio (`<repo>-<agente>-<n>`) y en una rama nueva (`kuntur/<agente>-<n>`).
//! La terminal acoplada de esa sesión arranca en el worktree.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreePlan {
    /// Raíz del repositorio de origen.
    pub repo: PathBuf,
    pub path: PathBuf,
    pub branch: String,
}

/// Raíz del repositorio que contiene `dir` (sube hasta encontrar `.git`, carpeta o archivo).
pub fn find_repo_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().find(|d| d.join(".git").exists()).map(Path::to_path_buf)
}

/// Parte segura de un id de agente para nombres de carpeta y rama (`extra:aider` → `aider`).
pub fn slug(agent: &str) -> String {
    let base = agent.rsplit(':').next().unwrap_or(agent);
    let s: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "agente".into() } else { s }
}

/// Primer `n` libre: ni la carpeta ni la rama existen.
pub fn plan_worktree(
    repo: &Path,
    agent: &str,
    path_exists: impl Fn(&Path) -> bool,
    branch_exists: impl Fn(&str) -> bool,
) -> Option<WorktreePlan> {
    let name = repo.file_name()?.to_string_lossy().into_owned();
    let parent = repo.parent()?;
    let slug = slug(agent);
    (1..1000).find_map(|n| {
        let path = parent.join(format!("{name}-{slug}-{n}"));
        let branch = format!("kuntur/{slug}-{n}");
        (!path_exists(&path) && !branch_exists(&branch)).then(|| WorktreePlan {
            repo: repo.to_path_buf(),
            path,
            branch,
        })
    })
}

fn git(repo: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Sin ventana de consola.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// ¿Existe la rama en el repositorio?
pub fn branch_exists(repo: &Path, branch: &str) -> bool {
    git(repo)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("refs/heads/{branch}"))
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Planifica y crea el worktree del agente para el repositorio que contiene `dir`.
pub fn create_for(dir: &Path, agent: &str) -> Result<WorktreePlan, String> {
    let repo =
        find_repo_root(dir).ok_or_else(|| "La carpeta de la sesión no está en un repositorio git".to_string())?;
    let plan = plan_worktree(&repo, agent, Path::exists, |b| branch_exists(&repo, b))
        .ok_or_else(|| "No hay un nombre libre para el worktree".to_string())?;
    let out = git(&repo)
        .args(["worktree", "add", "-b", &plan.branch])
        .arg(&plan.path)
        .output()
        .map_err(|e| format!("No se pudo ejecutar git: {e}"))?;
    if out.status.success() {
        Ok(plan)
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("git worktree add falló: {}", err.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres_libres_en_carpeta_hermana() {
        let repo = Path::new(r"D:\proy\web");
        let taken = [PathBuf::from(r"D:\proy\web-claude-1")];
        let plan = plan_worktree(
            repo,
            "claude",
            |p| taken.iter().any(|t| t == p),
            |b| b == "kuntur/claude-2",
        )
        .unwrap();
        assert_eq!(plan.path, PathBuf::from(r"D:\proy\web-claude-3"));
        assert_eq!(plan.branch, "kuntur/claude-3");
        assert_eq!(plan.repo, repo);
        assert_eq!(slug("extra:Mi Agente"), "mi-agente");
        assert_eq!(slug("::"), "agente");
        assert!(plan_worktree(Path::new("C:\\"), "claude", |_| false, |_| false).is_none());
    }

    /// Prueba real con un repositorio temporal (necesita git en el PATH).
    #[test]
    fn crea_un_worktree_real() {
        if Command::new("git").arg("--version").output().is_err() {
            eprintln!("git no está instalado: se omite");
            return;
        }
        let base = std::env::temp_dir().join(format!("kuntur-wt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("repo");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        let run = |args: &[&str]| {
            let ok = git(&repo)
                .args(args)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            assert!(ok, "git {args:?}");
        };
        run(&["init", "-q"]);
        std::fs::write(repo.join("src").join("a.txt"), "hola").unwrap();
        run(&["add", "."]);
        run(&[
            "-c",
            "user.name=Prueba",
            "-c",
            "user.email=prueba@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "inicio",
        ]);
        let plan = create_for(&repo.join("src"), "claude").unwrap();
        assert_eq!(plan.path, base.join("repo-claude-1"));
        assert!(plan.path.join("src").join("a.txt").exists());
        assert!(branch_exists(&repo, "kuntur/claude-1"));
        // El segundo toma el siguiente número.
        let second = create_for(&repo, "claude").unwrap();
        assert_eq!(second.branch, "kuntur/claude-2");
        assert!(create_for(&base, "claude").is_err(), "fuera de un repositorio");
        for wt in [&plan.path, &second.path] {
            let _ = git(&repo).args(["worktree", "remove", "--force"]).arg(wt).output();
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
