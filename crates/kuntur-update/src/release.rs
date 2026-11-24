//! Elegir la actualización a partir de la respuesta de `GET /repos/{repo}/releases` de GitHub.
//!
//! Decisiones:
//! - Se consideran **también las versiones preliminares** (`prerelease`): Kuntur está en 0.x y sus
//!   versiones salen como preliminares; con solo `/releases/latest` (que las ignora) nadie vería la
//!   0.2.0. Los borradores (`draft`) nunca.
//! - Gana la versión semver más alta **mayor que la instalada**; el orden de la lista no importa.
//! - El instalador es el asset `kuntur-<versión>-x86_64.msi` (o, si no, el único `.msi`).
//! - SHA-256: el campo `digest` del asset (`sha256:<hex>`, lo calcula GitHub) y, si no está, la
//!   línea `SHA-256: \`<hex>\`` de la nota de la versión. Sin SHA-256 no se instala.

use semver::Version;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
    /// `sha256:<hex>` (GitHub lo agrega a los assets nuevos).
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    pub html_url: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

/// Una versión más nueva que la instalada, lista para descargar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub version: Version,
    /// Página de la versión (notas; en Linux/Mac se abre en el navegador).
    pub page: String,
    /// Instalador `.msi` (Windows), con su SHA-256 en minúsculas.
    pub installer: Option<Installer>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installer {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

/// Lee la respuesta JSON de la API (una lista de versiones).
pub fn parse_releases(json: &str) -> Result<Vec<Release>, serde_json::Error> {
    serde_json::from_str(json)
}

/// Versión de una etiqueta (`v0.2.0`, `0.2.0`, `v0.3.0-beta.1`).
pub fn parse_tag(tag: &str) -> Option<Version> {
    let t = tag.trim();
    Version::parse(t.strip_prefix('v').or_else(|| t.strip_prefix('V')).unwrap_or(t)).ok()
}

/// SHA-256 de un `digest` de GitHub (`sha256:<64 hex>`), en minúsculas.
pub fn parse_digest(digest: &str) -> Option<String> {
    let hex = digest.trim().strip_prefix("sha256:")?;
    is_sha256(hex).then(|| hex.to_ascii_lowercase())
}

/// SHA-256 escrito en la nota: la primera línea que contiene `SHA-256` (o `SHA256`) y 64 dígitos
/// hexadecimales seguidos.
pub fn sha_from_notes(body: &str) -> Option<String> {
    body.lines()
        .filter(|l| {
            let u = l.to_ascii_uppercase();
            u.contains("SHA-256") || u.contains("SHA256")
        })
        .find_map(|l| {
            l.split(|c: char| !c.is_ascii_hexdigit())
                .find(|w| is_sha256(w))
                .map(str::to_ascii_lowercase)
        })
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// El instalador de una versión, con su SHA-256 (sin SHA-256 no hay instalador).
fn installer_of(release: &Release, version: &Version) -> Option<Installer> {
    let expected = format!("kuntur-{version}-x86_64.msi");
    let msis: Vec<&Asset> = release
        .assets
        .iter()
        .filter(|a| a.name.to_ascii_lowercase().ends_with(".msi"))
        .collect();
    let asset = msis
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(&expected))
        .or_else(|| (msis.len() == 1).then(|| &msis[0]))?;
    let sha256 = asset
        .digest
        .as_deref()
        .and_then(parse_digest)
        .or_else(|| release.body.as_deref().and_then(sha_from_notes))?;
    Some(Installer {
        name: asset.name.clone(),
        url: asset.browser_download_url.clone(),
        size: asset.size,
        sha256,
    })
}

/// La actualización a ofrecer: la versión publicada más alta que sea mayor que `current`.
pub fn choose_update(releases: &[Release], current: &str) -> Option<Update> {
    let current = parse_tag(current)?;
    releases
        .iter()
        .filter(|r| !r.draft)
        .filter_map(|r| parse_tag(&r.tag_name).map(|v| (v, r)))
        .filter(|(v, _)| *v > current)
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(version, r)| Update {
            installer: installer_of(r, &version),
            page: r.html_url.clone(),
            version,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "4aaa8dababf8425976c2a2b64a3c2238e0f71c03382b405002190c5f0ff9d4c0";

    fn release(tag: &str, prerelease: bool, draft: bool, assets: Vec<Asset>, body: &str) -> Release {
        Release {
            tag_name: tag.into(),
            name: None,
            draft,
            prerelease,
            html_url: format!("https://github.com/demo/kuntur/releases/tag/{tag}"),
            body: Some(body.into()),
            assets,
        }
    }

    fn msi(version: &str, digest: Option<&str>) -> Asset {
        Asset {
            name: format!("kuntur-{version}-x86_64.msi"),
            browser_download_url: format!(
                "https://github.com/demo/kuntur/releases/download/v{version}/kuntur-{version}-x86_64.msi"
            ),
            size: 13_508_608,
            digest: digest.map(String::from),
        }
    }

    #[test]
    fn lee_la_respuesta_de_github() {
        let json = format!(
            r#"[{{"tag_name":"v0.2.0","name":"Kuntur 0.2.0","draft":false,"prerelease":true,
                 "html_url":"https://github.com/demo/kuntur/releases/tag/v0.2.0","body":"notas",
                 "assets":[{{"name":"kuntur-0.2.0-x86_64.msi","size":10,"digest":"sha256:{SHA}",
                   "browser_download_url":"https://github.com/demo/x.msi","otro":1}}],"id":5}}]"#
        );
        let r = parse_releases(&json).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].assets[0].digest.as_deref(), Some(format!("sha256:{SHA}").as_str()));
    }

    #[test]
    fn etiquetas_y_digest() {
        assert_eq!(parse_tag("v0.2.0"), Some(Version::new(0, 2, 0)));
        assert_eq!(parse_tag("0.10.1"), Some(Version::new(0, 10, 1)));
        assert!(parse_tag("v0.3.0-beta.1").is_some());
        assert_eq!(parse_tag("nightly"), None);
        assert_eq!(
            parse_digest(&format!("sha256:{}", SHA.to_uppercase())),
            Some(SHA.into())
        );
        assert_eq!(parse_digest("sha1:abc"), None);
        assert_eq!(parse_digest("sha256:corto"), None);
    }

    #[test]
    fn sha_de_la_nota() {
        let body = format!("## Instalar\n1. Descarga…\n\nSHA-256: `{}`\n", SHA.to_uppercase());
        assert_eq!(sha_from_notes(&body), Some(SHA.into()));
        assert_eq!(sha_from_notes("sin hash"), None);
        // Un hash en otra línea no cuenta.
        assert_eq!(sha_from_notes(&format!("commit {SHA}")), None);
    }

    #[test]
    fn elige_la_mas_alta_mayor_que_la_instalada_incluidas_preliminares() {
        let list = vec![
            release(
                "v0.1.0",
                true,
                false,
                vec![msi("0.1.0", Some(&format!("sha256:{SHA}")))],
                "",
            ),
            release(
                "v0.3.0",
                false,
                true,
                vec![msi("0.3.0", Some(&format!("sha256:{SHA}")))],
                "",
            ), // borrador
            release(
                "v0.2.0",
                true,
                false,
                vec![msi("0.2.0", Some(&format!("sha256:{SHA}")))],
                "",
            ),
            release(
                "v0.1.5",
                false,
                false,
                vec![msi("0.1.5", Some(&format!("sha256:{SHA}")))],
                "",
            ),
        ];
        let u = choose_update(&list, "0.1.0").unwrap();
        assert_eq!(u.version, Version::new(0, 2, 0));
        let i = u.installer.unwrap();
        assert_eq!(i.name, "kuntur-0.2.0-x86_64.msi");
        assert_eq!(i.sha256, SHA);
        assert!(u.page.ends_with("/v0.2.0"));
        // Ya al día: nada que ofrecer.
        assert_eq!(choose_update(&list, "0.2.0"), None);
        assert_eq!(choose_update(&list, "1.0.0"), None);
    }

    #[test]
    fn sin_digest_usa_la_nota_y_sin_hash_no_hay_instalador() {
        let body = format!("SHA-256: `{SHA}`");
        let con_nota = vec![release("v0.2.0", false, false, vec![msi("0.2.0", None)], &body)];
        assert_eq!(
            choose_update(&con_nota, "0.1.0").unwrap().installer.unwrap().sha256,
            SHA
        );

        let sin_hash = vec![release("v0.2.0", false, false, vec![msi("0.2.0", None)], "notas")];
        let u = choose_update(&sin_hash, "0.1.0").unwrap();
        assert_eq!(u.installer, None, "sin SHA-256 no se ofrece instalar");
    }

    #[test]
    fn elige_el_msi_por_nombre_o_el_unico() {
        let mut otro = msi("0.2.0", Some(&format!("sha256:{SHA}")));
        otro.name = "Kuntur-Setup.msi".into();
        let zip = Asset {
            name: "fuentes.zip".into(),
            browser_download_url: "https://github.com/demo/f.zip".into(),
            size: 1,
            digest: None,
        };
        let list = vec![release("v0.2.0", false, false, vec![zip.clone(), otro], "")];
        assert_eq!(
            choose_update(&list, "0.1.0").unwrap().installer.unwrap().name,
            "Kuntur-Setup.msi"
        );
        let sin_msi = vec![release("v0.2.0", false, false, vec![zip], "")];
        assert_eq!(choose_update(&sin_msi, "0.1.0").unwrap().installer, None);
    }
}
