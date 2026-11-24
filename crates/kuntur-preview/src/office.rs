//! LibreOffice (opcional): convierte PPTX/DOCX/XLSX a PDF en modo headless. Solo se usa si está
//! instalado; si no, la vista previa ofrece "Abrir con su programa".

use std::path::{Path, PathBuf};

/// Busca `soffice.exe` en el PATH y en las carpetas de instalación habituales.
pub fn find_libreoffice() -> Option<PathBuf> {
    find_in(
        std::env::var_os("PATH").as_deref(),
        &[
            std::env::var_os("ProgramFiles").map(PathBuf::from),
            std::env::var_os("ProgramFiles(x86)").map(PathBuf::from),
        ],
    )
}

fn find_in(path_var: Option<&std::ffi::OsStr>, program_dirs: &[Option<PathBuf>]) -> Option<PathBuf> {
    let from_path = path_var
        .into_iter()
        .flat_map(std::env::split_paths)
        .map(|d| d.join("soffice.exe"));
    let installed = program_dirs
        .iter()
        .flatten()
        .map(|d| d.join("LibreOffice").join("program").join("soffice.exe"));
    from_path.chain(installed).find(|p| p.is_file())
}

/// Convierte `input` a PDF dentro de `out_dir` y devuelve la ruta del PDF.
pub fn convert_to_pdf(soffice: &Path, input: &Path, out_dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(out_dir)?;
    let status = crate::hidden_command(soffice)
        .args(["--headless", "--norestore", "--convert-to", "pdf", "--outdir"])
        .arg(out_dir)
        .arg(input)
        .status()?;
    let pdf = out_dir.join(input.with_extension("pdf").file_name().unwrap_or_default());
    if status.success() && pdf.is_file() {
        Ok(pdf)
    } else {
        Err(std::io::Error::other("LibreOffice no pudo convertir el archivo"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busca_en_path_y_en_program_files() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(find_in(None, &[Some(dir.path().to_path_buf())]), None);
        let program = dir.path().join("LibreOffice").join("program");
        std::fs::create_dir_all(&program).unwrap();
        std::fs::write(program.join("soffice.exe"), b"").unwrap();
        assert_eq!(
            find_in(None, &[None, Some(dir.path().to_path_buf())]),
            Some(program.join("soffice.exe"))
        );
        let path = std::env::join_paths([program.clone()]).unwrap();
        assert_eq!(find_in(Some(&path), &[]), Some(program.join("soffice.exe")));
    }
}
