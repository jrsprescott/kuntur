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

/// De punta a punta como la app instalada en 0.1.0: consulta GitHub, elige la versión nueva, descarga
/// el MSI de verdad y comprueba su SHA-256 y su firma. No instala nada. Solo a mano:
/// `cargo test -p kuntur-update --test github_real -- --ignored --nocapture`.
#[test]
#[ignore = "usa la red y descarga el instalador: solo a mano"]
fn desde_la_0_1_0_descarga_y_verifica_la_nueva() {
    use std::sync::atomic::AtomicBool;
    let list = kuntur_update::net::fetch_releases(&kuntur_update::releases_url()).expect("consulta a GitHub");
    let u = kuntur_update::choose_update(&list, "0.1.0").expect("hay una versión más nueva que la 0.1.0");
    let installer = u.installer.expect("con instalador .msi y SHA-256");
    eprintln!("0.1.0 → {} ({}, {} bytes)", u.version, installer.name, installer.size);
    let dir = std::env::temp_dir().join(format!("kuntur-update-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut last = 0;
    let path = kuntur_update::net::download(&installer, &dir, &AtomicBool::new(false), |n, _| last = n)
        .expect("descarga con el SHA-256 correcto");
    assert_eq!(last, installer.size, "se descargó completo");
    assert_eq!(
        kuntur_update::net::sha256_file(&path).unwrap(),
        installer.sha256.to_ascii_lowercase()
    );
    eprintln!("firma: {:?}", kuntur_update::signature::inspect(&path));
    let _ = std::fs::remove_dir_all(&dir);
}
