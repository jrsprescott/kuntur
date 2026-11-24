//! Recursos de Windows de `kuntur.exe` (F11): ícono (recurso 1, el que GPUI usa para la ventana y
//! el Explorador muestra) y la información de versión (nombre en el Administrador de tareas).

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let icon = manifest.join("../../assets/kuntur.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    let mut parts: Vec<u16> = version
        .split(['.', '-', '+'])
        .take(3)
        .map(|p| p.parse().unwrap_or(0))
        .collect();
    parts.resize(4, 0);
    let numeric = parts.iter().map(u16::to_string).collect::<Vec<_>>().join(",");
    // rc.exe acepta "/" en las rutas; las comillas y barras invertidas no se escapan bien.
    let icon_path = icon
        .canonicalize()
        .unwrap_or(icon)
        .display()
        .to_string()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    let rc = format!(
        r#"#pragma code_page(65001)
1 ICON "{icon_path}"

1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Kuntur"
      VALUE "FileDescription", "Kuntur"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "kuntur"
      VALUE "LegalCopyright", "MIT OR Apache-2.0"
      VALUE "OriginalFilename", "kuntur.exe"
      VALUE "ProductName", "Kuntur"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("kuntur.rc");
    std::fs::write(&out, rc).expect("no se pudo escribir kuntur.rc");
    embed_resource::compile(&out, embed_resource::NONE)
        .manifest_optional()
        .expect("no se pudo compilar el recurso de Windows (rc.exe del Windows SDK)");
}
