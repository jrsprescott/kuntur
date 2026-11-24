//! Cola de transferencias: subir (local → SFTP), descargar (SFTP → local) y copiar (local → local).
//!
//! - [`Queue`] es lógica pura: estados, conflictos ("¿sobrescribir / renombrar / omitir?"),
//!   cancelar y el resumen para la barra de progreso.
//! - [`Transfers`] la ejecuta en un hilo propio, un trabajo a la vez, copiando por bloques entre
//!   dos [`FileSystem`] cualesquiera. Un trabajo con el destino ocupado queda esperando la
//!   respuesta del usuario **sin frenar** a los demás.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::fs::FileSystem;
use crate::path::conflict_name;

pub type JobId = u64;

/// Tamaño de cada bloque copiado.
const CHUNK: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Del disco local al servidor (SFTP).
    Upload,
    /// Del servidor al disco local.
    Download,
    /// De una carpeta local a otra.
    Copy,
}

impl Direction {
    /// Verbo para la fila de la cola.
    pub fn verb(self) -> &'static str {
        match self {
            Direction::Upload => "Subiendo",
            Direction::Download => "Descargando",
            Direction::Copy => "Copiando",
        }
    }
}

/// Qué hacer si el destino ya existe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    Overwrite,
    /// Guardar como `nombre (1).ext`.
    Rename,
    Skip,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobState {
    Queued,
    /// El destino existe: se espera "sobrescribir / renombrar / omitir".
    Asking,
    Running,
    Done,
    Skipped,
    Cancelled,
    Failed(String),
}

impl JobState {
    pub fn is_finished(&self) -> bool {
        matches!(
            self,
            JobState::Done | JobState::Skipped | JobState::Cancelled | JobState::Failed(_)
        )
    }
}

#[derive(Clone, Debug)]
pub struct Job {
    pub id: JobId,
    pub direction: Direction,
    /// Ruta de origen (archivo o carpeta).
    pub source: String,
    /// Carpeta de destino.
    pub dest_dir: String,
    /// Nombre en el destino (cambia con "Renombrar").
    pub name: String,
    /// Bytes a copiar (se conoce al empezar) y copiados.
    pub total: u64,
    pub done: u64,
    pub state: JobState,
    /// Respuesta al conflicto, si ya se dio.
    pub choice: Option<Conflict>,
    cancel: Arc<AtomicBool>,
}

impl Job {
    /// Avance de 0.0 a 1.0.
    pub fn fraction(&self) -> f32 {
        if self.state == JobState::Done {
            return 1.0;
        }
        if self.total == 0 {
            return 0.0;
        }
        (self.done as f64 / self.total as f64).clamp(0.0, 1.0) as f32
    }
}

/// Resumen para el encabezado de la cola.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// En curso o en espera de turno.
    pub active: usize,
    /// Esperando "sobrescribir / renombrar / omitir".
    pub asking: usize,
    pub finished: usize,
    pub failed: usize,
    /// Bytes de los trabajos activos.
    pub done_bytes: u64,
    pub total_bytes: u64,
}

/// La cola en sí (sin hilos ni archivos).
#[derive(Default)]
pub struct Queue {
    jobs: Vec<Job>,
    next_id: JobId,
    /// "Hacer lo mismo con los demás": respuesta para todos los conflictos siguientes.
    policy: Option<Conflict>,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn jobs(&self) -> &[Job] {
        &self.jobs
    }

    pub fn get(&self, id: JobId) -> Option<&Job> {
        self.jobs.iter().find(|j| j.id == id)
    }

    fn get_mut(&mut self, id: JobId) -> Option<&mut Job> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }

    /// Agrega un trabajo en espera de turno.
    pub fn push(&mut self, direction: Direction, source: &str, name: &str, dest_dir: &str) -> JobId {
        self.next_id += 1;
        self.jobs.push(Job {
            id: self.next_id,
            direction,
            source: source.into(),
            dest_dir: dest_dir.into(),
            name: name.into(),
            total: 0,
            done: 0,
            state: JobState::Queued,
            choice: None,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        self.next_id
    }

    /// Siguiente trabajo en espera: pasa a "en curso".
    pub fn take_next(&mut self) -> Option<JobId> {
        let job = self.jobs.iter_mut().find(|j| j.state == JobState::Queued)?;
        job.state = JobState::Running;
        Some(job.id)
    }

    /// ¿Queda algo por hacer sin preguntar?
    pub fn has_queued(&self) -> bool {
        self.jobs.iter().any(|j| j.state == JobState::Queued)
    }

    /// ¿Hay algo en curso o en espera de turno?
    pub fn is_busy(&self) -> bool {
        self.jobs
            .iter()
            .any(|j| matches!(j.state, JobState::Queued | JobState::Running))
    }

    /// El destino de `id` existe: devuelve la respuesta ya dada (o la de "para todos"); si no hay,
    /// el trabajo queda preguntando y devuelve `None`.
    pub fn on_conflict(&mut self, id: JobId) -> Option<Conflict> {
        let policy = self.policy;
        let job = self.get_mut(id)?;
        match job.choice.or(policy) {
            Some(c) => Some(c),
            None => {
                job.state = JobState::Asking;
                None
            }
        }
    }

    /// Respuesta del usuario a un conflicto; con `for_all`, también para los que pregunten después
    /// y los que ya estaban preguntando. Los trabajos vuelven a la cola.
    pub fn resolve(&mut self, id: JobId, choice: Conflict, for_all: bool) {
        if for_all {
            self.policy = Some(choice);
        }
        for job in &mut self.jobs {
            if job.state == JobState::Asking && (job.id == id || for_all) {
                job.choice = Some(choice);
                job.state = JobState::Queued;
            }
        }
    }

    pub fn set_total(&mut self, id: JobId, total: u64) {
        if let Some(j) = self.get_mut(id) {
            j.total = total;
        }
    }

    pub fn progress(&mut self, id: JobId, done: u64) {
        if let Some(j) = self.get_mut(id) {
            j.done = done;
        }
    }

    /// El trabajo se guardará con otro nombre ("Renombrar").
    pub fn rename_to(&mut self, id: JobId, name: &str) {
        if let Some(j) = self.get_mut(id) {
            j.name = name.into();
        }
    }

    /// Fin de un trabajo en curso.
    pub fn finish(&mut self, id: JobId, result: Result<(), String>) {
        if let Some(j) = self.get_mut(id) {
            j.state = if j.cancel.load(Ordering::SeqCst) {
                JobState::Cancelled
            } else {
                match result {
                    Ok(()) => JobState::Done,
                    Err(e) => JobState::Failed(e),
                }
            };
        }
    }

    pub fn skip(&mut self, id: JobId) {
        if let Some(j) = self.get_mut(id) {
            j.state = JobState::Skipped;
        }
    }

    /// Cancela: si aún no empezó, en el acto; si está en curso, al terminar el bloque actual (y se
    /// borra lo copiado a medias).
    pub fn cancel(&mut self, id: JobId) {
        if let Some(j) = self.get_mut(id) {
            j.cancel.store(true, Ordering::SeqCst);
            if matches!(j.state, JobState::Queued | JobState::Asking) {
                j.state = JobState::Cancelled;
            }
        }
    }

    pub(crate) fn cancel_flag(&self, id: JobId) -> Option<Arc<AtomicBool>> {
        self.get(id).map(|j| j.cancel.clone())
    }

    /// Quita de la lista lo terminado (hecho, omitido, cancelado o con error).
    pub fn clear_finished(&mut self) {
        self.jobs.retain(|j| !j.state.is_finished());
        if !self.jobs.iter().any(|j| j.state == JobState::Asking) && !self.is_busy() {
            self.policy = None;
        }
    }

    pub fn summary(&self) -> Summary {
        let mut s = Summary::default();
        for j in &self.jobs {
            match &j.state {
                JobState::Queued | JobState::Running => {
                    s.active += 1;
                    s.done_bytes += j.done;
                    s.total_bytes += j.total;
                }
                JobState::Asking => s.asking += 1,
                JobState::Failed(_) => {
                    s.failed += 1;
                    s.finished += 1;
                }
                _ => s.finished += 1,
            }
        }
        s
    }

    /// Trabajo ya armado (estado y avance a mano): solo para las pruebas de humo, que muestran una
    /// cola **sintética**.
    pub fn push_synthetic(
        &mut self,
        direction: Direction,
        name: &str,
        dest_dir: &str,
        total: u64,
        done: u64,
        state: JobState,
    ) {
        let id = self.push(direction, name, name, dest_dir);
        let j = self.get_mut(id).unwrap();
        j.total = total;
        j.done = done;
        j.state = state;
    }
}

/// Origen y destino de cada trabajo.
type Ends = (Arc<dyn FileSystem>, Arc<dyn FileSystem>);

struct Inner {
    queue: Mutex<Queue>,
    ends: Mutex<HashMap<JobId, Ends>>,
    worker: AtomicBool,
}

/// La cola con su hilo trabajador. Clonar comparte la misma cola.
#[derive(Clone)]
pub struct Transfers {
    inner: Arc<Inner>,
}

impl Default for Transfers {
    fn default() -> Self {
        Self::new()
    }
}

impl Transfers {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                queue: Mutex::new(Queue::new()),
                ends: Mutex::new(HashMap::new()),
                worker: AtomicBool::new(false),
            }),
        }
    }

    /// Copia `source` (archivo o carpeta de `src`) dentro de la carpeta `dest_dir` de `dst`.
    pub fn enqueue(
        &self,
        direction: Direction,
        src: Arc<dyn FileSystem>,
        source: &str,
        dst: Arc<dyn FileSystem>,
        dest_dir: &str,
    ) -> JobId {
        let name = src.file_name(source).unwrap_or_else(|| source.to_string());
        let id = self
            .inner
            .queue
            .lock()
            .unwrap()
            .push(direction, source, &name, dest_dir);
        self.inner.ends.lock().unwrap().insert(id, (src, dst));
        self.kick();
        id
    }

    pub fn resolve(&self, id: JobId, choice: Conflict, for_all: bool) {
        self.inner.queue.lock().unwrap().resolve(id, choice, for_all);
        self.kick();
    }

    pub fn cancel(&self, id: JobId) {
        self.inner.queue.lock().unwrap().cancel(id);
    }

    pub fn clear_finished(&self) {
        let mut q = self.inner.queue.lock().unwrap();
        q.clear_finished();
        let alive: Vec<JobId> = q.jobs().iter().map(|j| j.id).collect();
        self.inner.ends.lock().unwrap().retain(|id, _| alive.contains(id));
    }

    /// Copia de los trabajos para dibujar.
    pub fn snapshot(&self) -> Vec<Job> {
        self.inner.queue.lock().unwrap().jobs().to_vec()
    }

    pub fn summary(&self) -> Summary {
        self.inner.queue.lock().unwrap().summary()
    }

    pub fn job(&self, id: JobId) -> Option<Job> {
        self.inner.queue.lock().unwrap().get(id).cloned()
    }

    pub fn is_busy(&self) -> bool {
        self.inner.queue.lock().unwrap().is_busy()
    }

    /// Acceso directo a la cola (pruebas de humo con datos sintéticos).
    pub fn with_queue<R>(&self, f: impl FnOnce(&mut Queue) -> R) -> R {
        f(&mut self.inner.queue.lock().unwrap())
    }

    /// Arranca el hilo trabajador si no está corriendo.
    fn kick(&self) {
        if !self.inner.queue.lock().unwrap().has_queued() {
            return;
        }
        if self
            .inner
            .worker
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let me = self.clone();
        let spawned = std::thread::Builder::new()
            .name("kuntur-transferencias".into())
            .spawn(move || me.work());
        if spawned.is_err() {
            self.inner.worker.store(false, Ordering::SeqCst);
        }
    }

    fn work(&self) {
        loop {
            let next = self.inner.queue.lock().unwrap().take_next();
            match next {
                Some(id) => self.run(id),
                None => {
                    self.inner.worker.store(false, Ordering::SeqCst);
                    // Por si llegó algo entre "no hay nada" y soltar la marca.
                    let again = self.inner.queue.lock().unwrap().has_queued();
                    if !again
                        || self
                            .inner
                            .worker
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_err()
                    {
                        return;
                    }
                }
            }
        }
    }

    fn run(&self, id: JobId) {
        let ends = self.inner.ends.lock().unwrap().get(&id).cloned();
        let job = self.inner.queue.lock().unwrap().get(id).cloned();
        let (Some((src, dst)), Some(job)) = (ends, job) else {
            return;
        };
        let cancel = self.inner.queue.lock().unwrap().cancel_flag(id).unwrap_or_default();
        let result = self.run_job(&job, src.as_ref(), dst.as_ref(), &cancel);
        let mut q = self.inner.queue.lock().unwrap();
        match result {
            Ok(Step::Copied) => q.finish(id, Ok(())),
            Ok(Step::Asking) => {}
            Ok(Step::Skipped) => q.skip(id),
            Err(e) => q.finish(id, Err(friendly(&e))),
        }
    }

    fn run_job(&self, job: &Job, src: &dyn FileSystem, dst: &dyn FileSystem, cancel: &AtomicBool) -> io::Result<Step> {
        let id = job.id;
        let Some(origin) = src.stat(&job.source)? else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "el origen ya no existe"));
        };
        let total = if origin.is_dir {
            tree_size(src, &job.source)?
        } else {
            origin.size
        };
        self.inner.queue.lock().unwrap().set_total(id, total);

        if dst.stat(&job.dest_dir)?.is_none() {
            dst.create_dir_all(&job.dest_dir)?;
        }
        let mut name = job.name.clone();
        let mut dest = dst.join(&job.dest_dir, &name);
        if job.direction == Direction::Copy && origin.is_dir && is_inside(&job.dest_dir, &job.source) {
            return Err(io::Error::other("no se puede copiar una carpeta dentro de sí misma"));
        }
        if let Some(existing) = dst.stat(&dest)? {
            let choice = self.inner.queue.lock().unwrap().on_conflict(id);
            match choice {
                None => return Ok(Step::Asking),
                Some(Conflict::Skip) => return Ok(Step::Skipped),
                Some(Conflict::Rename) => {
                    name = conflict_name(&name, |n| dst.exists(&dst.join(&job.dest_dir, n)).unwrap_or(true));
                    dest = dst.join(&job.dest_dir, &name);
                    self.inner.queue.lock().unwrap().rename_to(id, &name);
                }
                Some(Conflict::Overwrite) => {
                    if same_path(&dest, &job.source) && job.direction == Direction::Copy {
                        // Sobrescribir un archivo consigo mismo: no hay nada que hacer.
                        return Ok(Step::Skipped);
                    }
                    if existing.is_dir != origin.is_dir {
                        dst.remove_all(&dest)?;
                    }
                }
            }
        }

        let mut copied = 0u64;
        let mut report = |n: u64| {
            copied += n;
            self.inner.queue.lock().unwrap().progress(id, copied);
        };
        copy_tree(src, &job.source, origin.is_dir, dst, &dest, &mut report, cancel)?;
        Ok(Step::Copied)
    }
}

enum Step {
    Copied,
    Asking,
    Skipped,
}

fn same_path(a: &str, b: &str) -> bool {
    a.trim_end_matches(['/', '\\'])
        .eq_ignore_ascii_case(b.trim_end_matches(['/', '\\']))
}

/// ¿`dir` es `base` o está dentro?
fn is_inside(dir: &str, base: &str) -> bool {
    let (d, b) = (dir.to_lowercase(), base.trim_end_matches(['/', '\\']).to_lowercase());
    d == b || d.starts_with(&format!("{b}/")) || d.starts_with(&format!("{b}\\"))
}

fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "cancelado")
}

/// Bytes de los archivos de una carpeta (recursivo).
fn tree_size(fs: &dyn FileSystem, dir: &str) -> io::Result<u64> {
    let mut total = 0;
    for e in fs.list(dir)?.entries {
        total += if e.is_dir { tree_size(fs, &e.path)? } else { e.size };
    }
    Ok(total)
}

fn copy_tree(
    src: &dyn FileSystem,
    from: &str,
    is_dir: bool,
    dst: &dyn FileSystem,
    to: &str,
    report: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
) -> io::Result<()> {
    if !is_dir {
        return copy_file(src, from, dst, to, report, cancel);
    }
    if dst.stat(to)?.is_none() {
        dst.create_dir(to)?;
    }
    for e in src.list(from)?.entries {
        if cancel.load(Ordering::SeqCst) {
            return Err(cancelled());
        }
        let target = dst.join(to, &e.name);
        copy_tree(src, &e.path, e.is_dir, dst, &target, report, cancel)?;
    }
    Ok(())
}

fn copy_file(
    src: &dyn FileSystem,
    from: &str,
    dst: &dyn FileSystem,
    to: &str,
    report: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
) -> io::Result<()> {
    let mut reader = src.open_read(from)?;
    let mut writer = dst.create(to)?;
    let result = (|| {
        let mut buf = vec![0u8; CHUNK];
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(cancelled());
            }
            let n = match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            writer.write_all(&buf[..n])?;
            report(n as u64);
        }
        writer.flush()
    })();
    drop(writer);
    if result.is_err() {
        // Nada a medias en el destino.
        let _ = dst.remove_file(to);
    }
    result
}

/// Error de E/S en palabras.
fn friendly(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::NotFound => format!("no existe ({e})"),
        io::ErrorKind::PermissionDenied => "sin permiso".into(),
        io::ErrorKind::Interrupted => "cancelado".into(),
        _ => e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryFs;
    use crate::fs::LocalFs;
    use std::time::{Duration, Instant};

    #[test]
    fn cola_estados_y_resumen() {
        let mut q = Queue::new();
        let a = q.push(Direction::Upload, "C:\\a.txt", "a.txt", "/srv");
        let b = q.push(Direction::Upload, "C:\\b.txt", "b.txt", "/srv");
        assert_eq!(q.take_next(), Some(a));
        assert_eq!(q.get(a).unwrap().state, JobState::Running);
        q.set_total(a, 100);
        q.progress(a, 40);
        assert!((q.get(a).unwrap().fraction() - 0.4).abs() < 1e-6);
        let s = q.summary();
        assert_eq!((s.active, s.done_bytes, s.total_bytes), (2, 40, 100));
        q.finish(a, Ok(()));
        assert_eq!(q.get(a).unwrap().state, JobState::Done);
        assert_eq!(q.get(a).unwrap().fraction(), 1.0);
        assert_eq!(q.take_next(), Some(b));
        q.finish(b, Err("sin permiso".into()));
        assert_eq!(q.get(b).unwrap().state, JobState::Failed("sin permiso".into()));
        assert!(!q.is_busy());
        let s = q.summary();
        assert_eq!((s.finished, s.failed), (2, 1));
        q.clear_finished();
        assert!(q.jobs().is_empty());
    }

    #[test]
    fn conflicto_pregunta_y_para_todos() {
        let mut q = Queue::new();
        let a = q.push(Direction::Upload, "a", "a", "/d");
        let b = q.push(Direction::Upload, "b", "b", "/d");
        let c = q.push(Direction::Upload, "c", "c", "/d");
        q.take_next();
        assert_eq!(q.on_conflict(a), None);
        assert_eq!(q.get(a).unwrap().state, JobState::Asking);
        q.take_next();
        assert_eq!(q.on_conflict(b), None);
        // Responder a uno solo.
        q.resolve(a, Conflict::Rename, false);
        assert_eq!(q.get(a).unwrap().state, JobState::Queued);
        assert_eq!(q.get(b).unwrap().state, JobState::Asking);
        assert_eq!(q.take_next(), Some(a));
        assert_eq!(q.on_conflict(a), Some(Conflict::Rename));
        // "Para todos": resuelve el que esperaba y los siguientes.
        q.resolve(b, Conflict::Skip, true);
        assert_eq!(q.get(b).unwrap().choice, Some(Conflict::Skip));
        assert_eq!(q.take_next(), Some(b));
        assert_eq!(q.take_next(), Some(c));
        assert_eq!(q.on_conflict(c), Some(Conflict::Skip));
    }

    #[test]
    fn cancelar_antes_y_durante() {
        let mut q = Queue::new();
        let a = q.push(Direction::Download, "/x", "x", "C:\\d");
        let b = q.push(Direction::Download, "/y", "y", "C:\\d");
        q.cancel(b);
        assert_eq!(q.get(b).unwrap().state, JobState::Cancelled);
        assert_eq!(q.take_next(), Some(a));
        q.cancel(a);
        assert_eq!(q.get(a).unwrap().state, JobState::Running, "termina el bloque en curso");
        q.finish(a, Err("cancelado".into()));
        assert_eq!(q.get(a).unwrap().state, JobState::Cancelled);
        assert_eq!(q.take_next(), None);
    }

    fn wait(what: &str, cond: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if cond() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no llegó: {what}");
    }

    fn tmp(name: &str) -> String {
        let d = std::env::temp_dir().join(format!("kuntur-tr-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.to_string_lossy().into_owned()
    }

    #[test]
    fn sube_archivo_y_carpeta_y_resuelve_conflictos() {
        let local: Arc<dyn FileSystem> = Arc::new(LocalFs);
        let mem = MemoryFs::new(Some("demo@servidor"), "/home/demo");
        let remote: Arc<dyn FileSystem> = Arc::new(mem.clone());
        let base = tmp("subir");
        let file = LocalFs.join(&base, "informe.txt");
        std::fs::write(&file, vec![7u8; 200_000]).unwrap();
        let dir = LocalFs.join(&base, "web");
        std::fs::create_dir_all(LocalFs.join(&dir, "css")).unwrap();
        std::fs::write(LocalFs.join(&dir, "index.html"), "<h1>").unwrap();
        std::fs::write(LocalFs.join(&LocalFs.join(&dir, "css"), "a.css"), "b{}").unwrap();

        let t = Transfers::new();
        let a = t.enqueue(Direction::Upload, local.clone(), &file, remote.clone(), "/home/demo");
        let b = t.enqueue(Direction::Upload, local.clone(), &dir, remote.clone(), "/srv/nueva");
        wait("subidas", || !t.is_busy());
        assert_eq!(t.job(a).unwrap().state, JobState::Done);
        assert_eq!(t.job(a).unwrap().done, 200_000);
        assert_eq!(mem.read("/home/demo/informe.txt").unwrap().len(), 200_000);
        assert_eq!(t.job(b).unwrap().state, JobState::Done);
        assert_eq!(t.job(b).unwrap().total, 7);
        assert_eq!(mem.read("/srv/nueva/web/css/a.css").unwrap(), b"b{}");

        // Otra vez: el destino existe → pregunta; renombrar guarda "informe (1).txt".
        let c = t.enqueue(Direction::Upload, local.clone(), &file, remote.clone(), "/home/demo");
        wait("pregunta", || t.job(c).unwrap().state == JobState::Asking);
        assert!(!t.is_busy(), "la pregunta no frena la cola");
        t.resolve(c, Conflict::Rename, false);
        wait("renombrado", || t.job(c).unwrap().state == JobState::Done);
        assert_eq!(t.job(c).unwrap().name, "informe (1).txt");
        assert!(mem.exists("/home/demo/informe (1).txt").unwrap());

        // Sobrescribir y omitir.
        std::fs::write(&file, "corto").unwrap();
        let d = t.enqueue(Direction::Upload, local.clone(), &file, remote.clone(), "/home/demo");
        wait("pregunta 2", || t.job(d).unwrap().state == JobState::Asking);
        t.resolve(d, Conflict::Overwrite, false);
        wait("sobrescrito", || t.job(d).unwrap().state == JobState::Done);
        assert_eq!(mem.read("/home/demo/informe.txt").unwrap(), b"corto");
        let e = t.enqueue(Direction::Upload, local.clone(), &file, remote.clone(), "/home/demo");
        wait("pregunta 3", || t.job(e).unwrap().state == JobState::Asking);
        t.resolve(e, Conflict::Skip, false);
        wait("omitido", || t.job(e).unwrap().state == JobState::Skipped);

        // Descargar al disco local.
        let out = tmp("bajar");
        let f = t.enqueue(Direction::Download, remote, "/srv/nueva/web", local, &out);
        wait("descarga", || t.job(f).unwrap().state == JobState::Done);
        let html = std::path::Path::new(&out).join("web").join("index.html");
        assert_eq!(std::fs::read_to_string(html).unwrap(), "<h1>");
        t.clear_finished();
        assert!(t.snapshot().is_empty());
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&out);
    }

    /// Lee de a poco: da tiempo a cancelar a mitad de camino.
    struct Slow(MemoryFs);

    struct SlowReader(Box<dyn Read + Send>);

    impl Read for SlowReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            std::thread::sleep(Duration::from_millis(15));
            let n = buf.len().min(4096);
            self.0.read(&mut buf[..n])
        }
    }

    impl FileSystem for Slow {
        fn style(&self) -> crate::PathStyle {
            self.0.style()
        }
        fn remote_label(&self) -> Option<String> {
            None
        }
        fn home(&self) -> io::Result<String> {
            self.0.home()
        }
        fn list(&self, dir: &str) -> io::Result<crate::Listing> {
            self.0.list(dir)
        }
        fn stat(&self, path: &str) -> io::Result<Option<crate::FileEntry>> {
            self.0.stat(path)
        }
        fn open_read(&self, path: &str) -> io::Result<Box<dyn Read + Send>> {
            Ok(Box::new(SlowReader(self.0.open_read(path)?)))
        }
        fn create(&self, path: &str) -> io::Result<Box<dyn Write + Send>> {
            self.0.create(path)
        }
        fn create_dir(&self, path: &str) -> io::Result<()> {
            self.0.create_dir(path)
        }
        fn rename(&self, from: &str, to: &str) -> io::Result<()> {
            self.0.rename(from, to)
        }
        fn remove_file(&self, path: &str) -> io::Result<()> {
            self.0.remove_file(path)
        }
        fn remove_dir(&self, path: &str) -> io::Result<()> {
            self.0.remove_dir(path)
        }
    }

    #[test]
    fn cancelar_borra_lo_copiado_a_medias() {
        let origin = MemoryFs::new(None, "/o");
        origin.add_file("/o/grande.bin", &vec![1u8; 1_000_000], 0);
        let dest = MemoryFs::new(None, "/d");
        let t = Transfers::new();
        let id = t.enqueue(
            Direction::Copy,
            Arc::new(Slow(origin)),
            "/o/grande.bin",
            Arc::new(dest.clone()),
            "/d",
        );
        wait("empezó", || t.job(id).unwrap().done > 0);
        t.cancel(id);
        wait("cancelado", || t.job(id).unwrap().state == JobState::Cancelled);
        assert!(t.job(id).unwrap().done < 1_000_000);
        assert!(!dest.exists("/d/grande.bin").unwrap(), "no queda el archivo a medias");
    }

    #[test]
    fn copiar_local_en_la_misma_carpeta_no_se_pisa() {
        let base = tmp("misma");
        let file = LocalFs.join(&base, "nota.md");
        std::fs::write(&file, "hola").unwrap();
        let fs: Arc<dyn FileSystem> = Arc::new(LocalFs);
        let t = Transfers::new();
        let id = t.enqueue(Direction::Copy, fs.clone(), &file, fs.clone(), &base);
        wait("pregunta", || t.job(id).unwrap().state == JobState::Asking);
        t.resolve(id, Conflict::Overwrite, false);
        wait("omitido", || t.job(id).unwrap().state == JobState::Skipped);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hola");
        // Una carpeta dentro de sí misma: error claro.
        let sub = LocalFs.join(&base, "sub");
        std::fs::create_dir_all(&sub).unwrap();
        let bad = t.enqueue(Direction::Copy, fs.clone(), &base, fs, &sub);
        wait("error", || matches!(t.job(bad).unwrap().state, JobState::Failed(_)));
        let _ = std::fs::remove_dir_all(&base);
    }
}
