//! Mide la memoria que ocupan los datos de Uso y Auditoría con el historial real de esta PC.
//! Solo imprime totales (cantidades y bytes), nunca comandos, títulos ni rutas.
//!
//! cargo run --release -p kuntur-audit --example medir_memoria

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

struct Contador;
static VIVO: AtomicUsize = AtomicUsize::new(0);
static PICO: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Contador {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // SAFETY: se delega en el asignador del sistema con el mismo `Layout`.
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            let v = VIVO.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            PICO.fetch_max(v, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        VIVO.fetch_sub(l.size(), Ordering::Relaxed);
        // SAFETY: `p` vino de `alloc` con este mismo `Layout`.
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Contador = Contador;

fn mb(b: usize) -> String {
    format!("{:.1} MB", b as f64 / 1_048_576.0)
}

fn main() {
    let home = std::path::PathBuf::from(std::env::var_os("USERPROFILE").expect("USERPROFILE"));
    let appdata = std::path::PathBuf::from(std::env::var_os("APPDATA").expect("APPDATA"));
    let now = jiff::Timestamp::now().as_second();
    let since = kuntur_audit::window_start(now, &jiff::tz::TimeZone::system());

    // Uso: lo mismo que hace la app (almacén incremental + registros).
    let base = VIVO.load(Ordering::Relaxed);
    PICO.store(base, Ordering::Relaxed);
    let t = Instant::now();
    let mut store = kuntur_usage::store::UsageStore::open(&appdata.join("Kuntur").join("usage.db")).unwrap();
    store.import(&kuntur_usage::store::Sources::under_home(&home)).unwrap();
    let records = store.records_since(since).unwrap();
    drop(store);
    println!(
        "Uso: {} registros · retenido {} · pico {} · {} ms",
        records.len(),
        mb(VIVO.load(Ordering::Relaxed) - base),
        mb(PICO.load(Ordering::Relaxed) - base),
        t.elapsed().as_millis()
    );
    drop(records);

    // Auditoría: caché + copia de registros (lo que guarda la app).
    let base = VIVO.load(Ordering::Relaxed);
    PICO.store(base, Ordering::Relaxed);
    let t = Instant::now();
    let mut cache = kuntur_audit::AuditCache::new();
    cache.refresh(&kuntur_audit::Sources::under_home(&home), since);
    let en_cache = VIVO.load(Ordering::Relaxed) - base;
    let records = cache.records();
    println!(
        "Auditoría: {} comandos · caché {} · caché + lista {} · pico {} · {} ms",
        records.len(),
        mb(en_cache),
        mb(VIVO.load(Ordering::Relaxed) - base),
        mb(PICO.load(Ordering::Relaxed) - base),
        t.elapsed().as_millis()
    );
}
