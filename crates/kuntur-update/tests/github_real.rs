//! Comprobación manual contra la API real de GitHub (una petición anónima). No corre en
//! `cargo test`: `cargo test -p kuntur-update --test github_real -- --ignored --nocapture`.

#[test]
#[ignore = "usa la red: solo a mano"]
fn lee_las_versiones_publicadas() {
    let list = kuntur_update::net::fetch_releases(&kuntur_update::releases_url()).expect("consulta a GitHub");
    assert!(!list.is_empty(), "el repositorio tiene al menos la 0.1.0");
    for r in &list {
        eprintln!("{} prerelease={} assets={}", r.tag_name, r.prerelease, r.assets.len());
    }
    // Desde 0.0.1 siempre hay algo que ofrecer, con su instalador y SHA-256.
    let u = kuntur_update::choose_update(&list, "0.0.1").expect("hay una versión más nueva");
    let i = u.installer.expect("con instalador .msi y SHA-256");
    eprintln!(
        "ofrece {} → {} ({} bytes, sha256 {})",
        u.version, i.name, i.size, i.sha256
    );
    assert_eq!(i.sha256.len(), 64);
}
