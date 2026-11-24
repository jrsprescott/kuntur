//! Comprobación manual de los lectores contra el historial real de este equipo.
//! Ignorada en CI; imprime SOLO totales numéricos agregados (nunca títulos, rutas ni contenido):
//!
//! ```text
//! cargo test -p kuntur-usage --test datos_reales -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use kuntur_usage::{AgentKind, Period, UsageRecord, claude, codex, opencode, summarize};

fn home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn report(agent: AgentKind, records: &[UsageRecord]) {
    let sum = |f: fn(&UsageRecord) -> u64| records.iter().map(f).sum::<u64>();
    println!(
        "{:<12} registros={:>7} entrada={:>12} salida={:>12} cache_lect={:>14} cache_escr={:>12}",
        agent.name(),
        records.len(),
        sum(|r| r.input),
        sum(|r| r.output),
        sum(|r| r.cache_read),
        sum(|r| r.cache_write),
    );
}

/// Mide la importación incremental contra el historial real, en una base temporal (se borra al
/// terminar). Imprime solo tiempos y cantidades.
#[test]
#[ignore = "lee el historial real del usuario; solo para comprobación manual"]
fn importacion_incremental_del_historial_real() {
    use kuntur_usage::store::{Sources, UsageStore};
    let db = std::env::temp_dir().join(format!("kuntur-prueba-usage-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&db);
    let sources = Sources::under_home(&home());
    let t = std::time::Instant::now();
    let mut store = UsageStore::open(&db).unwrap();
    let first = store.import(&sources).unwrap();
    let n = store.records().unwrap().len();
    println!(
        "primera importación: {:?} en {:.2} s ({n} registros)",
        first,
        t.elapsed().as_secs_f64()
    );
    let t = std::time::Instant::now();
    let second = store.import(&sources).unwrap();
    let again = store.records().unwrap();
    println!(
        "segunda importación: {:?} en {:.2} s",
        second,
        t.elapsed().as_secs_f64()
    );
    let direct = kuntur_usage::load_local(&home()).len();
    println!("registros: almacén={} lectura directa={direct}", again.len());
    drop(store);
    let _ = std::fs::remove_file(&db);
}

#[test]
#[ignore = "lee el historial real del usuario; solo para comprobación manual"]
fn totales_del_historial_real() {
    let home = home();
    let claude = claude::read_dir(&home.join(".claude").join("projects"));
    let codex = codex::read_dir(&home.join(".codex").join("sessions"));
    let opencode = opencode::read_db(&home.join(".local").join("share").join("opencode").join("opencode.db"));
    report(AgentKind::ClaudeCode, &claude);
    report(AgentKind::Codex, &codex);
    report(AgentKind::OpenCode, &opencode);
    let limits = codex::latest_limits(&home.join(".codex").join("sessions"));
    println!("límites de Codex disponibles: {}", limits.is_some());

    let all: Vec<UsageRecord> = claude.into_iter().chain(codex).chain(opencode).collect();
    let now = jiff::Timestamp::now().as_second();
    let tz = jiff::tz::TimeZone::system();
    for period in [Period::Days7, Period::Days30, Period::All] {
        let s = summarize(&all, period, now, &tz);
        println!(
            "{period:?}: turnos={} sesiones={} días={} modelos={} tiempo_agente_min={}",
            s.totals.turns,
            s.totals.sessions,
            s.by_day.len(),
            s.models.len(),
            s.totals.agent_time_secs / 60
        );
    }
}
